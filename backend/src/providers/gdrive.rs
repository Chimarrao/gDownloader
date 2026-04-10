use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;

use crate::models::FileInfo;
use super::{Provider, ProgressUpdate};

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
            })
        })
    }

    fn download<'a>(
        &'a self,
        url: &'a str,
        dest_path: &'a str,
        progress_tx: tokio::sync::mpsc::Sender<ProgressUpdate>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u64>> + Send + 'a>>
    {
        Box::pin(async move {
            let id = Self::extract_id(url)
                .ok_or_else(|| anyhow!("URL do Google Drive inválida: {url}"))?;

            let client = Self::http_client()?;
            let download_url = Self::download_url(&id);

            let resp = client
                .get(&download_url)
                .send()
                .await?
                .error_for_status()?;

            let total = resp.content_length().unwrap_or(0);
            let mut file = tokio::fs::File::create(dest_path).await?;
            let mut stream = resp.bytes_stream();
            let mut downloaded: u64 = 0;

            while let Some(chunk) = stream.next().await {
                let chunk = chunk?;
                file.write_all(&chunk).await?;
                downloaded += chunk.len() as u64;
                let _ = progress_tx
                    .send(ProgressUpdate { bytes_downloaded: downloaded, total_bytes: total })
                    .await;
            }

            file.flush().await?;
            Ok(downloaded)
        })
    }
}

// --- Testes unitários ---
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_id_file_url() {
        let id = GDriveProvider::extract_id(
            "https://drive.google.com/file/d/1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgVE2upms/view",
        );
        assert_eq!(id, Some("1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgVE2upms".to_string()));
    }

    #[test]
    fn test_extract_id_open_url() {
        let id = GDriveProvider::extract_id(
            "https://drive.google.com/open?id=1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgVE2upms",
        );
        assert_eq!(id, Some("1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgVE2upms".to_string()));
    }

    #[test]
    fn test_extract_id_uc_url() {
        let id = GDriveProvider::extract_id(
            "https://drive.google.com/uc?id=1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgVE2upms&export=download",
        );
        assert_eq!(id, Some("1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgVE2upms".to_string()));
    }

    #[test]
    fn test_download_url_format() {
        let url = GDriveProvider::download_url("ABC123");
        assert!(url.contains("ABC123"));
        assert!(url.contains("confirm=t"));
        assert!(url.contains("export=download"));
    }
}
