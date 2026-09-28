use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;

use crate::models::FileInfo;
use super::{apply_speed_limit, host_matches, unsupported_error, ProgressUpdate, Provider, ProviderDefaults};

pub struct SharePointProvider;

impl SharePointProvider {
    pub fn matches(url: &str) -> bool {
        host_matches(url, &["onedrive.live.com", "1drv.ms"])
            || reqwest::Url::parse(url)
                .ok()
                .and_then(|parsed| parsed.host_str().map(str::to_ascii_lowercase))
                .map(|host| host.ends_with("sharepoint.com"))
                .unwrap_or(false)
    }

    fn auth_required_error() -> anyhow::Error {
        unsupported_error("OneDrive/SharePoint")
    }

    fn direct_download_error() -> anyhow::Error {
        unsupported_error("OneDrive/SharePoint")
    }

    fn build_download_url(url: &str) -> Result<reqwest::Url> {
        let mut parsed = reqwest::Url::parse(url)
            .map_err(|_| anyhow!("URL do OneDrive/SharePoint inválida: {url}"))?;

        let mut pairs = parsed
            .query_pairs()
            .map(|(key, value)| (key.into_owned(), value.into_owned()))
            .collect::<Vec<_>>();

        let mut has_download = false;
        for (key, value) in &mut pairs {
            if key == "download" {
                *value = "1".to_string();
                has_download = true;
            }
        }
        if !has_download {
            pairs.push(("download".to_string(), "1".to_string()));
        }

        parsed.set_query(None);
        {
            let mut query = parsed.query_pairs_mut();
            for (key, value) in pairs {
                query.append_pair(&key, &value);
            }
        }

        Ok(parsed)
    }

    fn looks_like_login(url: &reqwest::Url) -> bool {
        match url.domain() {
            Some(domain) => {
                domain.eq_ignore_ascii_case("login.microsoftonline.com")
                    || domain.eq_ignore_ascii_case("login.live.com")
            }
            None => false,
        }
    }

    fn is_binary_response(resp: &reqwest::Response) -> bool {
        resp.headers().get("content-disposition").is_some()
            || resp
                .headers()
                .get("content-type")
                .and_then(|value| value.to_str().ok())
                .map(|value| {
                    let lower = value.to_ascii_lowercase();
                    !lower.contains("text/html") && !lower.contains("application/json")
                })
                .unwrap_or(false)
    }

    fn decode_path_segment(value: &str) -> String {
        let bytes = value.as_bytes();
        let mut decoded = Vec::with_capacity(bytes.len());
        let mut idx = 0;

        while idx < bytes.len() {
            if bytes[idx] == b'%' && idx + 2 < bytes.len() {
                let hex = &value[idx + 1..idx + 3];
                if let Ok(num) = u8::from_str_radix(hex, 16) {
                    decoded.push(num);
                    idx += 3;
                    continue;
                }
            }

            if bytes[idx] == b'+' {
                decoded.push(b' ');
            } else {
                decoded.push(bytes[idx]);
            }
            idx += 1;
        }

        String::from_utf8_lossy(&decoded).into_owned()
    }

    fn filename_from_response(resp: &reqwest::Response) -> Option<String> {
        let disposition = resp.headers().get("content-disposition")?.to_str().ok()?;
        for part in disposition.split(';') {
            let trimmed = part.trim();
            if let Some(value) = trimmed.strip_prefix("filename*=") {
                let value = value.split("''").last().unwrap_or(value);
                let decoded = Self::decode_path_segment(value.trim_matches('"'));
                if !decoded.is_empty() {
                    return Some(decoded);
                }
            }
            if let Some(value) = trimmed.strip_prefix("filename=") {
                let decoded = Self::decode_path_segment(value.trim_matches('"'));
                if !decoded.is_empty() {
                    return Some(decoded);
                }
            }
        }
        None
    }

    fn fallback_filename(url: &reqwest::Url) -> String {
        let candidate = url
            .path_segments()
            .and_then(|segments| segments.last())
            .filter(|segment| !segment.is_empty())
            .map(Self::decode_path_segment)
            .filter(|segment| !segment.is_empty())
            .unwrap_or_else(|| "arquivo_onedrive".to_string());

        <Self as ProviderDefaults>::safe_filename(&candidate, "arquivo_onedrive")
    }

    async fn request_public_download(url: &str) -> Result<reqwest::Response> {
        let client = <Self as ProviderDefaults>::http_client()?;
        let download_url = Self::build_download_url(url)?;
        Ok(client.get(download_url).send().await?.error_for_status()?)
    }

    async fn open_download_response(
        url: &str,
        dest_path: &str,
        existing_bytes: u64,
    ) -> Result<(reqwest::Response, bool)> {
        let client = <Self as ProviderDefaults>::http_client()?;
        let download_url = Self::build_download_url(url)?;

        if existing_bytes == 0 {
            return Ok((client.get(download_url).send().await?.error_for_status()?, false));
        }

        let ranged = client
            .get(download_url.clone())
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
            let fresh = client.get(download_url).send().await?.error_for_status()?;
            return Ok((fresh, false));
        }

        Ok((ranged.error_for_status()?, false))
    }
}

impl ProviderDefaults for SharePointProvider {}

impl Provider for SharePointProvider {
    fn name(&self) -> &str { "OneDrive" }

    fn get_file_info<'a>(
        &'a self,
        url: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FileInfo>> + Send + 'a>> {
        Box::pin(async move {
            let resp = Self::request_public_download(url).await?;

            if Self::looks_like_login(resp.url()) {
                return Err(Self::auth_required_error());
            }

            if !Self::is_binary_response(&resp) {
                return Err(Self::direct_download_error());
            }

            let filename = Self::filename_from_response(&resp)
                .unwrap_or_else(|| Self::fallback_filename(resp.url()));
            let size = resp.content_length().unwrap_or(0);
            let mime_type = resp
                .headers()
                .get("content-type")
                .and_then(|value| value.to_str().ok())
                .map(|value| value.to_string());

            Ok(FileInfo {
                filename,
                size,
                mime_type,
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
        _selected_children: Option<Vec<String>>,
        progress_tx: tokio::sync::mpsc::Sender<ProgressUpdate>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u64>> + Send + 'a>> {
        Box::pin(async move {
            let existing_bytes = tokio::fs::metadata(dest_path)
                .await
                .ok()
                .filter(|meta| meta.is_file())
                .map(|meta| meta.len())
                .unwrap_or(0);

            let (resp, resumed) = Self::open_download_response(url, dest_path, existing_bytes).await?;

            if Self::looks_like_login(resp.url()) {
                return Err(Self::auth_required_error());
            }

            if !Self::is_binary_response(&resp) {
                return Err(Self::direct_download_error());
            }

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

            let mut downloaded = if resumed { existing_bytes } else { 0u64 };
            let mut session_downloaded = 0u64;
            let started_at = tokio::time::Instant::now();
            let mut stream = resp.bytes_stream();

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
        })
    }
}

#[cfg(test)]
#[path = "tests/sharepoint.rs"]
mod tests;
