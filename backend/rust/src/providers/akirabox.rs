use anyhow::{anyhow, Result};
use serde::Deserialize;
use serde_json::json;
use tokio::time::{sleep, Duration};

use crate::models::FileInfo;

use super::{host_matches, path_segments, ProgressUpdate, Provider, ProviderDefaults};

fn electron_proxy_port() -> Option<u16> {
    std::env::var("AKIRABOX_PROXY_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .filter(|&value| value > 0)
}

fn helper_proxy_token() -> Option<String> {
    std::env::var("GDOWNLOADER_HELPER_TOKEN")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

async fn proxy_action(payload: serde_json::Value) -> Result<serde_json::Value> {
    let port = electron_proxy_port().ok_or_else(|| anyhow!("Helper local do AkiraBox não disponível"))?;
    let token = helper_proxy_token()
        .ok_or_else(|| anyhow!("Token do helper local do AkiraBox não disponível"))?;
    let proxy_url = format!("http://127.0.0.1:{port}/");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(180))
        .build()?;
    let response = client
        .post(&proxy_url)
        .header("X-GDownloader-Token", token)
        .json(&payload)
        .send()
        .await?;
    Ok(response.json::<serde_json::Value>().await?)
}

#[derive(Debug, Deserialize)]
struct HelperJobStatus {
    status: String,
    #[serde(default, rename = "bytesDownloaded")]
    bytes_downloaded: u64,
    #[serde(default, rename = "totalBytes")]
    total_bytes: u64,
    #[serde(default, rename = "speedBps")]
    _speed_bps: u64,
    #[serde(default, rename = "etaSecs")]
    _eta_secs: u64,
    filename: Option<String>,
    error: Option<String>,
}

pub struct AkiraboxProvider;

impl AkiraboxProvider {
    pub fn matches(url: &str) -> bool {
        host_matches(url, &["akirabox.to", "www.akirabox.to"])
            && matches!(path_segments(url).as_slice(), [_code, second, ..] if second == "file")
    }

    async fn helper_file_info(url: &str) -> Result<FileInfo> {
        let json = proxy_action(json!({
            "action": "akirabox_file_info",
            "url": url,
        })).await?;

        Ok(serde_json::from_value::<FileInfo>(json)?)
    }

    async fn helper_start_download(source_url: &str, dest_path: &str) -> Result<String> {
        let json = proxy_action(json!({
            "action": "akirabox_download_file",
            "url": source_url,
            "destPath": dest_path,
        })).await?;

        json["jobId"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| anyhow!("Helper local do AkiraBox não retornou um jobId"))
    }

    async fn helper_job_status(job_id: &str) -> Result<HelperJobStatus> {
        let json = proxy_action(json!({
            "action": "akirabox_job_status",
            "jobId": job_id,
        })).await?;

        Ok(serde_json::from_value::<HelperJobStatus>(json)?)
    }
}

impl ProviderDefaults for AkiraboxProvider {}

impl Provider for AkiraboxProvider {
    fn name(&self) -> &str { "AkiraBox" }

    fn get_file_info<'a>(
        &'a self,
        url: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FileInfo>> + Send + 'a>> {
        Box::pin(async move {
            let mut info = Self::helper_file_info(url).await?;
            info.filename = <Self as ProviderDefaults>::safe_filename(&info.filename, "arquivo_akirabox");
            Ok(info)
        })
    }

    fn download<'a>(
        &'a self,
        url: &'a str,
        dest_path: &'a str,
        _speed_limit_bps: super::SpeedLimitBps,
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
                        return Err(anyhow!("O navegador do AkiraBox cancelou este download."));
                    }
                    "error" => {
                        return Err(anyhow!(
                            "{}",
                            status
                                .error
                                .unwrap_or_else(|| "O helper local do AkiraBox falhou.".to_string())
                        ));
                    }
                    other => {
                        return Err(anyhow!("Status inesperado do helper do AkiraBox: {other}"));
                    }
                }
            }
        })
    }
}

#[cfg(test)]
#[path = "tests/akirabox.rs"]
mod tests;
