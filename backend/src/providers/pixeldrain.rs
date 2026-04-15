use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;

use crate::models::FileInfo;
use super::{Provider, ProgressUpdate};

pub struct PixelDrainProvider;

impl PixelDrainProvider {
    pub fn matches(url: &str) -> bool {
        url.contains("pixeldrain.com")
    }

    // Extrai o ID do arquivo da URL do PixelDrain
    // Exemplo: https://pixeldrain.com/u/AbCdEfGh → "AbCdEfGh"
    pub fn extract_id(url: &str) -> Option<String> {
        // Divide a URL em segmentos pelo "/"
        // Exemplo: ["https:", "", "pixeldrain.com", "u", "AbCdEfGh"]
        let parts: Vec<&str> = url.split('/').collect();

        // Procura o índice do segmento "u" no path
        // position() é como findIndex() no JavaScript
        let u_pos = parts.iter().position(|&s| s == "u")?;

        // O ID é o segmento logo depois de "u"
        // trim_end_matches('/') remove trailing slashes
        let id = parts.get(u_pos + 1)?.trim_end_matches('/');

        if id.is_empty() || !url.contains("pixeldrain.com") {
            return None;
        }

        Some(id.to_string())
    }
}

impl Provider for PixelDrainProvider {
    fn name(&self) -> &str { "PixelDrain" }

    fn get_file_info<'a>(&'a self, url: &'a str)
        -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FileInfo>> + Send + 'a>>
    {
        Box::pin(async move {
            let id = Self::extract_id(url)
                .ok_or_else(|| anyhow!("URL do PixelDrain inválida: {url}"))?;

            // API de informações do PixelDrain — retorna JSON com nome, tamanho, etc.
            let info_url = format!("https://pixeldrain.com/api/file/{id}/info");

            let client = reqwest::Client::new();
            let resp = client
                .get(&info_url)
                .send()
                .await?
                .error_for_status()?; // Retorna erro se HTTP status != 2xx

            // Parse do JSON de resposta
            let json: serde_json::Value = resp.json().await?;

            Ok(FileInfo {
                filename: json["name"]
                    .as_str()
                    .unwrap_or("arquivo_pixeldrain")
                    .to_string(),
                size: json["size"].as_u64().unwrap_or(0),
                mime_type: json["mime_type"].as_str().map(String::from),
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
                .ok_or_else(|| anyhow!("URL do PixelDrain inválida: {url}"))?;

            // URL de download direto do PixelDrain
            let download_url = format!("https://pixeldrain.com/api/file/{id}");

            let client = reqwest::Client::new();
            let resp = client
                .get(&download_url)
                .send()
                .await?
                .error_for_status()?;

            // content_length() retorna o tamanho total em bytes, se o servidor informar
            let total = resp.content_length().unwrap_or(0);

            // Cria (ou sobrescreve) o arquivo no disco
            // tokio::fs é a versão assíncrona de std::fs — não bloqueia o event loop
            let mut file = tokio::fs::File::create(dest_path).await?;

            // bytes_stream() converte a resposta em um stream de chunks de bytes
            // Equivalente a response.body no fetch API do JS
            let mut stream = resp.bytes_stream();
            let mut downloaded: u64 = 0;

            // Processa cada chunk conforme chega — sem carregar tudo na memória
            while let Some(chunk) = stream.next().await {
                let chunk = chunk?;
                file.write_all(&chunk).await?;
                downloaded += chunk.len() as u64;

                // Envia atualização de progresso — a fila repassa ao WebSocket
                // Se o receptor foi dropado (download cancelado), ignora o erro
                let _ = progress_tx.send(ProgressUpdate {
                    bytes_downloaded: downloaded,
                    total_bytes: total,
                }).await;
            }

            // Garante que todos os bytes foram escritos no disco antes de retornar
            file.flush().await?;
            Ok(downloaded)
        })
    }
}

