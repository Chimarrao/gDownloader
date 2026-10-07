// pCloud — proxy fino pro motor de download em Go (internal/pcloudengine),
// no mesmo padrão do OneDrive (ver onedrive_business.rs): o Rust só orquestra
// (fila, pause/retry, UI), o trabalho de rede de verdade (ler a árvore embutida
// na página do link público, resolver URL de download por fileid, download com
// resume) é todo feito no Go, via loopback numa porta fixa compartilhada.
use anyhow::{anyhow, Result};
use serde::Deserialize;

use crate::models::{FileChildInfo, FileInfo};
use super::{ProgressUpdate, Provider, ProviderDefaults};

const GO_ENGINE_BASE: &str = "http://127.0.0.1:48653";

pub struct PCloudProvider;

impl PCloudProvider {
    pub fn matches(url: &str) -> bool {
        let Some(parsed) = super::parse_url(url) else {
            return false;
        };
        let Some(host) = parsed.host_str() else {
            return false;
        };
        let host = host.to_ascii_lowercase();
        (host.contains("pcloud.link") || host.contains("pcloud.com"))
            && parsed.path().contains("/publink/show")
            && parsed.query_pairs().any(|(k, _)| k == "code")
    }
}

#[derive(Debug, Deserialize)]
struct GoChildInfo {
    filename: String,
    size: u64,
    #[serde(rename = "isFolder")]
    is_folder: bool,
    path: String,
    #[serde(rename = "sourceUrl")]
    source_url: String,
}

#[derive(Debug, Deserialize)]
struct GoFileInfoResponse {
    name: String,
    size: u64,
    #[serde(rename = "isFolder")]
    is_folder: bool,
    children: Vec<GoChildInfo>,
}

#[derive(Debug, Deserialize)]
struct GoStartJobResponse {
    #[serde(rename = "jobId")]
    job_id: String,
}

#[derive(Debug, Deserialize)]
struct GoJobStatus {
    status: String,
    #[serde(rename = "bytesDownloaded")]
    bytes_downloaded: i64,
    #[serde(rename = "totalBytes")]
    total_bytes: i64,
    error: String,
    #[serde(rename = "currentPath")]
    current_path: String,
    #[serde(rename = "currentFilename")]
    current_filename: String,
    #[serde(rename = "currentBytes")]
    current_bytes: i64,
    #[serde(rename = "currentTotal")]
    current_total: i64,
}

impl ProviderDefaults for PCloudProvider {}

impl Provider for PCloudProvider {
    fn name(&self) -> &str {
        "pCloud"
    }

    fn get_file_info<'a>(
        &'a self,
        url: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FileInfo>> + Send + 'a>> {
        Box::pin(async move {
            let client = <Self as ProviderDefaults>::http_client()?;
            let endpoint = format!(
                "{GO_ENGINE_BASE}/pcloud/file-info?url={}",
                urlencoding::encode(url)
            );
            let resp = client.get(&endpoint).send().await.map_err(|e| {
                anyhow!("Falha ao contatar o motor de download pCloud (Go): {e}")
            })?;
            if !resp.status().is_success() {
                let body = resp.text().await.unwrap_or_default();
                return Err(anyhow!("Não foi possível acessar a pasta do pCloud: {body}"));
            }
            let parsed: GoFileInfoResponse = resp
                .json()
                .await
                .map_err(|e| anyhow!("Resposta inválida do motor pCloud (Go): {e}"))?;

            let children = parsed
                .children
                .into_iter()
                .map(|c| FileChildInfo {
                    filename: c.filename,
                    size: c.size,
                    mime_type: None,
                    is_folder: c.is_folder,
                    path: Some(c.path),
                    source_url: Some(c.source_url),
                    bytes_downloaded: None,
                    speed_bps: None,
                    eta_secs: None,
                    status: None,
                })
                .collect::<Vec<_>>();

            Ok(FileInfo {
                filename: <Self as ProviderDefaults>::safe_filename(&parsed.name, "pCloud"),
                size: parsed.size,
                is_folder: parsed.is_folder,
                children: Some(children),
                ..Default::default()
            })
        })
    }

    fn download<'a>(
        &'a self,
        url: &'a str,
        dest_path: &'a str,
        _speed_limit_bps: super::SpeedLimitBps,
        _parallel_parts: usize,
        selected_children: Option<Vec<String>>,
        progress_tx: tokio::sync::mpsc::Sender<ProgressUpdate>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u64>> + Send + 'a>> {
        Box::pin(async move {
            let client = <Self as ProviderDefaults>::http_client()?;

            #[derive(serde::Serialize)]
            struct StartReq<'b> {
                url: &'b str,
                #[serde(rename = "destPath")]
                dest_path: &'b str,
                #[serde(rename = "selectedChildren")]
                selected_children: Vec<String>,
            }

            let body = StartReq {
                url,
                dest_path,
                selected_children: selected_children.unwrap_or_default(),
            };

            let start_resp = client
                .post(format!("{GO_ENGINE_BASE}/pcloud/jobs"))
                .json(&body)
                .send()
                .await
                .map_err(|e| anyhow!("Falha ao iniciar download no motor pCloud (Go): {e}"))?;

            if !start_resp.status().is_success() {
                let status = start_resp.status();
                let text = start_resp.text().await.unwrap_or_default();
                return Err(anyhow!("Motor pCloud (Go) recusou iniciar o download (HTTP {status}): {text}"));
            }

            let started: GoStartJobResponse = start_resp
                .json()
                .await
                .map_err(|e| anyhow!("Resposta inválida ao iniciar download no motor pCloud (Go): {e}"))?;

            let status_url = format!("{GO_ENGINE_BASE}/pcloud/jobs/{}", started.job_id);

            let mut last_child_path: Option<String> = None;
            let mut last_child_bytes: u64 = 0;
            let mut last_child_time = std::time::Instant::now();
            let mut last_child_speed: u64 = 0;

            loop {
                tokio::time::sleep(std::time::Duration::from_millis(700)).await;

                let poll_resp = client
                    .get(&status_url)
                    .send()
                    .await
                    .map_err(|e| anyhow!("Falha ao consultar progresso no motor pCloud (Go): {e}"))?;

                if !poll_resp.status().is_success() {
                    return Err(anyhow!(
                        "Motor pCloud (Go) perdeu o job de download (HTTP {})",
                        poll_resp.status()
                    ));
                }

                let job: GoJobStatus = poll_resp
                    .json()
                    .await
                    .map_err(|e| anyhow!("Resposta inválida ao consultar progresso no motor pCloud (Go): {e}"))?;

                let bytes_downloaded = job.bytes_downloaded.max(0) as u64;
                let total_bytes = job.total_bytes.max(0) as u64;

                let current_path = (!job.current_path.is_empty()).then(|| job.current_path.clone());
                let current_bytes = job.current_bytes.max(0) as u64;
                let current_total = job.current_total.max(0) as u64;

                if last_child_path.as_deref() != current_path.as_deref() {
                    last_child_path = current_path.clone();
                    last_child_bytes = current_bytes;
                    last_child_time = std::time::Instant::now();
                    last_child_speed = 0;
                } else {
                    let elapsed = last_child_time.elapsed().as_secs_f64();
                    if elapsed >= 0.5 {
                        let delta = current_bytes.saturating_sub(last_child_bytes);
                        last_child_speed = (delta as f64 / elapsed) as u64;
                        last_child_bytes = current_bytes;
                        last_child_time = std::time::Instant::now();
                    }
                }

                let child_eta = if last_child_speed > 0 && current_total > current_bytes {
                    (current_total - current_bytes) / last_child_speed
                } else {
                    0
                };

                let _ = progress_tx
                    .send(ProgressUpdate {
                        bytes_downloaded,
                        total_bytes,
                        child_path: current_path.clone(),
                        child_filename: (!job.current_filename.is_empty())
                            .then(|| job.current_filename.clone()),
                        child_bytes_downloaded: current_path.as_ref().map(|_| current_bytes),
                        child_total_bytes: current_path.as_ref().map(|_| current_total),
                        child_speed_bps: current_path.as_ref().map(|_| last_child_speed),
                        child_eta_secs: current_path.as_ref().map(|_| child_eta),
                    })
                    .await;

                match job.status.as_str() {
                    "done" => return Ok(bytes_downloaded),
                    "error" => return Err(anyhow!("{}", job.error)),
                    _ => continue,
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::PCloudProvider;

    #[test]
    fn matches_public_share_link() {
        assert!(PCloudProvider::matches(
            "https://u.pcloud.link/publink/show?code=kZLp1sVZlFLQy9jilHkzkJvU2G4j58M99SPk"
        ));
    }

    #[test]
    fn does_not_match_without_code() {
        assert!(!PCloudProvider::matches("https://u.pcloud.link/publink/show"));
    }

    #[test]
    fn does_not_match_unrelated_hosts() {
        assert!(!PCloudProvider::matches("https://mega.nz/file/abc"));
        assert!(!PCloudProvider::matches("https://drive.google.com/file/d/abc"));
    }
}
