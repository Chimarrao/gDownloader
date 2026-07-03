use anyhow::{anyhow, Result};
use serde::Deserialize;
use serde_json::json;
use tokio::time::{sleep, Duration};

use crate::models::FileInfo;

use super::{host_matches, parse_human_size, path_segments, removed_error, ProgressUpdate, Provider, ProviderDefaults};

fn electron_proxy_port() -> Option<u16> {
    std::env::var("KATFILE_PROXY_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .filter(|&value| value > 0)
}

async fn proxy_action(payload: serde_json::Value) -> Result<serde_json::Value> {
    let port = electron_proxy_port().ok_or_else(|| anyhow!("Helper local do Katfile não disponível"))?;
    let proxy_url = format!("http://127.0.0.1:{port}/");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(180))
        .build()?;
    let response = client.post(&proxy_url).json(&payload).send().await?;
    Ok(response.json::<serde_json::Value>().await?)
}

#[derive(Debug, Deserialize)]
struct HelperJobStatus {
    status: String,
    #[serde(default, rename = "bytesDownloaded")]
    bytes_downloaded: u64,
    #[serde(default, rename = "totalBytes")]
    total_bytes: u64,
    filename: Option<String>,
    error: Option<String>,
}

pub struct KatfileProvider;

impl KatfileProvider {
    pub fn matches(url: &str) -> bool {
        if !host_matches(
            url,
            &["katfile.ws", "www.katfile.ws", "katfile.com", "www.katfile.com", "katfile.space", "www.katfile.space"],
        ) {
            return false;
        }

        matches!(
            path_segments(url).as_slice(),
            [code] if code.len() >= 8 && code.chars().all(|ch| ch.is_ascii_alphanumeric())
        )
    }

    fn is_removed_page(html: &str) -> bool {
        let lower = html.to_lowercase();
        lower.contains("404-remove.png")
            || lower.contains("file has been removed")
            || lower.contains("file not found")
            || lower.contains("such file does not exist")
    }

    fn decode_html(value: &str) -> String {
        value.replace("&gt;", ">")
            .replace("&lt;", "<")
            .replace("&amp;", "&")
            .replace("&quot;", "\"")
            .replace("&#039;", "'")
            .trim()
            .to_string()
    }

    fn parse_human_size(value: &str) -> u64 {
        parse_human_size(value)
    }

    fn extract_filename(html: &str) -> Option<String> {
        let from_hidden = regex::Regex::new(r#"(?is)<input[^>]*name=["']fname["'][^>]*value=["']([^"']+)["']"#)
            .ok()
            .and_then(|re| re.captures(html))
            .map(|captures| Self::decode_html(&captures[1]));

        from_hidden.and_then(|value| {
            let clean = value.trim();
            if clean.is_empty() {
                None
            } else {
                Some(clean.to_string())
            }
        })
    }

    fn extract_size(html: &str) -> u64 {
        let from_inline = regex::Regex::new(r#"(?is)<span[^>]*id=["']fsize["'][^>]*>\s*([^<]+)\s*</span>"#)
            .ok()
            .and_then(|re| re.captures(html))
            .map(|captures| Self::decode_html(&captures[1]))
            .unwrap_or_default();

        let parsed = Self::parse_human_size(&from_inline);
        if parsed > 0 {
            return parsed;
        }

        regex::Regex::new(r#"([0-9]+(?:[.,][0-9]+)?)\s*(KB|MB|GB|TB)"#)
            .ok()
            .and_then(|re| re.captures(html))
            .map(|captures| Self::parse_human_size(&format!("{} {}", &captures[1], &captures[2])))
            .unwrap_or(0)
    }

    async fn helper_start_download(source_url: &str, dest_path: &str) -> Result<String> {
        let json = proxy_action(json!({
            "action": "katfile_download_file",
            "url": source_url,
            "destPath": dest_path,
        })).await?;

        json["jobId"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| anyhow!("Helper local do Katfile não retornou um jobId"))
    }

    async fn helper_job_status(job_id: &str) -> Result<HelperJobStatus> {
        let json = proxy_action(json!({
            "action": "katfile_job_status",
            "jobId": job_id,
        })).await?;

        Ok(serde_json::from_value::<HelperJobStatus>(json)?)
    }
}

impl ProviderDefaults for KatfileProvider {}

impl Provider for KatfileProvider {
    fn name(&self) -> &str { "Katfile" }

    fn get_file_info<'a>(
        &'a self,
        url: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FileInfo>> + Send + 'a>> {
        Box::pin(async move {
            let client = <Self as ProviderDefaults>::http_client()?;
            let html = client.get(url).send().await?.error_for_status()?.text().await?;

            if Self::is_removed_page(&html) {
                return Err(removed_error("Katfile"));
            }

            let filename = Self::extract_filename(&html)
                .unwrap_or_else(|| "arquivo_katfile".to_string());
            let size = Self::extract_size(&html);

            Ok(FileInfo {
                filename: <Self as ProviderDefaults>::safe_filename(&filename, "arquivo_katfile"),
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
        _speed_limit_bps: Option<u64>,
        _parallel_parts: usize,
        _selected_children: Option<Vec<String>>,
        progress_tx: tokio::sync::mpsc::Sender<ProgressUpdate>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u64>> + Send + 'a>> {
        Box::pin(async move {
            let info = self.get_file_info(url).await?;
            let expected_size = info.size;

            if let Some(parent) = std::path::Path::new(dest_path).parent() {
                tokio::fs::create_dir_all(parent).await?;
            }

            let job_id = Self::helper_start_download(url, dest_path).await?;

            loop {
                sleep(Duration::from_millis(500)).await;
                let status = Self::helper_job_status(&job_id).await?;

                let total_bytes = if expected_size > 0 {
                    expected_size.max(status.total_bytes)
                } else {
                    status.total_bytes
                };

                let _ = progress_tx
                    .send(ProgressUpdate {
                        bytes_downloaded: status.bytes_downloaded,
                        total_bytes,
                        child_path: None,
                        child_filename: status.filename.clone(),
                        child_bytes_downloaded: None,
                        child_total_bytes: None,
                        child_speed_bps: None,
                        child_eta_secs: None,
                    })
                    .await;

                match status.status.as_str() {
                    "pending" | "downloading" => continue,
                    "complete" => {
                        return Ok(status.bytes_downloaded.max(total_bytes));
                    }
                    "cancelled" => {
                        return Err(anyhow!("O navegador integrado do Katfile cancelou este download."));
                    }
                    "error" => {
                        return Err(anyhow!(
                            "{}",
                            status
                                .error
                                .unwrap_or_else(|| "O helper local do Katfile falhou.".to_string())
                        ));
                    }
                    other => {
                        return Err(anyhow!("Status inesperado do helper do Katfile: {other}"));
                    }
                }
            }
        })
    }
}

#[cfg(test)]
#[path = "tests/katfile.rs"]
mod tests;
