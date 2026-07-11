use std::collections::{HashMap, HashSet};

use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use serde_json::Value;
use tokio::fs::OpenOptions;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::models::{FileChildInfo, FileInfo};
use super::{apply_speed_limit, host_matches, rate_limit_error, Provider, ProgressUpdate, ProviderDefaults};

pub struct GDriveProvider;

const GDRIVE_FOLDER_MIME: &str = "application/vnd.google-apps.folder";

impl GDriveProvider {
    pub fn matches(url: &str) -> bool {
        host_matches(url, &["drive.google.com"])
    }

    pub fn is_folder_url(url: &str) -> bool {
        url.contains("/drive/folders/")
    }

    pub fn extract_folder_id(url: &str) -> Option<String> {
        let pos = url.find("/drive/folders/")?;
        let after = &url[pos + 15..];
        let id = after
            .split('/')
            .next()?
            .split('?')
            .next()?
            .split('#')
            .next()?
            .trim();
        if id.is_empty() {
            None
        } else {
            Some(id.to_string())
        }
    }

    // Extrai o ID do arquivo de diferentes formatos de URL do Google Drive
    // Suporta:
    //   /file/d/{id}/view
    //   /open?id={id}
    //   /uc?id={id}&export=download
    pub fn extract_id(url: &str) -> Option<String> {
        // Formato 1: /file/d/{id}/...
        if let Some(pos) = url.find("/file/d/") {
            let after = &url[pos + 8..]; // 8 = len("/file/d/")
            let id = after.split('/').next()?.split('?').next()?;
            if !id.is_empty() {
                return Some(id.to_string());
            }
        }

        // Formato 2: ?id={id} (usado em /open e /uc)
        if let Some(pos) = url.find("id=") {
            let after = &url[pos + 3..]; // 3 = len("id=")
            let id = after.split('&').next()?.split('#').next()?;
            if !id.is_empty() {
                return Some(id.to_string());
            }
        }

        None
    }

    // Monta a URL de download direto
    // Sem confirm para permitir que o Google devolva a página com uuid atual
    // quando o arquivo é grande demais para scan de vírus.
    fn download_url(id: &str) -> String {
        format!("https://drive.google.com/uc?export=download&id={id}")
    }

    fn is_binary_response(resp: &reqwest::Response) -> bool {
        resp.headers().get("content-disposition").is_some()
            || resp
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .map(|value| !value.to_ascii_lowercase().contains("text/html"))
                .unwrap_or(false)
    }

    fn is_html_response(resp: &reqwest::Response) -> bool {
        resp.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(|value| value.to_ascii_lowercase().contains("text/html"))
            .unwrap_or(false)
    }

    async fn ensure_download_response(resp: reqwest::Response) -> Result<reqwest::Response> {
        if !Self::is_html_response(&resp) {
            return Ok(resp);
        }

        let html = resp.text().await.unwrap_or_default();
        let title = Self::extract_title(&html, "Google Drive retornou uma página HTML");
        let messages = regex::Regex::new(r#"(?is)<p[^>]*class="uc-(?:error|warning)-(?:caption|subcaption)"[^>]*>(.*?)</p>"#)
            .ok()
            .map(|re| {
                re.captures_iter(&html)
                    .filter_map(|captures| {
                        let value = Self::strip_html(&captures[1]);
                        if value.trim().is_empty() { None } else { Some(value) }
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let message = if messages.is_empty() {
            title.clone()
        } else {
            messages.join(" ")
        };

        let lower = message.to_lowercase();
        if lower.contains("too many users")
            || lower.contains("quota exceeded")
            || lower.contains("view or download this file at this time")
        {
            return Err(rate_limit_error(
                24 * 60 * 60,
                "Google Drive atingiu a cota pública deste arquivo. O arquivo existe, mas o Google bloqueou downloads públicos temporariamente; vamos tentar novamente em até 24h.",
            ));
        }

        Err(anyhow!("Google Drive não liberou o arquivo: {message}"))
    }

    async fn existing_file_looks_like_html(path: &str) -> bool {
        let Ok(mut file) = tokio::fs::File::open(path).await else {
            return false;
        };
        let mut buf = vec![0u8; 512];
        let Ok(n) = file.read(&mut buf).await else {
            return false;
        };
        let head = String::from_utf8_lossy(&buf[..n]).to_ascii_lowercase();
        head.contains("<!doctype html") || head.contains("<html")
    }

    fn decode_html(value: &str) -> String {
        value
            .replace("&amp;", "&")
            .replace("&quot;", "\"")
            .replace("&#039;", "'")
            .replace("&#39;", "'")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
    }

    fn strip_html(value: &str) -> String {
        let without_tags = regex::Regex::new(r#"(?is)<[^>]+>"#)
            .ok()
            .map(|re| re.replace_all(value, " ").to_string())
            .unwrap_or_else(|| value.to_string());
        Self::decode_html(&without_tags)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn extract_title(html: &str, fallback: &str) -> String {
        regex::Regex::new(r#"(?is)<title>\s*(.*?)\s*(?:-\s*Google Drive)?\s*</title>"#)
            .ok()
            .and_then(|re| re.captures(html))
            .map(|captures| Self::decode_html(&captures[1]))
            .map(|title| title.trim().to_string())
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| fallback.to_string())
    }

    fn extract_drive_ivd(html: &str) -> Option<String> {
        let marker = "window['_DRIVE_ivd'] = '";
        let start = html.find(marker)? + marker.len();
        let end = html[start..].find("';")? + start;
        Some(Self::decode_js_string(&html[start..end]).replace("\\=", "="))
    }

    fn decode_js_string(value: &str) -> String {
        let mut decoded = String::with_capacity(value.len());
        let mut chars = value.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch != '\\' {
                decoded.push(ch);
                continue;
            }

            match chars.next() {
                Some('x') => {
                    let hex = chars.by_ref().take(2).collect::<String>();
                    if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                        decoded.push(byte as char);
                    }
                }
                Some('u') => {
                    let hex = chars.by_ref().take(4).collect::<String>();
                    if let Ok(code) = u32::from_str_radix(&hex, 16) {
                        if let Some(value) = char::from_u32(code) {
                            decoded.push(value);
                        }
                    }
                }
                Some('/') => decoded.push('/'),
                Some('\\') => decoded.push('\\'),
                Some('"') => decoded.push('"'),
                Some('\'') => decoded.push('\''),
                Some('n') => decoded.push('\n'),
                Some('r') => decoded.push('\r'),
                Some('t') => decoded.push('\t'),
                Some(other) => {
                    decoded.push('\\');
                    decoded.push(other);
                }
                None => decoded.push('\\'),
            }
        }
        decoded
    }

    #[cfg(test)]
    fn parse_folder_children_from_ivd(ivd: &str) -> Result<Vec<FileChildInfo>> {
        Self::parse_folder_children_from_ivd_with_prefix(ivd, "")
    }

    fn parse_folder_children_from_ivd_with_prefix(
        ivd: &str,
        prefix: &str,
    ) -> Result<Vec<FileChildInfo>> {
        let value: Value = serde_json::from_str(ivd)?;
        let entries = value
            .get(0)
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow!("Google Drive não retornou a lista de arquivos da pasta"))?;

        let children = entries
            .iter()
            .filter_map(|entry| {
                let file = entry.as_array()?;
                let id = file.get(0)?.as_str()?;
                let filename = file.get(2)?.as_str()?;
                let mime_type = file.get(3).and_then(Value::as_str).map(String::from);
                let size = file.get(13).and_then(Value::as_u64).unwrap_or(0);
                if id.is_empty() || filename.is_empty() {
                    return None;
                }
                let is_folder = mime_type.as_deref() == Some(GDRIVE_FOLDER_MIME);
                let path = if prefix.is_empty() {
                    filename.to_string()
                } else {
                    format!("{}/{}", prefix.trim_matches('/'), filename)
                };
                let source_url = if is_folder {
                    format!("https://drive.google.com/drive/folders/{id}")
                } else {
                    format!("https://drive.google.com/file/d/{id}/view")
                };
                Some(FileChildInfo {
                    filename: filename.to_string(),
                    size,
                    mime_type,
                    is_folder,
                    path: Some(path),
                    source_url: Some(source_url),
                    bytes_downloaded: None,
                    speed_bps: None,
                    eta_secs: None,
                    status: None,
                })
            })
            .collect::<Vec<_>>();

        if children.is_empty() {
            return Err(anyhow!("Pasta do Google Drive vazia ou sem arquivos acessíveis"));
        }
        Ok(children)
    }

    fn unique_child_path(path: &str, seen: &mut HashMap<String, usize>) -> String {
        let count = seen.entry(path.to_string()).or_insert(0);
        *count += 1;
        if *count == 1 {
            return path.to_string();
        }

        let duplicate_index = *count;
        let (dir, filename) = path
            .rsplit_once('/')
            .map(|(dir, filename)| (Some(dir), filename))
            .unwrap_or((None, path));
        let renamed = filename
            .rfind('.')
            .filter(|pos| *pos > 0)
            .map(|pos| {
                format!(
                    "{} ({}){}",
                    &filename[..pos],
                    duplicate_index,
                    &filename[pos..]
                )
            })
            .unwrap_or_else(|| format!("{filename} ({duplicate_index})"));

        dir.map(|dir| format!("{dir}/{renamed}")).unwrap_or(renamed)
    }

    async fn fetch_folder_html(client: &reqwest::Client, folder_id: &str) -> Result<String> {
        let page_url = format!("https://drive.google.com/drive/folders/{folder_id}");
        Ok(client
            .get(&page_url)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?)
    }

    async fn collect_folder_file_children(
        client: &reqwest::Client,
        root_folder_id: &str,
        root_html: String,
        selected_sources: Option<&HashSet<String>>,
    ) -> Result<Vec<FileChildInfo>> {
        let mut pending = vec![(root_folder_id.to_string(), String::new(), Some(root_html))];
        let mut visited = HashSet::new();
        let mut seen_file_paths = HashMap::new();
        let mut files = Vec::new();

        while let Some((folder_id, prefix, html)) = pending.pop() {
            if selected_sources
                .map(|selected| files.len() >= selected.len())
                .unwrap_or(false)
            {
                break;
            }

            if !visited.insert(folder_id.clone()) {
                continue;
            }

            let html = match html {
                Some(value) => value,
                None => Self::fetch_folder_html(client, &folder_id).await?,
            };
            let ivd = Self::extract_drive_ivd(&html)
                .ok_or_else(|| anyhow!("Google Drive não retornou metadados da pasta pública"))?;
            let children = Self::parse_folder_children_from_ivd_with_prefix(&ivd, &prefix)?;

            for mut child in children {
                if child.is_folder {
                    let source_url = child
                        .source_url
                        .as_deref()
                        .ok_or_else(|| anyhow!("Subpasta do Google Drive sem URL"))?;
                    let child_id = Self::extract_folder_id(source_url)
                        .ok_or_else(|| {
                            anyhow!("Subpasta do Google Drive com URL inválida: {source_url}")
                        })?;
                    let child_prefix = child.path.clone().unwrap_or_else(|| child.filename.clone());
                    pending.push((child_id, child_prefix, None));
                } else {
                    if selected_sources
                        .map(|selected| {
                            child
                                .source_url
                                .as_ref()
                                .map(|source_url| !selected.contains(source_url))
                                .unwrap_or(true)
                        })
                        .unwrap_or(false)
                    {
                        continue;
                    }

                    let path = child.path.clone().unwrap_or_else(|| child.filename.clone());
                    let unique_path = Self::unique_child_path(&path, &mut seen_file_paths);
                    if unique_path != path {
                        child.filename = unique_path
                            .rsplit('/')
                            .next()
                            .unwrap_or(&child.filename)
                            .to_string();
                        child.path = Some(unique_path);
                    }
                    files.push(child);
                }
            }
        }

        if files.is_empty() {
            let message = if selected_sources.is_some() {
                "Itens selecionados não encontrados na pasta do Google Drive"
            } else {
                "Pasta do Google Drive vazia ou sem arquivos acessíveis"
            };
            return Err(anyhow!(message));
        }

        Ok(files)
    }

    async fn get_folder_info_with_selection(
        client: &reqwest::Client,
        url: &str,
        selected_sources: Option<&HashSet<String>>,
    ) -> Result<FileInfo> {
        let folder_id = Self::extract_folder_id(url)
            .ok_or_else(|| anyhow!("URL de pasta do Google Drive inválida: {url}"))?;
        let html = Self::fetch_folder_html(client, &folder_id).await?;
        let folder_name = Self::extract_title(&html, &format!("gdrive_{folder_id}"));
        let children =
            Self::collect_folder_file_children(client, &folder_id, html, selected_sources).await?;
        let total_size = children.iter().map(|child| child.size).sum();

        Ok(FileInfo {
            filename: folder_name,
            size: total_size,
            mime_type: None,
            is_folder: true,
            children: Some(children),
            ..Default::default()
        })
    }

    async fn get_folder_info(client: &reqwest::Client, url: &str) -> Result<FileInfo> {
        Self::get_folder_info_with_selection(client, url, None).await
    }

    fn extract_confirm_download_url(html: &str) -> Option<String> {
        let action = regex::Regex::new(r#"id="download-form"[^>]*action="([^"]+)""#)
            .ok()?
            .captures(html)
            .map(|captures| captures[1].to_string())?;

        let mut url = reqwest::Url::parse(&action).ok()?;
        for captures in regex::Regex::new(r#"type="hidden"\s+name="([^"]+)"\s+value="([^"]*)""#)
            .ok()?
            .captures_iter(html)
        {
            url.query_pairs_mut()
                .append_pair(&captures[1], &Self::decode_html(&captures[2]));
        }

        Some(url.to_string())
    }

    fn extract_warning_page_metadata(html: &str, fallback_id: &str) -> (String, u64) {
        let filename = regex::Regex::new(r#"(?is)<span[^>]*class="uc-name-size"[^>]*>\s*<a [^>]+>([^<]+)</a>"#)
            .ok()
            .and_then(|re| re.captures(html))
            .map(|captures| Self::decode_html(&captures[1]))
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| format!("gdrive_{fallback_id}"));

        let size = regex::Regex::new(r#"(?is)\(([0-9]+(?:\.[0-9]+)?)\s*([KMGT]?B?)\).*?too large"#)
            .ok()
            .and_then(|re| re.captures(html))
            .map(|captures| {
                let number = captures[1].parse::<f64>().unwrap_or(0.0);
                let unit = captures[2].to_ascii_uppercase();
                let multiplier = match unit.as_str() {
                    "KB" | "K" => 1024f64,
                    "MB" | "M" => 1024f64.powi(2),
                    "GB" | "G" => 1024f64.powi(3),
                    "TB" | "T" => 1024f64.powi(4),
                    _ => 1f64,
                };
                (number * multiplier).round() as u64
            })
            .unwrap_or(0);

        (filename, size)
    }

    async fn resolve_download_url(client: &reqwest::Client, id: &str) -> Result<(String, Option<String>, u64)> {
        let initial_url = Self::download_url(id);
        let response = client.get(&initial_url).send().await?.error_for_status()?;

        if Self::is_binary_response(&response) {
            let filename = response
                .headers()
                .get("content-disposition")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| {
                    if let Some(pos) = s.find("filename=\"") {
                        let start = pos + 10;
                        let end = s[start..].find('"')? + start;
                        Some(s[start..end].to_string())
                    } else {
                        None
                    }
                });
            return Ok((initial_url, filename, response.content_length().unwrap_or(0)));
        }

        let html = response.text().await?;
        let (filename, size) = Self::extract_warning_page_metadata(&html, id);
        let confirm_url = Self::extract_confirm_download_url(&html)
            .ok_or_else(|| anyhow!("Google Drive não expôs o link final de confirmação"))?;
        Ok((confirm_url, Some(filename), size))
    }

    async fn probe_total_size(client: &reqwest::Client, download_url: &str) -> Result<u64> {
        let ranged = client
            .get(download_url)
            .header("Range", "bytes=0-0")
            .send()
            .await?
            .error_for_status()?;

        Ok(
            <Self as ProviderDefaults>::response_total_bytes(&ranged, 0)
                .max(ranged.content_length().unwrap_or(0)),
        )
    }

}

impl ProviderDefaults for GDriveProvider {}

impl Provider for GDriveProvider {
    fn name(&self) -> &str { "Google Drive" }

    fn get_file_info<'a>(&'a self, url: &'a str)
        -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FileInfo>> + Send + 'a>>
    {
        Box::pin(async move {
            if Self::is_folder_url(url) {
                let client = <Self as ProviderDefaults>::http_client()?;
                return Self::get_folder_info(&client, url).await;
            }

            let id = Self::extract_id(url)
                .ok_or_else(|| anyhow!("URL do Google Drive inválida: {url}"))?;

            let client = <Self as ProviderDefaults>::http_client()?;
            let (download_url, hinted_filename, hinted_size) = Self::resolve_download_url(&client, &id).await?;
            let resp = client.head(&download_url).send().await?;

            // Tenta extrair o nome do arquivo do header Content-Disposition
            // Exemplo: attachment; filename="arquivo.zip"
            let filename = resp
                .headers()
                .get("content-disposition")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| {
                    if let Some(pos) = s.find("filename=\"") {
                        let start = pos + 10; // 10 = len(filename=")
                        let end = s[start..].find('"')? + start;
                        Some(s[start..end].to_string())
                    } else if let Some(pos) = s.find("filename=") {
                        let start = pos + 9;
                        Some(s[start..].split(';').next()?.trim().to_string())
                    } else {
                        None
                    }
                })
                .or(hinted_filename)
                .unwrap_or_else(|| format!("gdrive_{id}"));

            let resolved_size = resp.content_length().unwrap_or(hinted_size);
            let size = if resolved_size > 0 {
                resolved_size
            } else {
                Self::probe_total_size(&client, &download_url).await.unwrap_or(0)
            };

            Ok(FileInfo {
                filename,
                size,
                mime_type: resp
                    .headers()
                    .get("content-type")
                    .and_then(|v| v.to_str().ok())
                    .map(String::from),
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
            if Self::is_folder_url(url) {
                let client = <Self as ProviderDefaults>::http_client()?;
                let selected_set = selected_children
                    .as_ref()
                    .filter(|items| !items.is_empty())
                    .map(|items| items.iter().cloned().collect::<HashSet<_>>());
                let info =
                    Self::get_folder_info_with_selection(&client, url, selected_set.as_ref())
                        .await?;
                let mut children = info.children.unwrap_or_default();
                if let Some(selected_set) = selected_set.as_ref() {
                    children.retain(|child| {
                        child
                            .source_url
                            .as_ref()
                            .map(|source_url| selected_set.contains(source_url))
                            .unwrap_or(false)
                    });
                }

                if children.is_empty() {
                    return Err(anyhow!("Pasta do Google Drive vazia ou sem arquivos acessíveis"));
                }

                tokio::fs::create_dir_all(dest_path).await?;
                let total_size: u64 = children.iter().map(|child| child.size).sum();
                let started_at = tokio::time::Instant::now();
                let mut downloaded_total = 0u64;
                let mut session_downloaded = 0u64;

                for child in &children {
                    let source_url = child
                        .source_url
                        .as_deref()
                        .ok_or_else(|| anyhow!("Item da pasta do Google Drive sem URL"))?;
                    let id = Self::extract_id(source_url)
                        .ok_or_else(|| anyhow!("Item da pasta do Google Drive com URL inválida: {source_url}"))?;
                    let (download_url, _hinted_filename, _hinted_size) =
                        Self::resolve_download_url(&client, &id).await?;
                    let child_path = child.path.clone().unwrap_or_else(|| child.filename.clone());
                    let output_path = format!("{}/{}", dest_path.trim_end_matches('/'), child_path);
                    if let Some(parent_dir) = std::path::Path::new(&output_path).parent() {
                        tokio::fs::create_dir_all(parent_dir).await?;
                    }

                    let mut existing_bytes = tokio::fs::metadata(&output_path)
                        .await
                        .ok()
                        .filter(|meta| meta.is_file())
                        .map(|meta| meta.len())
                        .unwrap_or(0);
                    if existing_bytes > 0 && Self::existing_file_looks_like_html(&output_path).await {
                        existing_bytes = 0;
                    }
                    if child.size > 0 && existing_bytes >= child.size {
                        downloaded_total += child.size;
                        continue;
                    }

                    let (resp, resumed) =
                        <Self as ProviderDefaults>::send_with_resume_fallback(&client, &download_url, &output_path, existing_bytes).await?;
                    let resp = Self::ensure_download_response(resp).await?;
                    if resumed {
                        downloaded_total += existing_bytes;
                    }
                    let mut file = if resumed {
                        OpenOptions::new().create(true).append(true).open(&output_path).await?
                    } else {
                        tokio::fs::File::create(&output_path).await?
                    };
                    let mut stream = resp.bytes_stream();
                    let mut child_session_downloaded = 0u64;
                    let child_started_at = tokio::time::Instant::now();

                    while let Some(chunk) = stream.next().await {
                        let chunk = chunk?;
                        file.write_all(&chunk).await?;
                        let chunk_len = chunk.len() as u64;
                        downloaded_total += chunk_len;
                        session_downloaded += chunk_len;
                        child_session_downloaded += chunk_len;

                        let child_downloaded = if resumed { existing_bytes } else { 0 } + child_session_downloaded;
                        let child_elapsed = child_started_at.elapsed().as_secs_f64();
                        let child_speed = if child_elapsed > 0.0 {
                            (child_session_downloaded as f64 / child_elapsed) as u64
                        } else {
                            0
                        };
                        let child_eta = if child_speed > 0 && child.size > child_downloaded {
                            (child.size - child_downloaded) / child_speed
                        } else {
                            0
                        };

                        let _ = progress_tx
                            .send(ProgressUpdate {
                                bytes_downloaded: downloaded_total,
                                total_bytes: total_size,
                                child_path: Some(child_path.clone()),
                                child_filename: Some(child.filename.clone()),
                                child_bytes_downloaded: Some(child_downloaded),
                                child_total_bytes: Some(child.size),
                                child_speed_bps: Some(child_speed),
                                child_eta_secs: Some(child_eta),
                            })
                            .await;
                        apply_speed_limit(started_at, session_downloaded, &speed_limit_bps).await;
                    }

                    file.flush().await?;
                }

                return Ok(downloaded_total);
            }

            let id = Self::extract_id(url)
                .ok_or_else(|| anyhow!("URL do Google Drive inválida: {url}"))?;

            let client = <Self as ProviderDefaults>::http_client()?;
            let (download_url, _hinted_filename, _hinted_size) = Self::resolve_download_url(&client, &id).await?;

            let mut existing_bytes = tokio::fs::metadata(dest_path)
                .await
                .ok()
                .filter(|meta| meta.is_file())
                .map(|meta| meta.len())
                .unwrap_or(0);
            if existing_bytes > 0 && Self::existing_file_looks_like_html(dest_path).await {
                existing_bytes = 0;
            }

            let (resp, resumed) =
                <Self as ProviderDefaults>::send_with_resume_fallback(&client, &download_url, dest_path, existing_bytes).await?;
            let resp = Self::ensure_download_response(resp).await?;
            let total = if resumed {
                <Self as ProviderDefaults>::response_total_bytes(&resp, existing_bytes)
            } else {
                resp.content_length().unwrap_or(0)
            };
            let mut file = if resumed {
                OpenOptions::new().create(true).append(true).open(dest_path).await?
            } else {
                tokio::fs::File::create(dest_path).await?
            };
            let mut stream = resp.bytes_stream();
            let mut downloaded: u64 = if resumed { existing_bytes } else { 0 };
            let mut session_downloaded: u64 = 0;
            let started_at = tokio::time::Instant::now();

            while let Some(chunk) = stream.next().await {
                let chunk = chunk?;
                file.write_all(&chunk).await?;
                let chunk_len = chunk.len() as u64;
                downloaded += chunk_len;
                session_downloaded += chunk_len;
                let _ = progress_tx
                    .send(ProgressUpdate {
                        bytes_downloaded: downloaded,
                        total_bytes: total,
                        child_path: None,
                        child_filename: None,
                        child_bytes_downloaded: None,
                        child_total_bytes: None,
                        child_speed_bps: None,
                        child_eta_secs: None,
                    })
                    .await;
                apply_speed_limit(started_at, session_downloaded, &speed_limit_bps).await;
            }

            file.flush().await?;
            Ok(downloaded)
        })
    }
}

#[cfg(test)]
#[path = "tests/gdrive.rs"]
mod tests;
