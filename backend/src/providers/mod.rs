use anyhow::{anyhow, Result};
use crate::models::FileInfo;
use futures_util::StreamExt;
use serde::Serialize;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, OnceLock, RwLock,
};
use std::{future::Future, pin::Pin};
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio::time::{sleep, Duration, Instant};

struct ProxyConfig {
    mode: String,
    host: String,
    port: u16,
    username: Option<String>,
    password: Option<String>,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self { mode: "none".to_string(), host: String::new(), port: 0, username: None, password: None }
    }
}

static GLOBAL_PROXY: OnceLock<RwLock<ProxyConfig>> = OnceLock::new();

pub fn update_global_proxy(mode: String, host: String, port: u16, username: Option<String>, password: Option<String>) {
    let lock = GLOBAL_PROXY.get_or_init(|| RwLock::new(ProxyConfig::default()));
    if let Ok(mut proxy) = lock.write() {
        *proxy = ProxyConfig { mode, host, port, username, password };
    }
}

// Declara os sub-módulos de cada provedor de download
// Em PHP seria algo como: require_once 'providers/MegaProvider.php';
pub mod gdrive;
#[path = "1fichier.rs"]
pub mod fichier;
pub mod drime;
pub mod rapidgator;
pub mod brupload;
pub mod brfiles;
pub mod moondl;
pub mod akirabox;
pub mod katfile;
pub mod direct_http;
pub mod mediafire;
pub mod mega;
pub mod pixeldrain;
pub mod sharepoint;
pub mod terabox;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCapabilities {
    pub max_parallel_downloads_free: Option<usize>,
    pub requires_browser_helper: bool,
    pub supports_folder: bool,
    pub supports_manual_auth: bool,
    pub supports_auto_captcha: bool,
    pub free_cooldown_secs: Option<u64>,
    pub requires_account_for_large_files: bool,
    pub supports_parallel_parts: bool,
}

impl Default for ProviderCapabilities {
    fn default() -> Self {
        Self {
            max_parallel_downloads_free: None,
            requires_browser_helper: false,
            supports_folder: false,
            supports_manual_auth: false,
            supports_auto_captcha: false,
            free_cooldown_secs: None,
            requires_account_for_large_files: false,
            supports_parallel_parts: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProviderAccountState {
    pub connected: bool,
    pub verified_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDescriptor {
    pub id: &'static str,
    pub name: &'static str,
    pub color: &'static str,
    pub capabilities: ProviderCapabilities,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_state: Option<ProviderAccountState>,
}

pub const DEFAULT_USER_AGENT: &str =
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/136 Safari/537.36";
pub const DEFAULT_ACCEPT_LANGUAGE: &str = "pt-BR,pt;q=0.9,en;q=0.8";

pub fn sanitize_filename(name: &str, fallback: &str) -> String {
    let trimmed = name.trim();
    let candidate = if trimmed.is_empty() { fallback } else { trimmed };
    candidate
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => ch,
        })
        .collect::<String>()
        .trim()
        .to_string()
}

pub fn parse_human_size(value: &str) -> u64 {
    let normalized = value.trim().replace(',', ".").to_uppercase();
    let Some(captures) = regex::Regex::new(r#"([0-9]+(?:\.[0-9]+)?)\s*(B|KB|MB|GB|TB)"#)
        .ok()
        .and_then(|re| re.captures(&normalized))
    else {
        return 0;
    };

    let number = captures[1].parse::<f64>().unwrap_or(0.0);
    let multiplier = match &captures[2] {
        "KB" => 1024.0,
        "MB" => 1024.0 * 1024.0,
        "GB" => 1024.0 * 1024.0 * 1024.0,
        "TB" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => 1.0,
    };

    (number * multiplier).round() as u64
}

pub fn extract_wait_seconds_from_text(value: &str) -> Option<u64> {
    let lower = value.to_lowercase();
    let patterns = [
        (r#"(\d+)\s*(?:hora|horas|hour|hours)"#, 3600u64),
        (r#"(\d+)\s*(?:minuto|minutos|minute|minutes)"#, 60u64),
        (r#"(\d+)\s*(?:segundo|segundos|second|seconds)"#, 1u64),
    ];

    let mut total = 0u64;
    let mut matched = false;

    for (pattern, multiplier) in patterns {
        let Some(re) = regex::Regex::new(pattern).ok() else {
            continue;
        };
        for captures in re.captures_iter(&lower) {
            total = total.saturating_add(
                captures[1]
                    .parse::<u64>()
                    .unwrap_or(0)
                    .saturating_mul(multiplier),
            );
            matched = true;
        }
    }

    if matched && total > 0 {
        return Some(total);
    }

    if let Some(captures) = regex::Regex::new(r#"(?:wait|aguarde|try again in)\s*(\d+)"#)
        .ok()
        .and_then(|re| re.captures(&lower))
    {
        return captures[1].parse::<u64>().ok();
    }

    None
}

pub fn extract_fragment_value(url: &str, key: &str) -> Option<String> {
    let fragment = url.split('#').nth(1)?;
    for part in fragment.split('&') {
        if let Some(value) = part.strip_prefix(&format!("{key}=")) {
            return Some(value.to_string());
        }
    }
    None
}

pub fn parse_url(url: &str) -> Option<reqwest::Url> {
    let clean = url.split('#').next().unwrap_or(url);
    reqwest::Url::parse(clean).ok()
}

pub fn host_matches(url: &str, hosts: &[&str]) -> bool {
    parse_url(url)
        .and_then(|parsed| parsed.host_str().map(str::to_ascii_lowercase))
        .map(|host| hosts.iter().any(|candidate| host == *candidate))
        .unwrap_or(false)
}

pub fn path_segments(url: &str) -> Vec<String> {
    parse_url(url)
        .and_then(|parsed| {
            parsed.path_segments().map(|values| {
                values
                    .filter(|segment| !segment.is_empty())
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
        })
        .unwrap_or_default()
}

pub fn removed_error(provider: &str) -> anyhow::Error {
    anyhow!("Arquivo não localizado no {provider}")
}

pub fn unsupported_error(provider: &str) -> anyhow::Error {
    anyhow!("Link não suportado pelo fluxo atual do {provider}")
}

pub fn premium_required_error(provider: &str, detail: &str) -> anyhow::Error {
    anyhow!("PREMIUM_REQUIRED:{provider}:{detail}")
}

pub fn rate_limit_error(secs: u64, message: impl AsRef<str>) -> anyhow::Error {
    anyhow!("RATE_LIMIT:{secs}:{}", message.as_ref())
}

pub fn captcha_required_error(kind: &str, sitekey: &str, pageurl: &str) -> anyhow::Error {
    anyhow!("CAPTCHA_REQUIRED:{kind}:{sitekey}:{pageurl}")
}

pub fn all_provider_descriptors() -> Vec<ProviderDescriptor> {
    [
        ("Mega", "#e84d3d"),
        ("MediaFire", "#0062C7"),
        ("Google Drive", "#4285F4"),
        ("PixelDrain", "#ff6600"),
        ("1Fichier", "#e67e22"),
        ("Drime", "#2ec4b6"),
        ("Rapidgator", "#23a2dc"),
        ("BRupload", "#16a34a"),
        ("BRFiles", "#22c55e"),
        ("MoonDL", "#64748b"),
        ("AkiraBox", "#0f172a"),
        ("Katfile", "#2563eb"),
        ("Terabox", "#2a6df5"),
        ("OneDrive", "#0a66d9"),
        ("Direct HTTP", "#0f766e"),
    ]
    .into_iter()
        .map(|(name, color)| ProviderDescriptor {
            id: provider_id_from_name(name),
            name,
            color,
            capabilities: capabilities_for_provider_name(name),
            account_state: None,
        })
        .collect()
}

pub fn provider_descriptor_from_name(name: &str) -> ProviderDescriptor {
    all_provider_descriptors()
        .into_iter()
        .find(|descriptor| descriptor.name == name)
        .unwrap_or(ProviderDescriptor {
            id: "unknown",
            name: "Unknown",
            color: "#64748b",
            capabilities: ProviderCapabilities::default(),
            account_state: None,
        })
}

pub trait ProviderDefaults {
    fn safe_filename(name: &str, fallback: &str) -> String
    where
        Self: Sized,
    {
        sanitize_filename(name, fallback)
    }

    fn http_client_with_proxy(proxy_mode: &str, proxy_host: &str, proxy_port: u16, proxy_username: Option<&str>, proxy_password: Option<&str>) -> Result<reqwest::Client>
    where
        Self: Sized,
    {
        let mut builder = reqwest::Client::builder()
            .user_agent(DEFAULT_USER_AGENT)
            .redirect(reqwest::redirect::Policy::limited(10));

        match proxy_mode {
            "http" | "https" => {
                let proxy_url = if let (Some(username), Some(password)) = (proxy_username, proxy_password) {
                    format!("http://{}:{}@{}:{}", username, password, proxy_host, proxy_port)
                } else {
                    format!("http://{}:{}", proxy_host, proxy_port)
                };
                if let Ok(proxy) = reqwest::Proxy::http(&proxy_url) {
                    builder = builder.proxy(proxy);
                }
            }
            "socks5" => {
                let proxy_url = if let (Some(username), Some(password)) = (proxy_username, proxy_password) {
                    format!("socks5://{}:{}@{}:{}", username, password, proxy_host, proxy_port)
                } else {
                    format!("socks5://{}:{}", proxy_host, proxy_port)
                };
                if let Ok(proxy) = reqwest::Proxy::all(&proxy_url) {
                    builder = builder.proxy(proxy);
                }
            }
            "tor" => {
                // TOR uses SOCKS5 on localhost:9050
                let proxy_url = "socks5://127.0.0.1:9050";
                if let Ok(proxy) = reqwest::Proxy::all(proxy_url) {
                    builder = builder.proxy(proxy);
                }
            }
            _ => {} // none or invalid
        }

        Ok(builder.build()?)
    }

    fn http_client() -> Result<reqwest::Client>
    where
        Self: Sized,
    {
        let lock = GLOBAL_PROXY.get_or_init(|| RwLock::new(ProxyConfig::default()));
        if let Ok(proxy) = lock.read() {
            Self::http_client_with_proxy(&proxy.mode, &proxy.host, proxy.port,
                proxy.username.as_deref(), proxy.password.as_deref())
        } else {
            Self::http_client_with_proxy("none", "", 0, None, None)
        }
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

#[derive(Debug, Clone, Default)]
pub struct DownloadContext {
    pub db_path: Option<String>,
    pub proxy_mode: String,
    pub proxy_host: String,
    pub proxy_port: u16,
    pub proxy_username: Option<String>,
    pub proxy_password: Option<String>,
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

    fn capabilities(&self) -> ProviderCapabilities {
        capabilities_for_provider_name(self.name())
    }

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

    fn get_file_info_with_context<'a>(
        &'a self,
        url: &'a str,
        _context: DownloadContext,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FileInfo>> + Send + 'a>> {
        self.get_file_info(url)
    }

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

    fn download_with_context<'a>(
        &'a self,
        url: &'a str,
        dest_path: &'a str,
        speed_limit_bps: Option<u64>,
        parallel_parts: usize,
        selected_children: Option<Vec<String>>,
        progress_tx: tokio::sync::mpsc::Sender<ProgressUpdate>,
        _context: DownloadContext,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u64>> + Send + 'a>> {
        // For now, just call the regular download. Providers can override if needed.
        self.download(
            url,
            dest_path,
            speed_limit_bps,
            parallel_parts,
            selected_children,
            progress_tx,
        )
    }
}

pub fn capabilities_for_provider_name(name: &str) -> ProviderCapabilities {
    match name {
        "BRFiles" => ProviderCapabilities {
            max_parallel_downloads_free: Some(1),
            free_cooldown_secs: Some(3600),
            ..ProviderCapabilities::default()
        },
        "MoonDL" => ProviderCapabilities {
            supports_auto_captcha: true,
            free_cooldown_secs: Some(3600),
            ..ProviderCapabilities::default()
        },
        "1Fichier" => ProviderCapabilities {
            free_cooldown_secs: Some(300),
            ..ProviderCapabilities::default()
        },
        "BRupload" => ProviderCapabilities {
            requires_browser_helper: true,
            supports_manual_auth: true,
            supports_auto_captcha: true,
            requires_account_for_large_files: true,
            ..ProviderCapabilities::default()
        },
        "AkiraBox" => ProviderCapabilities {
            requires_browser_helper: true,
            supports_auto_captcha: true,
            ..ProviderCapabilities::default()
        },
        "Katfile" => ProviderCapabilities {
            requires_browser_helper: true,
            supports_auto_captcha: true,
            ..ProviderCapabilities::default()
        },
        "Terabox" => ProviderCapabilities {
            requires_browser_helper: true,
            supports_folder: true,
            supports_manual_auth: true,
            max_parallel_downloads_free: Some(1),
            ..ProviderCapabilities::default()
        },
        "MediaFire" | "Mega" => ProviderCapabilities {
            supports_folder: true,
            free_cooldown_secs: if name == "Mega" { Some(30 * 60) } else { None },
            ..ProviderCapabilities::default()
        },
        "Rapidgator" => ProviderCapabilities {
            supports_auto_captcha: true,
            free_cooldown_secs: Some(3600),
            ..ProviderCapabilities::default()
        },
        _ => ProviderCapabilities::default(),
    }
}

pub fn provider_id_from_name(name: &str) -> &'static str {
    match name {
        "Mega" => "mega",
        "MediaFire" => "mediafire",
        "Google Drive" => "gdrive",
        "PixelDrain" => "pixeldrain",
        "1Fichier" => "fichier",
        "Drime" => "drime",
        "Rapidgator" => "rapidgator",
        "BRupload" => "brupload",
        "BRFiles" => "brfiles",
        "MoonDL" => "moondl",
        "AkiraBox" => "akirabox",
        "Katfile" => "katfile",
        "Terabox" => "terabox",
        "OneDrive" => "onedrive",
        "Direct HTTP" => "direct_http",
        _ => "unknown",
    }
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
    if rapidgator::RapidgatorProvider::matches(url) {
        return Some(Box::new(rapidgator::RapidgatorProvider));
    }
    if brupload::BruploadProvider::matches(url) {
        return Some(Box::new(brupload::BruploadProvider));
    }
    if brfiles::BrfilesProvider::matches(url) {
        return Some(Box::new(brfiles::BrfilesProvider));
    }
    if moondl::MoonDLProvider::matches(url) {
        return Some(Box::new(moondl::MoonDLProvider));
    }
    if akirabox::AkiraboxProvider::matches(url) {
        return Some(Box::new(akirabox::AkiraboxProvider));
    }
    if katfile::KatfileProvider::matches(url) {
        return Some(Box::new(katfile::KatfileProvider));
    }
    if direct_http::DirectHttpProvider::matches(url) {
        return Some(Box::new(direct_http::DirectHttpProvider));
    }
    // URL não reconhecida por nenhum provider suportado
    None
}
