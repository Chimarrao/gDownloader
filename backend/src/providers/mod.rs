use anyhow::Result;
use crate::models::FileInfo;
use futures_util::StreamExt;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::{future::Future, pin::Pin};
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio::time::{sleep, Duration, Instant};

// Declara os sub-módulos de cada provedor de download
// Em PHP seria algo como: require_once 'providers/MegaProvider.php';
pub mod gdrive;
pub mod fichier;
pub mod drime;
pub mod mediafire;
pub mod mega;
pub mod pixeldrain;
pub mod sharepoint;
pub mod terabox;

pub trait ProviderDefaults {
    fn safe_filename(name: &str, fallback: &str) -> String
    where
        Self: Sized,
    {
        let trimmed = name.trim();
        let candidate = if trimmed.is_empty() { fallback } else { trimmed };
        candidate
            .chars()
            .map(|ch| match ch {
                '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
                _ => ch,
            })
            .collect()
    }

    fn http_client() -> Result<reqwest::Client>
    where
        Self: Sized,
    {
        Ok(reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36")
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()?)
    }

    fn response_total_bytes(resp: &reqwest::Response, resumed_bytes: u64) -> u64
    where
        Self: Sized,
    {
        resp.headers()
            .get("content-range")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split('/').last())
            .and_then(|value| value.parse::<u64>().ok())
            .or_else(|| resp.content_length().map(|len| len + resumed_bytes))
            .unwrap_or(0)
    }

    fn send_with_resume_fallback<'a>(
        client: &'a reqwest::Client,
        url: &'a str,
        dest_path: &'a str,
        existing_bytes: u64,
    ) -> Pin<Box<dyn Future<Output = Result<(reqwest::Response, bool)>> + Send + 'a>>
    where
        Self: Sized,
    {
        Box::pin(async move {
            if existing_bytes == 0 {
                return Ok((client.get(url).send().await?.error_for_status()?, false));
            }

            let ranged = client
                .get(url)
                .header("Range", format!("bytes={existing_bytes}-"))
                .send()
                .await?;

            if ranged.status() == reqwest::StatusCode::PARTIAL_CONTENT {
                return Ok((ranged, true));
            }

            if matches!(
                ranged.status(),
                reqwest::StatusCode::BAD_REQUEST | reqwest::StatusCode::RANGE_NOT_SATISFIABLE
            ) {
                let _ = tokio::fs::remove_file(dest_path).await;
                let fresh = client.get(url).send().await?.error_for_status()?;
                return Ok((fresh, false));
            }

            Ok((ranged.error_for_status()?, false))
        })
    }
}

// Estrutura de atualização de progresso enviada pelo provider durante o download
// Usada para calcular velocidade (bytes/s) e ETA no handler de downloads
#[derive(Debug, Clone)]
pub struct ProgressUpdate {
    pub bytes_downloaded: u64,
    pub total_bytes: u64, // 0 se o servidor não informar Content-Length
    pub child_path: Option<String>,
    pub child_filename: Option<String>,
    pub child_bytes_downloaded: Option<u64>,
    pub child_total_bytes: Option<u64>,
    pub child_speed_bps: Option<u64>,
    pub child_eta_secs: Option<u64>,
}

pub async fn apply_speed_limit(
    started_at: Instant,
    bytes_downloaded: u64,
    speed_limit_bps: Option<u64>,
) {
    let Some(limit) = speed_limit_bps else {
        return;
    };
    if limit == 0 {
        return;
    }

    let expected_elapsed = bytes_downloaded as f64 / limit as f64;
    let actual_elapsed = started_at.elapsed().as_secs_f64();
    if expected_elapsed > actual_elapsed {
        sleep(Duration::from_secs_f64(expected_elapsed - actual_elapsed)).await;
    }
}

fn parse_content_range_total(resp: &reqwest::Response) -> Option<u64> {
    resp.headers()
        .get("content-range")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split('/').last())
        .and_then(|value| value.parse::<u64>().ok())
}

pub async fn try_parallel_download(
    client: &reqwest::Client,
    url: &str,
    dest_path: &str,
    speed_limit_bps: Option<u64>,
    parallel_parts: usize,
    progress_tx: tokio::sync::mpsc::Sender<ProgressUpdate>,
) -> Result<Option<u64>> {
    if parallel_parts <= 1 {
        return Ok(None);
    }

    let probe = client
        .get(url)
        .header("Range", "bytes=0-0")
        .send()
        .await?;

    if probe.status() != reqwest::StatusCode::PARTIAL_CONTENT {
        return Ok(None);
    }

    let total_bytes = parse_content_range_total(&probe).unwrap_or(0);
    if total_bytes == 0 {
        return Ok(None);
    }

    const MIN_PART_SIZE: u64 = 2 * 1024 * 1024;
    let max_useful_parts = (total_bytes / MIN_PART_SIZE).max(1) as usize;
    let part_count = parallel_parts.min(max_useful_parts).max(1);
    if part_count <= 1 {
        return Ok(None);
    }

    let part_dir = format!("{dest_path}.parts");
    let _ = tokio::fs::remove_dir_all(&part_dir).await;
    tokio::fs::create_dir_all(&part_dir).await?;

    let started_at = Instant::now();
    let total_downloaded = Arc::new(AtomicU64::new(0));
    let mut tasks = Vec::with_capacity(part_count);

    // Each task gets an equal share of the global limit.
    // If the share would be smaller than one 64KB piece/s, cap it at 65_536 to avoid
    // second-long stalls inside each task at very low speeds.
    let task_limit = speed_limit_bps.map(|l| (l / part_count as u64).max(65_536));

    for part_index in 0..part_count {
        let client = client.clone();
        let url = url.to_string();
        let part_dir = part_dir.clone();
        let progress_tx = progress_tx.clone();
        let total_downloaded = Arc::clone(&total_downloaded);
        let start = (total_bytes * part_index as u64) / part_count as u64;
        let end = ((total_bytes * (part_index as u64 + 1)) / part_count as u64).saturating_sub(1);

        tasks.push(tokio::spawn(async move {
            let part_path = format!("{part_dir}/part-{part_index:03}");
            let resp = client
                .get(&url)
                .header("Range", format!("bytes={start}-{end}"))
                .send()
                .await?
                .error_for_status()?;

            if resp.status() != reqwest::StatusCode::PARTIAL_CONTENT {
                return Err(anyhow::anyhow!("Servidor não aceitou download em partes"));
            }

            let mut file = tokio::fs::File::create(&part_path).await?;
            let mut stream = resp.bytes_stream();
            let mut task_session_downloaded = 0u64;

            while let Some(chunk) = stream.next().await {
                let chunk = chunk?;
                for piece in chunk.chunks(65_536) {
                    file.write_all(piece).await?;
                    let piece_len = piece.len() as u64;
                    task_session_downloaded += piece_len;

                    let downloaded = total_downloaded.fetch_add(piece_len, Ordering::Relaxed)
                        + piece_len;

                    let _ = progress_tx
                        .send(ProgressUpdate {
                            bytes_downloaded: downloaded,
                            total_bytes,
                            child_path: None,
                            child_filename: None,
                            child_bytes_downloaded: None,
                            child_total_bytes: None,
                            child_speed_bps: None,
                            child_eta_secs: None,
                        })
                        .await;

                    apply_speed_limit(started_at, task_session_downloaded, task_limit).await;
                }
            }

            file.flush().await?;
            Ok::<(), anyhow::Error>(())
        }));
    }

    for task in tasks {
        task.await??;
    }

    let _ = tokio::fs::remove_file(dest_path).await;
    let mut output = tokio::fs::File::create(dest_path).await?;
    for part_index in 0..part_count {
        let part_path = format!("{part_dir}/part-{part_index:03}");
        let mut part = OpenOptions::new().read(true).open(&part_path).await?;
        tokio::io::copy(&mut part, &mut output).await?;
    }
    output.flush().await?;
    let _ = tokio::fs::remove_dir_all(&part_dir).await;

    Ok(Some(total_bytes))
}

// Trait = como uma interface PHP — define o contrato que todo provider deve seguir
// Send + Sync = restrições de thread-safety: o provider pode ser movido entre threads
//              e compartilhado por referência entre threads — obrigatório com tokio
pub trait Provider: Send + Sync + ProviderDefaults {
    // Retorna o nome legível do provider para logs e exibição na UI
    fn name(&self) -> &str;

    // Busca metadados do arquivo (nome, tamanho, MIME) sem baixar o conteúdo
    //
    // A assinatura complexa é necessária porque Rust não suporta `async fn` em traits
    // diretamente (ainda). Pin<Box<dyn Future>> é o equivalente manual de uma Promise:
    //   - Box<dyn Future> = heap-allocated, dyn Future = qualquer Future (como interface JS)
    //   - Pin = garante que o Future não seja movido na memória (requisito do async runtime)
    //   - Send = pode ser enviado para outra thread (necessário para tokio)
    //   - 'a = lifetime: o Future não pode viver mais que &self e &url
    fn get_file_info<'a>(
        &'a self,
        url: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FileInfo>> + Send + 'a>>;

    // Baixa o arquivo para dest_path e envia atualizações de progresso pelo canal
    // Result<u64> = Ok(bytes_baixados) ou Err(motivo_do_erro) — como try/catch mas em forma de valor
    fn download<'a>(
        &'a self,
        url: &'a str,
        dest_path: &'a str,
        speed_limit_bps: Option<u64>,
        parallel_parts: usize,
        selected_children: Option<Vec<String>>,
        // Sender do canal de progresso — o provider envia, o handler recebe
        // mpsc = Multiple Producer, Single Consumer (como uma fila de mensagens)
        progress_tx: tokio::sync::mpsc::Sender<ProgressUpdate>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u64>> + Send + 'a>>;
}

// Detecta qual provider consegue lidar com a URL fornecida
// Retorna Box<dyn Provider> = heap-allocated, dyn Provider = qualquer tipo que implemente
// Provider (como type hinting de interface no PHP: function foo(ProviderInterface $p))
// Option<T> = pode ser Some(provider) ou None — como nullable no PHP (Provider|null)
pub fn detect_provider(url: &str) -> Option<Box<dyn Provider>> {
    // Testa cada provider na ordem de prioridade
    // O primeiro que reconhecer a URL vence — sem overlap entre eles
    if mega::MegaProvider::matches(url) {
        return Some(Box::new(mega::MegaProvider));
    }
    if mediafire::MediaFireProvider::matches(url) {
        return Some(Box::new(mediafire::MediaFireProvider));
    }
    if drime::DrimeProvider::matches(url) {
        return Some(Box::new(drime::DrimeProvider));
    }
    if fichier::FichierProvider::matches(url) {
        return Some(Box::new(fichier::FichierProvider));
    }
    if terabox::TeraboxProvider::matches(url) {
        return Some(Box::new(terabox::TeraboxProvider));
    }
    if sharepoint::SharePointProvider::matches(url) {
        return Some(Box::new(sharepoint::SharePointProvider));
    }
    if gdrive::GDriveProvider::matches(url) {
        return Some(Box::new(gdrive::GDriveProvider));
    }
    if pixeldrain::PixelDrainProvider::matches(url) {
        return Some(Box::new(pixeldrain::PixelDrainProvider));
    }
    // URL não reconhecida por nenhum provider suportado
    None
}
