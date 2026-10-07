// OneDrive-for-Business / SharePoint "my site" — proxy fino pro motor de
// download que roda DE VERDADE no sidecar Go (internal/onedriveengine), não
// aqui. Início da migração gradual de provedores pro motor Go pedida pelo
// usuário: aqui o Rust só orquestra (fila, pause/retry, UI) e repassa a
// chamada de rede pro Go via loopback numa porta fixa — o trabalho real
// (bootstrap do cookie anônimo, listagem recursiva via API REST do
// SharePoint, download com resume) é todo feito lá.
//
// Por quê um provider separado do sharepoint.rs genérico: aquele casa
// QUALQUER host "*.sharepoint.com" e tenta append `?download=1` na URL crua,
// o que não funciona pro formato de link de compartilhamento
// (.../:f:/g/personal/<user>/<token>) — confirmado manualmente via curl que a
// URL de navegação crua (.../_layouts/15/onedrive.aspx?id=...) sempre
// redireciona pra login SEM o cookie anônimo que só visitar o link de
// compartilhamento concede. Por isso este provider precisa ser checado ANTES
// do genérico em detect_provider().
use anyhow::{anyhow, Result};
use serde::Deserialize;

use crate::models::{FileChildInfo, FileInfo};
use super::{ProgressUpdate, Provider, ProviderDefaults};

const GO_ENGINE_BASE: &str = "http://127.0.0.1:48653";

pub struct OneDriveBusinessProvider;

impl OneDriveBusinessProvider {
    pub fn matches(url: &str) -> bool {
        let Some(parsed) = super::parse_url(url) else {
            return false;
        };
        let Some(host) = parsed.host_str() else {
            return false;
        };
        host.to_ascii_lowercase().contains("sharepoint.com") && parsed.path().contains("/:f:/")
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

impl ProviderDefaults for OneDriveBusinessProvider {}

impl Provider for OneDriveBusinessProvider {
    fn name(&self) -> &str {
        // Mesmo nome do provider genérico — reaproveita o mapeamento de
        // ícone/cor já existente no frontend (provider_id_from_name
        // "OneDrive" -> "onedrive"), sem precisar de nenhuma mudança de UI.
        "OneDrive"
    }

    fn get_file_info<'a>(
        &'a self,
        url: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FileInfo>> + Send + 'a>> {
        Box::pin(async move {
            let client = <Self as ProviderDefaults>::http_client()?;
            let endpoint = format!(
                "{GO_ENGINE_BASE}/onedrive/file-info?url={}",
                urlencoding::encode(url)
            );
            let resp = client.get(&endpoint).send().await.map_err(|e| {
                anyhow!("Falha ao contatar o motor de download OneDrive (Go): {e}")
            })?;
            if !resp.status().is_success() {
                let body = resp.text().await.unwrap_or_default();
                return Err(anyhow!("Não foi possível acessar a pasta do OneDrive: {body}"));
            }
            let parsed: GoFileInfoResponse = resp
                .json()
                .await
                .map_err(|e| anyhow!("Resposta inválida do motor OneDrive (Go): {e}"))?;

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
                filename: <Self as ProviderDefaults>::safe_filename(&parsed.name, "OneDrive"),
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
                .post(format!("{GO_ENGINE_BASE}/onedrive/jobs"))
                .json(&body)
                .send()
                .await
                .map_err(|e| anyhow!("Falha ao iniciar download no motor OneDrive (Go): {e}"))?;

            if !start_resp.status().is_success() {
                let status = start_resp.status();
                let text = start_resp.text().await.unwrap_or_default();
                return Err(anyhow!("Motor OneDrive (Go) recusou iniciar o download (HTTP {status}): {text}"));
            }

            let started: GoStartJobResponse = start_resp
                .json()
                .await
                .map_err(|e| anyhow!("Resposta inválida ao iniciar download no motor OneDrive (Go): {e}"))?;

            let status_url = format!("{GO_ENGINE_BASE}/onedrive/jobs/{}", started.job_id);

            // A tela de downloads calcula velocidade/ETA/percentual de pastas
            // SOMANDO o progresso por arquivo filho (child_bytes_downloaded /
            // child_speed_bps), não o agregado do job — sem isso a velocidade
            // ficava travada em "0 KB/s" mesmo com bytes reais sendo escritos.
            // O Go não calcula velocidade (só bytes acumulados), então o
            // cálculo de delta/tempo é feito aqui, reiniciando o relógio toda
            // vez que o arquivo "atual" do job muda.
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
                    .map_err(|e| anyhow!("Falha ao consultar progresso no motor OneDrive (Go): {e}"))?;

                if !poll_resp.status().is_success() {
                    return Err(anyhow!(
                        "Motor OneDrive (Go) perdeu o job de download (HTTP {})",
                        poll_resp.status()
                    ));
                }

                let job: GoJobStatus = poll_resp
                    .json()
                    .await
                    .map_err(|e| anyhow!("Resposta inválida ao consultar progresso no motor OneDrive (Go): {e}"))?;

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
    use super::OneDriveBusinessProvider;

    #[test]
    fn matches_business_share_link() {
        assert!(OneDriveBusinessProvider::matches(
            "https://fitxdeutschlandgmbh-my.sharepoint.com/:f:/g/personal/animes_fitxdeutschlandgmbh_onmicrosoft_com/Er-Hpv679BBJrOW8L-axDwIBvuziSoATLOzvXsTKENwnmw?e=NkhLwL"
        ));
    }

    #[test]
    fn does_not_match_raw_navigation_url() {
        // A URL de navegação crua (sem /:f:/) exige login — não é um ponto de
        // entrada válido, então não deve ser reivindicada por este provider.
        assert!(!OneDriveBusinessProvider::matches(
            "https://fitxdeutschlandgmbh-my.sharepoint.com/personal/animes_fitxdeutschlandgmbh_onmicrosoft_com/_layouts/15/onedrive.aspx?id=%2Fpersonal%2Fanimes_fitxdeutschlandgmbh_onmicrosoft_com%2FDocuments%2FTwin%20Peaks"
        ));
    }

    #[test]
    fn does_not_match_unrelated_hosts() {
        assert!(!OneDriveBusinessProvider::matches("https://mega.nz/file/abc"));
        assert!(!OneDriveBusinessProvider::matches("https://onedrive.live.com/?id=abc"));
    }
}
