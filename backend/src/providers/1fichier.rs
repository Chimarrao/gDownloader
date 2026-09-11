use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio::time::Duration;
use tracing::{debug, info, warn};

use crate::models::{FileChildInfo, FileInfo};
use super::{
    apply_speed_limit, capabilities_for_provider_name, extract_wait_seconds_from_text,
    rate_limit_error, ProgressUpdate, Provider, ProviderDefaults,
};

pub struct FichierProvider;

/// Janela máxima local para um URL final. O servidor continua sendo a fonte de
/// verdade: antes de escrever qualquer byte, o link salvo precisa responder como
/// arquivo binário; caso contrário é descartado e o fluxo normal é retomado.
const RESOLVED_LINK_CACHE_TTL_SECS: u64 = 24 * 60 * 60;

impl FichierProvider {
    pub fn matches(url: &str) -> bool {
        super::parse_url(url)
            .and_then(|parsed| parsed.host_str().map(str::to_ascii_lowercase))
            .map(|host| host == "1fichier.com" || host.ends_with(".1fichier.com"))
            .unwrap_or(false)
    }

    fn extract_download_page(url: &str) -> Option<String> {
        if !Self::matches(url) {
            return None;
        }

        let normalized = url.trim();
        if normalized.is_empty() {
            return None;
        }

        Some(normalized.to_string())
    }

    fn extract_between(haystack: &str, start: &str, end: &str) -> Option<String> {
        let start_idx = haystack.find(start)? + start.len();
        let rest = &haystack[start_idx..];
        let end_idx = rest.find(end)?;
        Some(rest[..end_idx].trim().to_string())
    }

    fn decode_basic_html_entities(value: &str) -> String {
        value
            .replace("&amp;", "&")
            .replace("&quot;", "\"")
            .replace("&#039;", "'")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&nbsp;", " ")
    }

    /// Converte um pequeno fragmento HTML do 1fichier em texto seguro para
    /// nomes de pasta/arquivo. A página de diretório passou a envolver o nome
    /// da pasta em um `span`; deixar essa marcação chegar ao `safe_filename`
    /// transforma `<span style=...>` em texto visível.
    fn html_to_text(value: &str) -> String {
        let without_tags = regex::Regex::new(r"(?is)<[^>]*>")
            .map(|re| re.replace_all(value, "").into_owned())
            .unwrap_or_else(|_| value.to_string());
        Self::decode_basic_html_entities(&without_tags)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn extract_folder_name(html: &str) -> Option<String> {
        // Exemplo atual:
        // <div class="bh3 alc">Shared folder <span ...>soldado</span></div>
        // O conteúdo do span é o nome dado pelo autor da pasta; o prefixo
        // "Shared folder" é apenas texto de interface do 1fichier.
        let span_name = regex::Regex::new(
            r#"(?is)<div\b[^>]*\bclass\s*=\s*["'][^"']*\bbh3\b[^"']*["'][^>]*>.*?<span\b[^>]*>(.*?)</span>"#,
        )
        .ok()
        .and_then(|re| re.captures(html))
        .and_then(|captures| captures.get(1))
        .map(|capture| Self::html_to_text(capture.as_str()))
        .filter(|name| !name.is_empty());

        span_name.or_else(|| {
            Self::extract_between(html, "<div class=\"bh3 alc\">", "</div>")
                .map(|value| Self::html_to_text(&value))
                .filter(|name| !name.is_empty())
        })
    }

    fn extract_filename_and_size(html: &str, fallback_name: &str) -> (String, u64) {
        let filename = Self::extract_between(
            html,
            "<span style=\"font-weight:bold\">",
            "</span>",
        )
        .map(|s| Self::decode_basic_html_entities(&s))
        .filter(|s| !s.is_empty())
        .or_else(|| {
            Self::extract_between(html, "<title>", "</title>")
                .map(|s| s.replace(" - 1fichier.com", "").trim().to_string())
        })
        .unwrap_or_else(|| fallback_name.to_string());

        let human_size = Self::extract_between(
            html,
            "<span style=\"font-size:0.9em;font-style:italic\">",
            "</span>",
        )
        .unwrap_or_default();

        (filename, Self::parse_human_size(&human_size))
    }

    fn parse_human_size(value: &str) -> u64 {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return 0;
        }

        let mut parts = trimmed.split_whitespace();
        let number = parts
            .next()
            .map(|raw| raw.replace(',', "."))
            .and_then(|raw| raw.parse::<f64>().ok())
            .unwrap_or(0.0);
        let unit = parts.next().unwrap_or("").to_ascii_uppercase();

        let multiplier = match unit.as_str() {
            "B" => 1f64,
            "KB" => 1024f64,
            "MB" => 1024f64.powi(2),
            "GB" => 1024f64.powi(3),
            "TB" => 1024f64.powi(4),
            _ => 1f64,
        };

        (number * multiplier).round() as u64
    }

    fn extract_wait_seconds(html: &str) -> Option<u64> {
        // JS countdown: var ct = 60;
        let marker = "var ct = ";
        if let Some(start) = html.find(marker) {
            let rest = &html[start + marker.len()..];
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(s) = digits.parse::<u64>() {
                return Some(s);
            }
        }
        // English text: "wait X minutes/hours"
        let lower = html.to_lowercase();
        if let Some(pos) = lower.find("wait ") {
            let rest = &lower[pos + 5..];
            let n: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(n) = n.parse::<u64>() {
                let after = rest[n.to_string().len()..].trim_start();
                if after.starts_with("hour") { return Some(n * 3600); }
                if after.starts_with("minute") { return Some(n * 60); }
                if after.starts_with("second") { return Some(n); }
            }
        }
        extract_wait_seconds_from_text(html)
    }

    fn has_free_slot_error(html: &str) -> bool {
        html.contains("All free guest slots are currently in use")
            || html.contains("Sign in instantly to continue your download")
    }

    // Cobre PT/FR/EN conhecidos; ampliar quando houver amostras reais de outras línguas.
    /// 1fichier bloqueia IPs de servidor/VPN/proxy/Tor para download gratuito.
    /// A página de bloqueio não tem o formulário de download, então sem isso o
    /// fluxo cairia em um erro genérico de "link não encontrado".
    fn has_restricted_access_error(html: &str) -> bool {
        html.contains("professional infrastructure detected")
            || html.contains("Accès restreint")
            || html.contains("professional network infrastructures")
    }

    fn restricted_access_error() -> anyhow::Error {
        anyhow!(
            "1Fichier bloqueou este IP para download gratuito (VPN, proxy, Tor ou \
             infraestrutura de servidor detectada). Desative o proxy/Tor e use uma \
             conexão residencial, ou utilize uma conta premium."
        )
    }

    fn is_folder_page(url: &str, html: &str) -> bool {
        url.contains("/dir/") || html.contains("liste des fichiers") || html.contains("file list")
    }

    /// Extrai links de arquivos de uma página de pasta do 1fichier.
    fn extract_folder_children(html: &str) -> Vec<FileChildInfo> {
        // O 1fichier deixou de usar as antigas classes `normal alg` e agora
        // adiciona atributos ao link. O parser precisa reconhecer a semântica
        // `file-obj`, não uma ordem exata de atributos/classes.
        let Some(re) = regex::Regex::new(
            r#"(?is)<td\b[^>]*\bclass\s*=\s*["'][^"']*\bfile-obj\b[^"']*["'][^>]*>\s*<a\b[^>]*\bhref\s*=\s*["'](https://1fichier\.com/\?[^"']+)["'][^>]*>(.*?)</a>\s*</td>\s*<td\b[^>]*>\s*([^<]+)"#,
        )
        .ok() else {
            return Vec::new();
        };

        re.captures_iter(html)
            .filter_map(|captures| {
                let url = captures[1].trim().to_string();
                let filename = <Self as ProviderDefaults>::safe_filename(
                    &Self::html_to_text(captures[2].trim()),
                    "arquivo_1fichier",
                );
                let size = Self::parse_human_size(&Self::decode_basic_html_entities(captures[3].trim()));

                if filename.is_empty() || url.is_empty() {
                    return None;
                }

                Some(FileChildInfo {
                    filename: filename.clone(),
                    size,
                    mime_type: None,
                    is_folder: false,
                    path: Some(filename),
                    source_url: Some(url),
                    bytes_downloaded: None,
                    speed_bps: None,
                    eta_secs: None,
                    status: None,
                })
            })
            .collect()
    }

    fn load_cached_direct_link(db_path: Option<&str>, page_url: &str) -> Option<String> {
        let db_path = db_path?;
        let conn = rusqlite::Connection::open(db_path).ok()?;
        let _ = conn.busy_timeout(std::time::Duration::from_secs(2));
        crate::db::load_resolved_download_link(&conn, "fichier", page_url)
            .ok()
            .flatten()
    }

    fn save_cached_direct_link(db_path: Option<&str>, page_url: &str, direct_url: &str) {
        let Some(db_path) = db_path else { return; };
        let Ok(conn) = rusqlite::Connection::open(db_path) else { return; };
        let _ = conn.busy_timeout(std::time::Duration::from_secs(2));
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let _ = crate::db::save_resolved_download_link(
            &conn,
            "fichier",
            page_url,
            direct_url,
            Some(page_url),
            now.saturating_add(RESOLVED_LINK_CACHE_TTL_SECS),
        );
    }

    fn touch_cached_direct_link(db_path: Option<&str>, page_url: &str) {
        let Some(db_path) = db_path else { return; };
        let Ok(conn) = rusqlite::Connection::open(db_path) else { return; };
        let _ = conn.busy_timeout(std::time::Duration::from_secs(2));
        let _ = crate::db::touch_resolved_download_link(&conn, "fichier", page_url);
    }

    fn remove_cached_direct_link(db_path: Option<&str>, page_url: &str) {
        let Some(db_path) = db_path else { return; };
        let Ok(conn) = rusqlite::Connection::open(db_path) else { return; };
        let _ = conn.busy_timeout(std::time::Duration::from_secs(2));
        let _ = crate::db::delete_resolved_download_link(&conn, "fichier", page_url);
    }

    fn cached_link_is_invalid(error: &anyhow::Error) -> bool {
        let message = error.to_string().to_ascii_lowercase();
        message.contains("link temporário não retornou arquivo binário")
            || message.contains("401 unauthorized")
            || message.contains("403 forbidden")
            || message.contains("404 not found")
            || message.contains("410 gone")
    }

    async fn stream_direct_link(
        client: &reqwest::Client,
        direct_url: &str,
        page_url: &str,
        dest_path: &str,
        speed_limit_bps: super::SpeedLimitBps,
        progress_tx: tokio::sync::mpsc::Sender<ProgressUpdate>,
    ) -> Result<u64> {
        // Resume: se já existe um arquivo parcial no disco (retomada após pausa ou
        // queda de conexão), pede só o que falta via Range e continua de onde parou.
        let existing = tokio::fs::metadata(dest_path)
            .await
            .ok()
            .filter(|meta| meta.is_file())
            .map(|meta| meta.len())
            .unwrap_or(0);

        let mut request = client.get(direct_url).header("Referer", page_url);
        if existing > 0 {
            request = request.header(reqwest::header::RANGE, format!("bytes={existing}-"));
        }
        let resp = request.send().await?;
        // Arquivo já estava completo (o servidor rejeita o Range além do fim): pronto.
        if existing > 0 && resp.status() == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
            return Ok(existing);
        }
        let resp = resp.error_for_status()?;
        // Um token vencido pode responder uma página HTML com HTTP 200. Não a
        // gravamos como arquivo: o chamador descarta esse token e gera outro.
        if !Self::is_binary_response(&resp) {
            return Err(anyhow!("Link temporário não retornou arquivo binário"));
        }
        // Só retoma se o servidor confirmou o Range (206). Senão, começa do zero.
        let resume_from = if existing > 0 && resp.status() == reqwest::StatusCode::PARTIAL_CONTENT {
            existing
        } else {
            0
        };
        Self::stream_response_to_file(resp, dest_path, resume_from, speed_limit_bps, progress_tx).await
    }

    async fn try_cached_direct_link(
        client: &reqwest::Client,
        page_url: &str,
        dest_path: &str,
        speed_limit_bps: super::SpeedLimitBps,
        progress_tx: tokio::sync::mpsc::Sender<ProgressUpdate>,
        db_path: Option<&str>,
    ) -> Result<Option<u64>> {
        let Some(direct_url) = Self::load_cached_direct_link(db_path, page_url) else {
            return Ok(None);
        };

        info!(target: "gdownloader_backend::providers::1fichier", "1Fichier reutilizando link temporário salvo: {}", page_url);
        match Self::stream_direct_link(
            client,
            &direct_url,
            page_url,
            dest_path,
            speed_limit_bps,
            progress_tx,
        )
        .await
        {
            Ok(bytes) => {
                Self::touch_cached_direct_link(db_path, page_url);
                Ok(Some(bytes))
            }
            Err(error) if Self::cached_link_is_invalid(&error) => {
                warn!(target: "gdownloader_backend::providers::1fichier", "1Fichier link temporário expirado/inválido; voltando ao fluxo do host: {}", page_url);
                Self::remove_cached_direct_link(db_path, page_url);
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }

    async fn download_single_file(
        client: &reqwest::Client,
        page_url: &str,
        dest_path: &str,
        speed_limit_bps: super::SpeedLimitBps,
        progress_tx: tokio::sync::mpsc::Sender<ProgressUpdate>,
        db_path: Option<&str>,
    ) -> Result<u64> {
        if let Some(bytes) = Self::try_cached_direct_link(
            client,
            page_url,
            dest_path,
            speed_limit_bps.clone(),
            progress_tx.clone(),
            db_path,
        )
        .await?
        {
            return Ok(bytes);
        }

        info!(target: "gdownloader_backend::providers::1fichier", "1Fichier abrindo landing page: {}", page_url);
        let landing = client.get(page_url).send().await?.error_for_status()?.text().await?;

        if Self::has_restricted_access_error(&landing) {
            warn!(target: "gdownloader_backend::providers::1fichier", "1Fichier bloqueou IP (VPN/proxy/Tor) para {}", page_url);
            return Err(Self::restricted_access_error());
        }

        if Self::has_free_slot_error(&landing) {
            warn!(target: "gdownloader_backend::providers::1fichier", "1Fichier sem slot gratuito para {}", page_url);
            return Err(Self::free_slot_error());
        }

        let wait_seconds = Self::extract_wait_seconds(&landing).unwrap_or(0);
        debug!(target: "gdownloader_backend::providers::1fichier", "1Fichier wait_seconds={} url={}", wait_seconds, page_url);
        if wait_seconds > 90 {
            return Err(rate_limit_error(wait_seconds, format!("1Fichier: aguarde {} minutos", wait_seconds / 60)));
        } else if wait_seconds > 0 {
            tokio::time::sleep(Duration::from_secs(wait_seconds)).await;
        }

        let response = client
            .post(page_url)
            .header("Referer", page_url)
            .form(&[("dl_no_ssl", "on")])
            .send()
            .await?
            .error_for_status()?;

        if Self::is_binary_response(&response) {
            info!(target: "gdownloader_backend::providers::1fichier", "1Fichier respondeu binário direto: {}", page_url);
            // Caminho binário direto: a resposta do POST já veio sem Range, então
            // não há como retomar aqui (recomeça do zero).
            return Self::stream_response_to_file(response, dest_path, 0, speed_limit_bps, progress_tx).await;
        }

        let html = response.text().await?;
        if Self::has_restricted_access_error(&html) {
            warn!(target: "gdownloader_backend::providers::1fichier", "1Fichier bloqueou IP (VPN/proxy/Tor) após POST: {}", page_url);
            return Err(Self::restricted_access_error());
        }
        if Self::has_free_slot_error(&html) {
            warn!(target: "gdownloader_backend::providers::1fichier", "1Fichier continuou sem slot gratuito após POST: {}", page_url);
            return Err(Self::free_slot_error());
        }

        if let Some(secs) = Self::extract_wait_seconds(&html).filter(|secs| *secs > 0) {
            return Err(rate_limit_error(
                secs.max(60),
                format!("1Fichier: o host pediu aguardar {} segundos antes de liberar o link", secs),
            ));
        }

        let direct_url = Self::extract_direct_link(&html).ok_or_else(|| {
            // O 1Fichier pode devolver uma página HTTP 200 sem botão quando o
            // slot recém-criado ainda está em cooldown. Isso não é um link
            // incompatível: preservar o parcial e tentar depois é mais seguro
            // do que encerrar o download e forçar a geração de outro token.
            let cooldown = capabilities_for_provider_name("1Fichier")
                .free_cooldown_secs
                .unwrap_or(300);
            rate_limit_error(
                cooldown,
                "1Fichier ainda não disponibilizou o link temporário. O parcial foi preservado e a fila vai tentar novamente após o cooldown.",
            )
        })?;
        info!(target: "gdownloader_backend::providers::1fichier", "1Fichier link final extraído para {}", page_url);
        Self::save_cached_direct_link(db_path, page_url, &direct_url);
        Self::stream_direct_link(client, &direct_url, page_url, dest_path, speed_limit_bps, progress_tx).await
    }

    async fn stream_response_to_file(
        response: reqwest::Response,
        dest_path: &str,
        resume_from: u64,
        speed_limit_bps: super::SpeedLimitBps,
        progress_tx: tokio::sync::mpsc::Sender<ProgressUpdate>,
    ) -> Result<u64> {
        let content_len = response.content_length().unwrap_or(0);
        // Quando retomamos, content_len é só o que falta; o total é parcial + restante.
        let total = if resume_from > 0 {
            resume_from.saturating_add(content_len)
        } else {
            content_len
        };
        let mut file = if resume_from > 0 {
            OpenOptions::new().create(true).append(true).open(dest_path).await?
        } else {
            tokio::fs::File::create(dest_path).await?
        };
        let mut stream = response.bytes_stream();
        let mut downloaded = resume_from;
        let mut session_downloaded = 0u64;
        let started_at = tokio::time::Instant::now();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            file.write_all(&chunk).await?;
            let chunk_len = chunk.len() as u64;
            downloaded += chunk_len;
            session_downloaded += chunk_len;

            let _ = progress_tx.send(ProgressUpdate {
                bytes_downloaded: downloaded,
                total_bytes: total,
                child_path: None,
                child_filename: None,
                child_bytes_downloaded: None,
                child_total_bytes: None,
                child_speed_bps: None,
                child_eta_secs: None,
            }).await;
            apply_speed_limit(started_at, session_downloaded, &speed_limit_bps).await;
        }

        file.flush().await?;
        Ok(downloaded)
    }

    fn extract_direct_link(html: &str) -> Option<String> {
        // 1) Caminho feliz: o botão verde "Click here to download" do 1fichier
        //    (`class="...btn-orange..."`). O href pode vir antes ou depois da classe.
        let button_patterns = [
            r#"(?is)<a\b[^>]*class="[^"]*btn-orange[^"]*"[^>]*href="(https?://[^"]+)""#,
            r#"(?is)<a\b[^>]*href="(https?://[^"]+)"[^>]*class="[^"]*btn-orange[^"]*""#,
        ];
        for pattern in button_patterns {
            if let Ok(re) = regex::Regex::new(pattern) {
                if let Some(caps) = re.captures(html) {
                    let href = caps[1].to_string();
                    if Self::is_plausible_direct_link(&href) {
                        return Some(href);
                    }
                }
            }
        }

        // 2) Fallback: primeiro href que pareça realmente o arquivo final.
        //    O filtro descarta favicon/CSS/JS, páginas institucionais e redes
        //    sociais — sem ele, o primeiro href da página é o favicon (~1 KB),
        //    que era exatamente o "download de 1 KB" relatado.
        let mut cursor = html;
        while let Some(pos) = cursor.find("href=\"") {
            let rest = &cursor[pos + 6..];
            let end = rest.find('"')?;
            let href = &rest[..end];
            if href.starts_with("http") && Self::is_plausible_direct_link(href) {
                return Some(href.to_string());
            }
            cursor = &rest[end + 1..];
        }

        None
    }

    /// Decide se um href tem cara de arquivo final do 1fichier (e não de
    /// favicon, folha de estilo, página institucional ou link social).
    fn is_plausible_direct_link(href: &str) -> bool {
        let lower = href.to_ascii_lowercase();

        // Hosts que nunca servem o arquivo final.
        const BAD_HOST_FRAGMENTS: [&str; 4] =
            ["img.1fichier.com", "twitter.com", "facebook.com", "dstorage.fr"];
        if BAD_HOST_FRAGMENTS.iter().any(|frag| lower.contains(frag)) {
            return false;
        }

        // Páginas institucionais / autenticação.
        const BAD_PATHS: [&str; 11] = [
            "/login", "/register", "/hlp", "/tarifs", "/cgu", "/abus", "/network",
            "/contact", "/revendeurs", "/api", "/console",
        ];
        if BAD_PATHS.iter().any(|frag| lower.contains(frag)) {
            return false;
        }

        // Recursos estáticos (favicon, css, js, imagens, html).
        const BAD_EXT: [&str; 9] =
            [".ico", ".png", ".jpg", ".jpeg", ".gif", ".svg", ".css", ".js", ".html"];
        let path_only = lower.split(['?', '#']).next().unwrap_or(&lower);
        if BAD_EXT.iter().any(|ext| path_only.ends_with(ext)) {
            return false;
        }

        // Precisa ter um caminho ou querystring reais (não apenas o domínio raiz).
        let rest = lower.splitn(2, "://").nth(1).unwrap_or("");
        let has_path = rest
            .split_once('/')
            .map(|(_, path)| !path.trim_matches('/').is_empty())
            .unwrap_or(false);
        has_path || rest.contains('?')
    }

    fn is_binary_response(resp: &reqwest::Response) -> bool {
        resp.headers()
            .get("content-disposition")
            .is_some()
            || resp
                .headers()
                .get("content-type")
                .and_then(|value| value.to_str().ok())
                .map(|value| !value.to_ascii_lowercase().contains("text/html"))
                .unwrap_or(false)
    }

    fn free_slot_error() -> anyhow::Error {
        let cooldown = capabilities_for_provider_name("1Fichier")
            .free_cooldown_secs
            .unwrap_or(300);
        rate_limit_error(
            cooldown,
            "1Fichier está sem slot gratuito disponível no momento. O host exige aguardar ou entrar com conta.",
        )
    }
}

impl ProviderDefaults for FichierProvider {}

impl Provider for FichierProvider {
    fn name(&self) -> &str { "1Fichier" }

    fn get_file_info<'a>(&'a self, url: &'a str)
        -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FileInfo>> + Send + 'a>>
    {
        Box::pin(async move {
            let page_url = Self::extract_download_page(url)
                .ok_or_else(|| anyhow!("URL do 1fichier inválida: {url}"))?;
            let client = <Self as ProviderDefaults>::http_client()?;
            info!(target: "gdownloader_backend::providers::1fichier", "1Fichier coletando metadata: {}", page_url);
            let html = client.get(&page_url).send().await?.error_for_status()?.text().await?;

            if Self::has_restricted_access_error(&html) {
                return Err(Self::restricted_access_error());
            }

            // Rate limit
            if let Some(secs) = Self::extract_wait_seconds(&html) {
                if secs > 90 {
                    return Err(rate_limit_error(secs, format!("1Fichier: aguarde {} minutos", secs / 60)));
                }
            }

            // Folder detection
            if Self::is_folder_page(&page_url, &html) {
                let children = Self::extract_folder_children(&html);
                info!(
                    target: "gdownloader_backend::providers::1fichier",
                    "1Fichier pasta detectada: {} child_count={}",
                    page_url,
                    children.len()
                );
                let folder_name = Self::extract_folder_name(&html)
                    .or_else(|| Self::extract_between(&html, "<title>", "</title>"))
                    .unwrap_or_else(|| "pasta_1fichier".to_string());
                let total_size = children.iter().map(|child| child.size).sum();
                return Ok(FileInfo {
                    filename: <Self as ProviderDefaults>::safe_filename(&folder_name, "pasta_1fichier"),
                    size: total_size,
                    mime_type: None,
                    is_folder: true,
                    children: if children.is_empty() { None } else { Some(children) },
                    ..Default::default()
                });
            }

            let fallback = "arquivo_1fichier";
            let (filename, size) = Self::extract_filename_and_size(&html, fallback);

            Ok(FileInfo {
                filename: <Self as ProviderDefaults>::safe_filename(&filename, fallback),
                size,
                mime_type: None,
                is_folder: false,
                children: None,
                ..Default::default()
            })
        })
    }

    fn download<'a>(
        &'a self,
        url: &'a str,
        dest_path: &'a str,
        speed_limit_bps: super::SpeedLimitBps,
        _parallel_parts: usize,
        selected_children: Option<Vec<String>>,
        progress_tx: tokio::sync::mpsc::Sender<ProgressUpdate>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u64>> + Send + 'a>>
    {
        Box::pin(async move {
            let page_url = Self::extract_download_page(url)
                .ok_or_else(|| anyhow!("URL do 1fichier inválida: {url}"))?;
            let client = <Self as ProviderDefaults>::http_client()?;
            let cache_db_path = super::TASK_DB_PATH
                .try_with(Clone::clone)
                .ok()
                .flatten();
            info!(target: "gdownloader_backend::providers::1fichier", "1Fichier iniciando download: {}", page_url);
            let landing = client.get(&page_url).send().await?.error_for_status()?.text().await?;

            if Self::is_folder_page(&page_url, &landing) {
                let mut children = Self::extract_folder_children(&landing);
                info!(
                    target: "gdownloader_backend::providers::1fichier",
                    "1Fichier download de pasta: {} child_count={}",
                    page_url,
                    children.len()
                );
                if let Some(selected) = selected_children {
                    let selected_set = selected.into_iter().collect::<std::collections::HashSet<_>>();
                    children.retain(|child| child.source_url.as_ref().map(|url| selected_set.contains(url)).unwrap_or(false));
                }

                if children.is_empty() {
                    return Err(anyhow!("Pasta do 1Fichier vazia ou sem arquivos selecionados"));
                }

                tokio::fs::create_dir_all(dest_path).await?;

                let total_size: u64 = children.iter().map(|child| child.size).sum();
                let mut downloaded_total = 0u64;

                for (file_index, child) in children.iter().enumerate() {
                    let child_url = child.source_url.clone().ok_or_else(|| anyhow!("Item da pasta do 1Fichier sem URL"))?;
                    let child_path = child.path.clone().unwrap_or_else(|| child.filename.clone());
                    info!(
                        target: "gdownloader_backend::providers::1fichier",
                        "1Fichier baixando item da pasta: {} -> {}",
                        child_url,
                        child_path
                    );
                    let output_path = format!("{}/{}", dest_path.trim_end_matches('/'), child_path);

                    if let Some(parent_dir) = std::path::Path::new(&output_path).parent() {
                        tokio::fs::create_dir_all(parent_dir).await?;
                    }

                    if let Ok(metadata) = tokio::fs::metadata(&output_path).await {
                        let existing_len = metadata.len();
                        if child.size > 0 && existing_len >= child.size {
                            downloaded_total = downloaded_total.saturating_add(child.size);
                            let _ = progress_tx.send(ProgressUpdate {
                                bytes_downloaded: downloaded_total,
                                total_bytes: total_size,
                                child_path: Some(child_path.clone()),
                                child_filename: Some(child.filename.clone()),
                                child_bytes_downloaded: Some(child.size),
                                child_total_bytes: Some(child.size),
                                child_speed_bps: Some(0),
                                child_eta_secs: Some(0),
                            }).await;
                            continue;
                        }
                    }

                    let base_downloaded = downloaded_total;
                    let child_filename = child.filename.clone();
                    let child_total = child.size;
                    let child_started_at = tokio::time::Instant::now();
                    let (child_tx, mut child_rx) = tokio::sync::mpsc::channel::<ProgressUpdate>(64);
                    let child_client = <Self as ProviderDefaults>::http_client_for_file_index(file_index)
                        .unwrap_or_else(|_| client.clone());
                    let child_speed_limit = speed_limit_bps.clone();
                    let child_dest = output_path.clone();
                    let child_cache_db_path = cache_db_path.clone();

                    let child_task = tokio::spawn(async move {
                        Self::download_single_file(
                            &child_client,
                            &child_url,
                            &child_dest,
                            child_speed_limit,
                            child_tx,
                            child_cache_db_path.as_deref(),
                        )
                        .await
                    });

                    while let Some(update) = child_rx.recv().await {
                        let child_downloaded = update.bytes_downloaded;
                        let total_downloaded = base_downloaded + child_downloaded;
                        let child_elapsed = child_started_at.elapsed().as_secs_f64();
                        let child_speed = if child_elapsed > 0.0 {
                            (child_downloaded as f64 / child_elapsed) as u64
                        } else {
                            0
                        };
                        let child_eta = if child_speed > 0 && child_total > child_downloaded {
                            (child_total - child_downloaded) / child_speed
                        } else {
                            0
                        };

                        let _ = progress_tx.send(ProgressUpdate {
                            bytes_downloaded: total_downloaded,
                            total_bytes: total_size,
                            child_path: Some(child_path.clone()),
                            child_filename: Some(child_filename.clone()),
                            child_bytes_downloaded: Some(child_downloaded),
                            child_total_bytes: Some(child_total),
                            child_speed_bps: Some(child_speed),
                            child_eta_secs: Some(child_eta),
                        }).await;
                    }

                    let child_bytes = child_task
                        .await
                        .map_err(|error| anyhow!("Falha interna ao baixar item do 1Fichier: {error}"))??;
                    downloaded_total = base_downloaded + child_bytes;
                }

                return Ok(downloaded_total);
            }

            Self::download_single_file(
                &client,
                &page_url,
                dest_path,
                speed_limit_bps,
                progress_tx,
                cache_db_path.as_deref(),
            )
            .await
        })
    }
}

#[cfg(test)]
#[path = "tests/1fichier.rs"]
mod tests;
