use std::collections::HashSet;

use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use serde_json::{json, Value};
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;

use crate::models::{FileChildInfo, FileInfo};

use super::{
    apply_speed_limit, host_matches, parse_human_size, path_segments, ProgressUpdate, Provider,
    ProviderDefaults,
};

const SEND_NOW_HOME: &str = "https://send.now/";
const SEND_NOW_HOSTS: &[&str] = &["send.now", "www.send.now"];

fn electron_proxy_port() -> Option<u16> {
    std::env::var("SENDNOW_PROXY_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .filter(|port| *port > 0)
}

fn helper_proxy_token() -> Option<String> {
    std::env::var("GDOWNLOADER_HELPER_TOKEN")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

async fn browser_action(payload: Value) -> Result<Value> {
    let port =
        electron_proxy_port().ok_or_else(|| anyhow!("Helper local do Send.now não disponível"))?;
    let token = helper_proxy_token()
        .ok_or_else(|| anyhow!("Token do helper local do Send.now não disponível"))?;
    let response = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?
        .post(format!("http://127.0.0.1:{port}/"))
        .header("X-GDownloader-Token", token)
        .json(&payload)
        .send()
        .await?;
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        return Err(anyhow!(
            "Helper local do Send.now respondeu HTTP {status}: {}",
            body.trim()
        ));
    }
    serde_json::from_str(&body).map_err(|error| {
        anyhow!("Resposta inválida do helper local do Send.now: {error}: {body}")
    })
}

#[derive(Debug, Clone)]
enum SendNowTarget {
    Folder { url: String, name: String },
    File { id: String },
}

#[derive(Debug, Clone)]
struct SendNowFile {
    id: String,
    name: String,
    size: u64,
    source_url: String,
}

pub struct SendNowProvider;

impl SendNowProvider {
    pub fn matches(url: &str) -> bool {
        if !host_matches(url, SEND_NOW_HOSTS) {
            return false;
        }
        matches!(
            Self::parse_target(url),
            Some(SendNowTarget::Folder { .. } | SendNowTarget::File { .. })
        )
    }

    fn parse_target(url: &str) -> Option<SendNowTarget> {
        if !host_matches(url, SEND_NOW_HOSTS) {
            return None;
        }
        let segments = path_segments(url);
        match segments.as_slice() {
            [prefix, folder_id, folder_name, ..] if prefix == "s" && !folder_id.is_empty() => {
                Some(SendNowTarget::Folder {
                    url: url.split('#').next().unwrap_or(url).to_string(),
                    name: Self::decode_html(folder_name),
                })
            }
            [prefix, id, ..] if prefix == "d" && !id.is_empty() => {
                Some(SendNowTarget::File { id: id.to_string() })
            }
            [id] if id.len() >= 6 && id.chars().all(|ch| ch.is_ascii_alphanumeric()) => {
                Some(SendNowTarget::File { id: id.to_string() })
            }
            _ => None,
        }
    }

    fn decode_html(value: &str) -> String {
        value
            .replace("&amp;", "&")
            .replace("&quot;", "\"")
            .replace("&#039;", "'")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .trim()
            .to_string()
    }

    fn response_is_cloudflare_challenge(status: reqwest::StatusCode, body: &str) -> bool {
        status == reqwest::StatusCode::FORBIDDEN
            && (body.contains("Just a moment") || body.contains("cf-mitigated"))
    }

    async fn fetch_html(client: &reqwest::Client, url: &str) -> Result<String> {
        let response = client.get(url).send().await?;
        let status = response.status();
        let body = response.text().await?;
        if Self::response_is_cloudflare_challenge(status, &body) {
            return Err(anyhow!(
                "Send.now pediu a verificação do Cloudflare. Abra o link uma vez no navegador e tente novamente; não é um link removido."
            ));
        }
        if !status.is_success() {
            return Err(anyhow!("Send.now respondeu HTTP {status}"));
        }
        Ok(body)
    }

    fn parse_folder_files(html: &str) -> Vec<SendNowFile> {
        let Some(row_re) = regex::Regex::new(
            r#"(?is)<tr\b[^>]*\bclass\s*=\s*[\"'][^\"']*\bselectable\b[^\"']*[\"'][^>]*>.*?<a\b[^>]*\bhref\s*=\s*[\"']https?://(?:www\.)?send\.now/(?:d/)?([A-Za-z0-9]+)[^\"']*[\"'][^>]*>\s*(.*?)\s*</a>.*?<span\b[^>]*>\s*([^<]+?)\s*</span>.*?</tr>"#,
        ).ok() else {
            return Vec::new();
        };

        row_re
            .captures_iter(html)
            .filter_map(|captures| {
                let id = captures[1].trim();
                let name = Self::decode_html(
                    &regex::Regex::new(r#"(?is)<[^>]+>"#)
                        .ok()?
                        .replace_all(&captures[2], " "),
                );
                if id.is_empty() || name.is_empty() {
                    return None;
                }
                Some(SendNowFile {
                    id: id.to_string(),
                    name: <Self as ProviderDefaults>::safe_filename(&name, "arquivo_sendnow"),
                    size: parse_human_size(&Self::decode_html(&captures[3])),
                    source_url: format!("https://send.now/{id}"),
                })
            })
            .collect()
    }

    async fn download_response(
        client: &reqwest::Client,
        id: &str,
        existing_bytes: u64,
    ) -> Result<(reqwest::Response, bool)> {
        // A página e a API do Send.now ficam atrás do Cloudflare. O helper do
        // Electron resolve o redirect com a sessão persistente; daqui em diante
        // o download é feito diretamente pelo Rust, sem cookies do usuário.
        let direct_url = Self::resolve_direct_url(id).await?;
        let mut request = client
            .get(direct_url)
            .header(reqwest::header::REFERER, SEND_NOW_HOME);
        if existing_bytes > 0 {
            request = request.header(reqwest::header::RANGE, format!("bytes={existing_bytes}-"));
        }
        let response = request.send().await?;
        let resumed =
            existing_bytes > 0 && response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
        let response = response.error_for_status()?;
        if Self::is_document_response(&response) {
            return Err(anyhow!(
                "Send.now devolveu uma página de verificação em vez do arquivo. O link temporário será revalidado antes de tentar novamente."
            ));
        }
        Ok((response, resumed))
    }

    /// Um download de arquivo não deve receber HTML/JavaScript sem
    /// Content-Disposition. Esta é a forma usada pelo Cloudflare para entregar
    /// o desafio, que antes era salvo silenciosamente com extensão `.rar`.
    fn is_document_response(response: &reqwest::Response) -> bool {
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let has_attachment_name = response
            .headers()
            .get(reqwest::header::CONTENT_DISPOSITION)
            .is_some();
        !has_attachment_name
            && (content_type.contains("text/html")
                || content_type.contains("application/javascript")
                || content_type.contains("text/javascript"))
    }

    /// O Send.now entrega URLs temporárias por POST. Na prévia de um link
    /// unitário, capturamos só o redirect e fazemos um Range de um byte para
    /// obter nome e tamanho sem iniciar um download completo.
    async fn resolve_direct_url(id: &str) -> Result<String> {
        let source_url = format!("https://send.now/{id}");
        let browser_result = browser_action(json!({
            "action": "sendnow_resolve",
            "url": source_url,
        }))
        .await;
        let browser_error = browser_result.as_ref().err().map(|error| error.to_string());
        // O helper Electron trabalha na sessão persistente do Send.now. Se o
        // host respondeu 403, ele abriu a página para verificação manual; não
        // faça o POST de fallback em Rust, pois isso só repetiria a requisição
        // sem a interação do usuário e esconderia o motivo real da fila.
        if browser_error
            .as_deref()
            .is_some_and(|error| error.contains("SENDNOW_MANUAL_VERIFICATION_REQUIRED"))
        {
            return Err(anyhow!(
                "SENDNOW_MANUAL_VERIFICATION_REQUIRED:{source_url}"
            ));
        }
        if let Ok(result) = &browser_result {
            if let Some(url) = result["url"]
                .as_str()
                .filter(|url| url.starts_with("http://") || url.starts_with("https://"))
            {
                return Ok(url.to_string());
            }
        }

        let resolver = reqwest::Client::builder()
            .user_agent(super::DEFAULT_USER_AGENT)
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        let mut download_id = id.to_string();
        let mut rand = String::new();
        let mut referer = source_url.clone();
        let mut request = resolver
            .post(SEND_NOW_HOME)
            .header(reqwest::header::REFERER, SEND_NOW_HOME);
        if let Ok(result) = &browser_result {
            if let Some(user_agent) = result["userAgent"]
                .as_str()
                .filter(|value| !value.trim().is_empty())
            {
                request = request.header(reqwest::header::USER_AGENT, user_agent);
            }
            if let Some(cookie_header) = result["cookieHeader"]
                .as_str()
                .filter(|value| !value.trim().is_empty())
            {
                request = request.header(reqwest::header::COOKIE, cookie_header);
            }
            if let Some(value) = result["downloadId"]
                .as_str()
                .filter(|value| value.chars().all(|ch| ch.is_ascii_alphanumeric()))
                .filter(|value| !value.is_empty())
            {
                download_id = value.to_string();
            }
            if let Some(value) = result["rand"].as_str() {
                rand = value.to_string();
            }
            if let Some(value) = result["referer"]
                .as_str()
                .filter(|value| value.starts_with("https://send.now/"))
            {
                referer = value.to_string();
            }
        }
        let response = request
            .form(&[
                ("op", "download2"),
                ("id", download_id.as_str()),
                ("rand", rand.as_str()),
                ("referer", referer.as_str()),
                ("method_free", ""),
                ("method_premium", ""),
            ])
            .send()
            .await?;
        response
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string)
            .filter(|url| url.starts_with("http://") || url.starts_with("https://"))
            .ok_or_else(|| {
                let detail = browser_error
                    .as_deref()
                    .map(|error| format!(" ({error})"))
                    .unwrap_or_default();
                anyhow!("Send.now não retornou o link temporário do arquivo{detail}")
            })
    }

    fn filename_from_content_disposition(response: &reqwest::Response) -> Option<String> {
        let value = response
            .headers()
            .get(reqwest::header::CONTENT_DISPOSITION)?
            .to_str()
            .ok()?;
        regex::Regex::new(r#"(?i)filename\*?=(?:UTF-8''|\")?([^;\"]+)"#)
            .ok()?
            .captures(value)
            .map(|captures| {
                urlencoding::decode(&captures[1])
                    .unwrap_or_else(|_| captures[1].into())
                    .trim()
                    .to_string()
            })
            .filter(|name| !name.is_empty())
    }

    async fn probe_single_file(
        client: &reqwest::Client,
        id: &str,
    ) -> Result<(Option<String>, u64)> {
        let direct_url = Self::resolve_direct_url(id).await?;
        let response = client
            .get(direct_url)
            .header(reqwest::header::REFERER, SEND_NOW_HOME)
            .header(reqwest::header::RANGE, "bytes=0-0")
            .send()
            .await?
            .error_for_status()?;
        let name = Self::filename_from_content_disposition(&response);
        let size = <Self as ProviderDefaults>::response_total_bytes(&response, 0);
        Ok((name, size))
    }

    async fn stream_file(
        client: &reqwest::Client,
        file: &SendNowFile,
        path: &str,
        base_downloaded: u64,
        total_size: u64,
        speed_limit_bps: &super::SpeedLimitBps,
        started_at: tokio::time::Instant,
        session_downloaded: &mut u64,
        child: bool,
        progress_tx: &tokio::sync::mpsc::Sender<ProgressUpdate>,
    ) -> Result<u64> {
        let existing = tokio::fs::metadata(path)
            .await
            .ok()
            .filter(|meta| meta.is_file())
            .map(|meta| meta.len())
            .unwrap_or(0);
        if file.size > 0 && existing >= file.size {
            return Ok(file.size);
        }
        let (response, resumed) = Self::download_response(client, &file.id, existing).await?;
        let mut out = if resumed {
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .await?
        } else {
            tokio::fs::File::create(path).await?
        };
        let mut downloaded = if resumed { existing } else { 0 };
        let file_total = if file.size > 0 {
            file.size
        } else {
            response
                .content_length()
                .unwrap_or(0)
                .saturating_add(downloaded)
        };
        let mut stream = response.bytes_stream();
        let child_started = tokio::time::Instant::now();
        let mut child_session = 0u64;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            out.write_all(&chunk).await?;
            let len = chunk.len() as u64;
            downloaded += len;
            *session_downloaded += len;
            child_session += len;
            let elapsed = child_started.elapsed().as_secs_f64();
            let child_speed = if elapsed > 0.0 {
                (child_session as f64 / elapsed) as u64
            } else {
                0
            };
            let child_eta = if child_speed > 0 && file_total > downloaded {
                (file_total - downloaded) / child_speed
            } else {
                0
            };
            let _ = progress_tx
                .send(ProgressUpdate {
                    bytes_downloaded: base_downloaded + downloaded,
                    total_bytes: total_size.max(file_total),
                    child_path: None,
                    child_filename: child.then(|| file.name.clone()),
                    child_bytes_downloaded: child.then_some(downloaded),
                    child_total_bytes: child.then_some(file_total),
                    child_speed_bps: child.then_some(child_speed),
                    child_eta_secs: child.then_some(child_eta),
                })
                .await;
            apply_speed_limit(started_at, *session_downloaded, speed_limit_bps).await;
        }
        out.flush().await?;
        Ok(downloaded)
    }
}

impl ProviderDefaults for SendNowProvider {}

impl Provider for SendNowProvider {
    fn name(&self) -> &str {
        "Send.now"
    }

    fn get_file_info<'a>(
        &'a self,
        url: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FileInfo>> + Send + 'a>> {
        Box::pin(async move {
            let target = Self::parse_target(url)
                .ok_or_else(|| anyhow!("URL do Send.now inválida: {url}"))?;
            let client = <Self as ProviderDefaults>::http_client()?;
            match target {
                SendNowTarget::Folder { url, name } => {
                    let files = Self::parse_folder_files(&Self::fetch_html(&client, &url).await?);
                    if files.is_empty() {
                        return Err(anyhow!("Send.now: nenhum arquivo encontrado nesta pasta"));
                    }
                    let children = files
                        .iter()
                        .map(|file| FileChildInfo {
                            filename: file.name.clone(),
                            size: file.size,
                            mime_type: None,
                            is_folder: false,
                            path: None,
                            source_url: Some(file.source_url.clone()),
                            bytes_downloaded: None,
                            speed_bps: None,
                            eta_secs: None,
                            status: None,
                        })
                        .collect::<Vec<_>>();
                    Ok(FileInfo {
                        filename: <Self as ProviderDefaults>::safe_filename(&name, "pasta_sendnow"),
                        size: children.iter().map(|file| file.size).sum(),
                        mime_type: None,
                        is_folder: true,
                        children: Some(children),
                        ..Default::default()
                    })
                }
                SendNowTarget::File { id } => {
                    let page_url = format!("https://send.now/{id}");
                    // Prioriza o redirect temporário: além de ser a única via
                    // estável quando há Cloudflare, fornece nome e tamanho reais.
                    let probe = Self::probe_single_file(&client, &id).await;
                    let (header_name, size) =
                        probe.as_ref().map_or((None, 0), |value| value.clone());
                    let page_name =
                        Self::fetch_html(&client, &page_url)
                            .await
                            .ok()
                            .and_then(|html| {
                                regex::Regex::new(
                                    r#"(?is)<title>\s*(.*?)\s*(?:-|–)\s*Send(?:\.now|\.cm)"#,
                                )
                                .ok()
                                .and_then(|re| re.captures(&html))
                                .map(|captures| Self::decode_html(&captures[1]))
                                .filter(|name| {
                                    !name.is_empty() && !name.eq_ignore_ascii_case("file")
                                })
                            });
                    if probe.is_err() && page_name.is_none() {
                        return Err(probe.err().expect("probe error must exist"));
                    }
                    let name = header_name
                        .or(page_name)
                        .unwrap_or_else(|| format!("sendnow_{id}"));
                    Ok(FileInfo {
                        filename: <Self as ProviderDefaults>::safe_filename(
                            &name,
                            &format!("sendnow_{id}"),
                        ),
                        size,
                        mime_type: None,
                        is_folder: false,
                        children: None,
                        ..Default::default()
                    })
                }
            }
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
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u64>> + Send + 'a>> {
        Box::pin(async move {
            let target = Self::parse_target(url)
                .ok_or_else(|| anyhow!("URL do Send.now inválida: {url}"))?;
            let client = <Self as ProviderDefaults>::http_client()?;
            let started_at = tokio::time::Instant::now();
            let mut session_downloaded = 0u64;
            match target {
                SendNowTarget::File { id } => {
                    let fallback = format!("sendnow_{id}");
                    let file = SendNowFile {
                        id,
                        name: fallback.clone(),
                        size: 0,
                        source_url: url.to_string(),
                    };
                    Self::stream_file(
                        &client,
                        &file,
                        dest_path,
                        0,
                        0,
                        &speed_limit_bps,
                        started_at,
                        &mut session_downloaded,
                        false,
                        &progress_tx,
                    )
                    .await
                }
                SendNowTarget::Folder { url, .. } => {
                    let mut files =
                        Self::parse_folder_files(&Self::fetch_html(&client, &url).await?);
                    if let Some(selected) = selected_children {
                        let selected = selected.into_iter().collect::<HashSet<_>>();
                        files.retain(|file| selected.contains(&file.source_url));
                    }
                    if files.is_empty() {
                        return Err(anyhow!("Send.now: nenhum arquivo selecionado disponível"));
                    }
                    let total = files.iter().map(|file| file.size).sum();
                    tokio::fs::create_dir_all(dest_path).await?;
                    let mut completed = 0u64;
                    for file in &files {
                        let path = format!("{}/{}", dest_path.trim_end_matches('/'), file.name);
                        let downloaded = Self::stream_file(
                            &client,
                            file,
                            &path,
                            completed,
                            total,
                            &speed_limit_bps,
                            started_at,
                            &mut session_downloaded,
                            true,
                            &progress_tx,
                        )
                        .await?;
                        completed += downloaded;
                    }
                    Ok(completed)
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_single_and_folder_links() {
        assert!(matches!(
            SendNowProvider::parse_target("https://send.now/abcdef123456"),
            Some(SendNowTarget::File { .. })
        ));
        assert!(matches!(
            SendNowProvider::parse_target("https://send.now/s/9XXV/Minha_Pasta"),
            Some(SendNowTarget::Folder { .. })
        ));
    }

    #[test]
    fn extracts_folder_children() {
        let html = r#"<tr class="selectable"><td><a href="https://send.now/abc123def"> Filme &amp; Parte.rar </a></td><td><span>12.9 GB</span></td></tr>"#;
        let files = SendNowProvider::parse_folder_files(html);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].name, "Filme & Parte.rar");
        assert_eq!(files[0].size, 13_851_269_530);
    }
}
