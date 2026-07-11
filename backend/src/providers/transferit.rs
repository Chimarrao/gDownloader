use std::collections::HashSet;

use aes::cipher::generic_array::GenericArray;
use anyhow::{anyhow, Context, Result};
use cipher::{KeyIvInit, StreamCipher, StreamCipherSeek};
use futures_util::StreamExt;
use serde_json::Value;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;

use crate::models::{FileChildInfo, FileInfo};

use super::{apply_speed_limit, host_matches, path_segments, ProgressUpdate, Provider, ProviderDefaults};
use super::mega::MegaProvider;

type Aes128Ctr = ctr::Ctr128BE<aes::Aes128>;

const TRANSFERIT_API: &str = "https://bt7.api.mega.co.nz/cs?id=0&v=2";

#[derive(Debug, Clone)]
struct TransferItFile {
    handle: String,
    name: String,
    size: u64,
    key_bytes: Vec<u8>,
    source_url: String,
}

#[derive(Debug, Clone)]
struct TransferItListing {
    handle: String,
    title: String,
    files: Vec<TransferItFile>,
}

#[derive(Debug, Clone)]
struct TransferItDownloadMeta {
    url: String,
    size: u64,
}

pub struct TransferItProvider;

impl TransferItProvider {
    pub fn matches(url: &str) -> bool {
        Self::extract_transfer_handle(url).is_some()
    }

    fn extract_transfer_handle(url: &str) -> Option<String> {
        if !host_matches(url, &["transfer.it", "www.transfer.it"]) {
            return None;
        }

        let segments = path_segments(url);
        match segments.as_slice() {
            [prefix, handle, ..] if prefix == "t" && handle.len() >= 8 => Some(handle.to_string()),
            _ => None,
        }
    }

    fn decode_transfer_text(value: &str) -> Option<String> {
        let decoded = MegaProvider::mega_base64_decode(value);
        String::from_utf8(decoded).ok().map(|text| text.trim().to_string()).filter(|text| !text.is_empty())
    }

    async fn api_request(
        client: &reqwest::Client,
        query: Option<&str>,
        payload: Value,
    ) -> Result<Value> {
        let url = match query {
            Some(query) => format!("{TRANSFERIT_API}&{query}"),
            None => TRANSFERIT_API.to_string(),
        };
        let body = serde_json::json!([payload]);
        let response: Value = client
            .post(&url)
            .json(&body)
            .send()
            .await
            .context("Falha ao consultar a API pública do Transfer.it")?
            .json()
            .await
            .context("Falha ao parsear resposta pública do Transfer.it")?;

        let result = &response[0];
        if let Some(code) = result.as_i64() {
            return Err(anyhow!("Transfer.it retornou erro {code}"));
        }

        Ok(result.clone())
    }

    async fn get_transfer_info(client: &reqwest::Client, handle: &str) -> Result<(String, u64, u64)> {
        let result = Self::api_request(client, None, serde_json::json!({ "a": "xi", "xh": handle })).await?;
        let title = result["t"]
            .as_str()
            .and_then(Self::decode_transfer_text)
            .unwrap_or_else(|| format!("transferit_{handle}"));
        let size = result["size"]
            .as_array()
            .and_then(|values| values.first())
            .and_then(|value| value.as_u64())
            .unwrap_or(0);
        let file_count = result["size"]
            .as_array()
            .and_then(|values| values.get(1))
            .and_then(|value| value.as_u64())
            .unwrap_or(0);

        Ok((title, size, file_count))
    }

    async fn get_listing(client: &reqwest::Client, handle: &str) -> Result<TransferItListing> {
        let (title, _total_size, _file_count) = Self::get_transfer_info(client, handle).await?;
        let result = Self::api_request(
            client,
            Some(&format!("x={handle}")),
            serde_json::json!({ "a": "f", "c": 1, "r": 1 }),
        )
        .await?;

        let nodes = result["f"]
            .as_array()
            .ok_or_else(|| anyhow!("Transfer.it não retornou a lista de arquivos"))?;

        let mut files = Vec::new();
        for node in nodes {
            if node["t"].as_u64() != Some(0) {
                continue;
            }

            let Some(file_handle) = node["h"].as_str() else {
                continue;
            };
            let Some(key_field) = node["k"].as_str() else {
                continue;
            };
            let key_part = key_field.rsplit(':').next().unwrap_or(key_field);
            let key_bytes = MegaProvider::mega_base64_decode(key_part);
            let Some(attr_key) = MegaProvider::derive_attr_key_from_node_key(&key_bytes, true) else {
                continue;
            };
            let name = node["a"]
                .as_str()
                .and_then(|attr| MegaProvider::decrypt_attributes_name(attr, &attr_key))
                .unwrap_or_else(|| title.clone());
            let safe_name = <Self as ProviderDefaults>::safe_filename(&name, &format!("transferit_{file_handle}"));
            let source_url = format!("https://transfer.it/t/{handle}#n={file_handle}");

            files.push(TransferItFile {
                handle: file_handle.to_string(),
                name: safe_name,
                size: node["s"].as_u64().unwrap_or(0),
                key_bytes,
                source_url,
            });
        }

        if files.is_empty() {
            return Err(anyhow!("Transfer.it não retornou arquivos acessíveis"));
        }

        Ok(TransferItListing {
            handle: handle.to_string(),
            title: <Self as ProviderDefaults>::safe_filename(&title, &format!("transferit_{handle}")),
            files,
        })
    }

    async fn get_download_meta(
        client: &reqwest::Client,
        transfer_handle: &str,
        file_handle: &str,
    ) -> Result<TransferItDownloadMeta> {
        let result = Self::api_request(
            client,
            Some(&format!("x={transfer_handle}")),
            serde_json::json!({ "a": "g", "n": file_handle, "pt": 1, "g": 1, "ssl": 1 }),
        )
        .await?;

        let url = result["g"]
            .as_str()
            .ok_or_else(|| anyhow!("Transfer.it não retornou URL de download"))?
            .to_string();
        let size = result["s"].as_u64().unwrap_or(0);

        Ok(TransferItDownloadMeta { url, size })
    }

    async fn download_file(
        client: &reqwest::Client,
        transfer_handle: &str,
        file: &TransferItFile,
        dest_path: &str,
        base_downloaded: u64,
        total_bytes: u64,
        speed_limit_bps: super::SpeedLimitBps,
        started_at: tokio::time::Instant,
        session_downloaded: &mut u64,
        progress_tx: tokio::sync::mpsc::Sender<ProgressUpdate>,
    ) -> Result<u64> {
        let meta = Self::get_download_meta(client, transfer_handle, &file.handle).await?;
        let expected_size = if file.size > 0 { file.size } else { meta.size };
        let (aes_key, iv) = MegaProvider::derive_key_and_iv(&file.key_bytes);
        let existing_bytes = tokio::fs::metadata(dest_path)
            .await
            .ok()
            .filter(|meta| meta.is_file())
            .map(|meta| meta.len())
            .unwrap_or(0);

        let mut request = client.get(&meta.url);
        if existing_bytes > 0 {
            request = request.header("Range", format!("bytes={existing_bytes}-"));
        }
        let response = request.send().await?.error_for_status()?;
        let resumed = existing_bytes > 0 && response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
        let mut output = if resumed {
            OpenOptions::new().create(true).append(true).open(dest_path).await?
        } else {
            tokio::fs::File::create(dest_path).await?
        };
        let mut downloaded = if resumed { existing_bytes } else { 0 };
        let mut cipher = Aes128Ctr::new(
            GenericArray::from_slice(&aes_key),
            GenericArray::from_slice(&iv),
        );
        if resumed {
            cipher.seek(existing_bytes);
        }
        let mut stream = response.bytes_stream();

        while let Some(chunk) = stream.next().await {
            let mut chunk = chunk?.to_vec();
            for part in chunk.chunks_mut(64 * 1024) {
                cipher.apply_keystream(part);
                output.write_all(part).await?;
                let len = part.len() as u64;
                downloaded += len;
                *session_downloaded += len;

                let _ = progress_tx
                    .send(ProgressUpdate {
                        bytes_downloaded: base_downloaded + downloaded,
                        total_bytes,
                        child_path: None,
                        child_filename: Some(file.name.clone()),
                        child_bytes_downloaded: Some(downloaded),
                        child_total_bytes: Some(expected_size),
                        child_speed_bps: None,
                        child_eta_secs: None,
                    })
                    .await;

                apply_speed_limit(started_at, *session_downloaded, &speed_limit_bps).await;
            }
        }

        output.flush().await?;
        Ok(downloaded)
    }
}

impl ProviderDefaults for TransferItProvider {}

impl Provider for TransferItProvider {
    fn name(&self) -> &str { "Transfer.it" }

    fn get_file_info<'a>(
        &'a self,
        url: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FileInfo>> + Send + 'a>> {
        Box::pin(async move {
            let handle = Self::extract_transfer_handle(url)
                .ok_or_else(|| anyhow!("URL do Transfer.it inválida: {url}"))?;
            let client = <Self as ProviderDefaults>::http_client()?;
            let listing = Self::get_listing(&client, &handle).await?;

            if listing.files.len() == 1 {
                let file = &listing.files[0];
                return Ok(FileInfo {
                    filename: file.name.clone(),
                    size: file.size,
                    mime_type: None,
                    is_folder: false,
                    children: None,
                    ..Default::default()
                });
            }

            let children = listing
                .files
                .iter()
                .map(|file| FileChildInfo {
                    filename: file.name.clone(),
                    size: file.size,
                    mime_type: None,
                    is_folder: false,
                    path: Some(file.name.clone()),
                    source_url: Some(file.source_url.clone()),
                    bytes_downloaded: None,
                    speed_bps: None,
                    eta_secs: None,
                    status: None,
                })
                .collect::<Vec<_>>();

            Ok(FileInfo {
                filename: listing.title,
                size: children.iter().map(|child| child.size).sum(),
                mime_type: None,
                is_folder: true,
                children: Some(children),
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
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u64>> + Send + 'a>> {
        Box::pin(async move {
            let handle = Self::extract_transfer_handle(url)
                .ok_or_else(|| anyhow!("URL do Transfer.it inválida: {url}"))?;
            let client = <Self as ProviderDefaults>::http_client()?;
            let listing = Self::get_listing(&client, &handle).await?;
            let started_at = tokio::time::Instant::now();
            let mut session_downloaded = 0u64;

            if listing.files.len() == 1 {
                return Self::download_file(
                    &client,
                    &listing.handle,
                    &listing.files[0],
                    dest_path,
                    0,
                    listing.files[0].size,
                    speed_limit_bps,
                    started_at,
                    &mut session_downloaded,
                    progress_tx,
                )
                .await;
            }

            let mut files = listing.files;
            if let Some(selected) = selected_children.filter(|items| !items.is_empty()) {
                let selected_set = selected.into_iter().collect::<HashSet<_>>();
                files.retain(|file| selected_set.contains(&file.source_url));
            }
            if files.is_empty() {
                return Err(anyhow!("Transfer.it sem arquivos selecionados"));
            }

            tokio::fs::create_dir_all(dest_path).await?;
            let total_bytes = files.iter().map(|file| file.size).sum::<u64>();
            let mut downloaded_total = 0u64;

            for file in &files {
                let output_path = format!("{}/{}", dest_path.trim_end_matches('/'), file.name);
                let downloaded = Self::download_file(
                    &client,
                    &listing.handle,
                    file,
                    &output_path,
                    downloaded_total,
                    total_bytes,
                    speed_limit_bps.clone(),
                    started_at,
                    &mut session_downloaded,
                    progress_tx.clone(),
                )
                .await?;
                downloaded_total += downloaded;
            }

            Ok(downloaded_total)
        })
    }
}

#[cfg(test)]
#[path = "tests/transferit.rs"]
mod tests;
