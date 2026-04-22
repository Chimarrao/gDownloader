use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;

use crate::models::FileInfo;
use super::{apply_speed_limit, host_matches, try_parallel_download, Provider, ProgressUpdate, ProviderDefaults};

pub struct GDriveProvider;

impl GDriveProvider {
    pub fn matches(url: &str) -> bool {
        host_matches(url, &["drive.google.com"])
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
    // confirm=t evita a página de aviso para arquivos grandes
    fn download_url(id: &str) -> String {
        format!("https://drive.google.com/uc?export=download&id={id}&confirm=t")
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

    fn decode_html(value: &str) -> String {
        value
            .replace("&amp;", "&")
            .replace("&quot;", "\"")
            .replace("&#039;", "'")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
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
        let filename = regex::Regex::new(r#"<span class="uc-name-size"><a [^>]+>([^<]+)</a>"#)
            .ok()
            .and_then(|re| re.captures(html))
            .map(|captures| Self::decode_html(&captures[1]))
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| format!("gdrive_{fallback_id}"));

        let size = regex::Regex::new(r#"\(([^)]+)\)\s+is too large"#)
            .ok()
            .and_then(|re| re.captures(html))
            .map(|captures| super::parse_human_size(&captures[1]))
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

}

impl ProviderDefaults for GDriveProvider {}

impl Provider for GDriveProvider {
    fn name(&self) -> &str { "Google Drive" }

    fn get_file_info<'a>(&'a self, url: &'a str)
        -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FileInfo>> + Send + 'a>>
    {
        Box::pin(async move {
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

            Ok(FileInfo {
                filename,
                size: resp.content_length().unwrap_or(hinted_size),
                mime_type: resp
                    .headers()
                    .get("content-type")
                    .and_then(|v| v.to_str().ok())
                    .map(String::from),
                is_folder: false,
                children: None,
            })
        })
    }

    fn download<'a>(
        &'a self,
        url: &'a str,
        dest_path: &'a str,
        speed_limit_bps: Option<u64>,
        parallel_parts: usize,
        _selected_children: Option<Vec<String>>,
        progress_tx: tokio::sync::mpsc::Sender<ProgressUpdate>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u64>> + Send + 'a>>
    {
        Box::pin(async move {
            let id = Self::extract_id(url)
                .ok_or_else(|| anyhow!("URL do Google Drive inválida: {url}"))?;

            let client = <Self as ProviderDefaults>::http_client()?;
            let (download_url, _hinted_filename, _hinted_size) = Self::resolve_download_url(&client, &id).await?;

            let existing_bytes = tokio::fs::metadata(dest_path)
                .await
                .ok()
                .filter(|meta| meta.is_file())
                .map(|meta| meta.len())
                .unwrap_or(0);

            if existing_bytes == 0 {
                if let Some(downloaded) = try_parallel_download(
                    &client,
                    &download_url,
                    dest_path,
                    speed_limit_bps,
                    parallel_parts,
                    progress_tx.clone(),
                )
                .await?
                {
                    return Ok(downloaded);
                }
            }

            let (resp, resumed) =
                <Self as ProviderDefaults>::send_with_resume_fallback(&client, &download_url, dest_path, existing_bytes).await?;
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
                apply_speed_limit(started_at, session_downloaded, speed_limit_bps).await;
            }

            file.flush().await?;
            Ok(downloaded)
        })
    }
}
