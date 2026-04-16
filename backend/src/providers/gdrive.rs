use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;

use crate::models::FileInfo;
use super::{apply_speed_limit, try_parallel_download, Provider, ProgressUpdate};

pub struct GDriveProvider;

impl GDriveProvider {
    pub fn matches(url: &str) -> bool {
        url.contains("drive.google.com")
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

    // Cria um cliente HTTP com User-Agent configurado
    // O Google Drive requer um User-Agent válido, caso contrário retorna 403
    fn http_client() -> Result<reqwest::Client> {
        Ok(reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
            .redirect(reqwest::redirect::Policy::limited(10)) // Segue até 10 redirects
            .build()?)
    }

    fn response_total_bytes(resp: &reqwest::Response, resumed_bytes: u64) -> u64 {
        resp.headers()
            .get("content-range")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split('/').last())
            .and_then(|value| value.parse::<u64>().ok())
            .or_else(|| resp.content_length().map(|len| len + resumed_bytes))
            .unwrap_or(0)
    }

    async fn send_with_resume_fallback(
        client: &reqwest::Client,
        url: &str,
        dest_path: &str,
        existing_bytes: u64,
    ) -> Result<(reqwest::Response, bool)> {
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
    }
}

impl Provider for GDriveProvider {
    fn name(&self) -> &str { "Google Drive" }

    fn get_file_info<'a>(&'a self, url: &'a str)
        -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FileInfo>> + Send + 'a>>
    {
        Box::pin(async move {
            let id = Self::extract_id(url)
                .ok_or_else(|| anyhow!("URL do Google Drive inválida: {url}"))?;

            let client = Self::http_client()?;
            let download_url = Self::download_url(&id);

            // HEAD request = só os headers, sem baixar o body
            // Usado para obter Content-Length e Content-Disposition sem desperdiçar banda
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
                .unwrap_or_else(|| format!("gdrive_{id}"));

            Ok(FileInfo {
                filename,
                size: resp.content_length().unwrap_or(0),
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
        progress_tx: tokio::sync::mpsc::Sender<ProgressUpdate>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u64>> + Send + 'a>>
    {
        Box::pin(async move {
            let id = Self::extract_id(url)
                .ok_or_else(|| anyhow!("URL do Google Drive inválida: {url}"))?;

            let client = Self::http_client()?;
            let download_url = Self::download_url(&id);

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
                Self::send_with_resume_fallback(&client, &download_url, dest_path, existing_bytes).await?;
            let total = if resumed {
                Self::response_total_bytes(&resp, existing_bytes)
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
