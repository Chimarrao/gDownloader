use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use serde_json::Value;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;

use crate::models::{FileChildInfo, FileInfo};
use super::{apply_speed_limit, try_parallel_download, Provider, ProgressUpdate};

pub struct PixelDrainProvider;

enum PixelDrainTarget {
    File { id: String },
    List { id: String, selected_index: usize },
}

impl PixelDrainProvider {
    pub fn matches(url: &str) -> bool {
        url.contains("pixeldrain.com")
    }

    fn parse_target(url: &str) -> Option<PixelDrainTarget> {
        if !url.contains("pixeldrain.com") {
            return None;
        }

        let parts: Vec<&str> = url.split('/').collect();

        if let Some(pos) = parts.iter().position(|&s| s == "u") {
            let id = parts.get(pos + 1)?.split('?').next()?.split('#').next()?.trim();
            if !id.is_empty() {
                return Some(PixelDrainTarget::File { id: id.to_string() });
            }
        }

        if let Some(pos) = parts.iter().position(|&s| s == "l") {
            let id = parts.get(pos + 1)?.split('?').next()?.split('#').next()?.trim();
            if !id.is_empty() {
                return Some(PixelDrainTarget::List {
                    id: id.to_string(),
                    selected_index: Self::extract_selected_index(url),
                });
            }
        }

        None
    }

    fn extract_selected_index(url: &str) -> usize {
        let Some(fragment) = url.split('#').nth(1) else {
            return 0;
        };

        fragment
            .split('&')
            .find_map(|part| part.strip_prefix("item="))
            .and_then(|raw| raw.parse::<usize>().ok())
            .unwrap_or(0)
    }

    fn safe_filename(name: &str, fallback: &str) -> String {
        let trimmed = name.trim();
        let candidate = if trimmed.is_empty() { fallback } else { trimmed };
        candidate
            .chars()
            .map(|ch| match ch {
                '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
                _ => ch,
            })
            .collect()
    }

    async fn fetch_file_info(client: &reqwest::Client, id: &str) -> Result<Value> {
        let info_url = format!("https://pixeldrain.com/api/file/{id}/info");
        Ok(client.get(&info_url).send().await?.error_for_status()?.json().await?)
    }

    async fn fetch_list_info(client: &reqwest::Client, id: &str) -> Result<Value> {
        let info_url = format!("https://pixeldrain.com/api/list/{id}");
        Ok(client.get(&info_url).send().await?.error_for_status()?.json().await?)
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

impl Provider for PixelDrainProvider {
    fn name(&self) -> &str { "PixelDrain" }

    fn get_file_info<'a>(&'a self, url: &'a str)
        -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FileInfo>> + Send + 'a>>
    {
        Box::pin(async move {
            let target = Self::parse_target(url)
                .ok_or_else(|| anyhow!("URL do PixelDrain inválida: {url}"))?;

            let client = reqwest::Client::new();

            match target {
                PixelDrainTarget::File { id } => {
                    let json = Self::fetch_file_info(&client, &id).await?;
                    Ok(FileInfo {
                        filename: json["name"]
                            .as_str()
                            .unwrap_or("arquivo_pixeldrain")
                            .to_string(),
                        size: json["size"].as_u64().unwrap_or(0),
                        mime_type: json["mime_type"].as_str().map(String::from),
                        is_folder: false,
                        children: None,
                    })
                }
                PixelDrainTarget::List { id, .. } => {
                    let json = Self::fetch_list_info(&client, &id).await?;
                    let files = json["files"].as_array().cloned().unwrap_or_default();

                    let children = files
                        .iter()
                        .map(|file| FileChildInfo {
                            filename: file["name"].as_str().unwrap_or("arquivo_pixeldrain").to_string(),
                            size: file["size"].as_u64().unwrap_or(0),
                            mime_type: file["mime_type"].as_str().map(String::from),
                            is_folder: false,
                            source_url: file["id"]
                                .as_str()
                                .map(|id| format!("https://pixeldrain.com/u/{id}")),
                            bytes_downloaded: None,
                            speed_bps: None,
                            eta_secs: None,
                            status: None,
                        })
                        .collect::<Vec<_>>();

                    let total_size = children.iter().map(|child| child.size).sum();

                    Ok(FileInfo {
                        filename: Self::safe_filename(
                            json["title"].as_str().unwrap_or("lista_pixeldrain"),
                            "lista_pixeldrain",
                        ),
                        size: total_size,
                        mime_type: None,
                        is_folder: true,
                        children: Some(children),
                    })
                }
            }
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
            let target = Self::parse_target(url)
                .ok_or_else(|| anyhow!("URL do PixelDrain inválida: {url}"))?;

            let client = reqwest::Client::new();
            let started_at = tokio::time::Instant::now();

            match target {
                PixelDrainTarget::File { id } => {
                    let download_url = format!("https://pixeldrain.com/api/file/{id}");
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

                    while let Some(chunk) = stream.next().await {
                        let chunk = chunk?;
                        file.write_all(&chunk).await?;
                        let chunk_len = chunk.len() as u64;
                        downloaded += chunk_len;
                        session_downloaded += chunk_len;

                        let _ = progress_tx.send(ProgressUpdate {
                            bytes_downloaded: downloaded,
                            total_bytes: total,
                            child_filename: None,
                            child_bytes_downloaded: None,
                            child_total_bytes: None,
                            child_speed_bps: None,
                            child_eta_secs: None,
                        }).await;
                        apply_speed_limit(started_at, session_downloaded, speed_limit_bps).await;
                    }

                    file.flush().await?;
                    Ok(downloaded)
                }
                PixelDrainTarget::List { id, selected_index } => {
                    let json = Self::fetch_list_info(&client, &id).await?;
                    let files = json["files"]
                        .as_array()
                        .cloned()
                        .ok_or_else(|| anyhow!("Lista do PixelDrain sem arquivos"))?;

                    if files.is_empty() {
                        return Err(anyhow!("Lista do PixelDrain vazia"));
                    }

                    tokio::fs::create_dir_all(dest_path).await?;
                    let _ = selected_index;

                    let total_size: u64 = files.iter()
                        .map(|file| file["size"].as_u64().unwrap_or(0))
                        .sum();
                    let mut downloaded_total = 0u64;
                    let mut session_downloaded = 0u64;

                    for (index, file_meta) in files.iter().enumerate() {
                        let file_id = file_meta["id"]
                            .as_str()
                            .ok_or_else(|| anyhow!("Arquivo da lista do PixelDrain sem ID"))?;
                        let default_name = format!("arquivo_{index}");
                        let filename = Self::safe_filename(
                            file_meta["name"].as_str().unwrap_or(&default_name),
                            &default_name,
                        );
                        let file_path = format!("{}/{}", dest_path.trim_end_matches('/'), filename);
                        let file_total = file_meta["size"].as_u64().unwrap_or(0);
                        let existing_bytes = tokio::fs::metadata(&file_path)
                            .await
                            .ok()
                            .filter(|meta| meta.is_file())
                            .map(|meta| meta.len())
                            .unwrap_or(0);

                        if file_total > 0 && existing_bytes >= file_total {
                            downloaded_total += file_total;
                            continue;
                        }

                        let download_url = format!("https://pixeldrain.com/api/file/{file_id}");
                        let mut request = client.get(&download_url);
                        if existing_bytes > 0 {
                            request = request.header("Range", format!("bytes={existing_bytes}-"));
                        }
                        let resp = request.send().await?.error_for_status()?;
                        let resumed = existing_bytes > 0 && resp.status() == reqwest::StatusCode::PARTIAL_CONTENT;
                        if resumed {
                            downloaded_total += existing_bytes;
                        }

                        let mut file = if resumed {
                            OpenOptions::new().create(true).append(true).open(&file_path).await?
                        } else {
                            tokio::fs::File::create(&file_path).await?
                        };
                        let mut stream = resp.bytes_stream();
                        let mut child_session_downloaded = 0u64;
                        let child_started_at = tokio::time::Instant::now();

                        while let Some(chunk) = stream.next().await {
                            let chunk = chunk?;
                            file.write_all(&chunk).await?;
                            let chunk_len = chunk.len() as u64;
                            downloaded_total += chunk_len;
                            session_downloaded += chunk_len;
                            child_session_downloaded += chunk_len;

                            let child_elapsed = child_started_at.elapsed().as_secs_f64();
                            let child_speed = if child_elapsed > 0.0 {
                                (child_session_downloaded as f64 / child_elapsed) as u64
                            } else {
                                0
                            };
                            let child_downloaded = if resumed { existing_bytes } else { 0 } + child_session_downloaded;
                            let child_eta = if child_speed > 0 && file_total > child_downloaded {
                                (file_total - child_downloaded) / child_speed
                            } else {
                                0
                            };

                            let _ = progress_tx.send(ProgressUpdate {
                                bytes_downloaded: downloaded_total,
                                total_bytes: total_size,
                                child_filename: Some(filename.clone()),
                                child_bytes_downloaded: Some(child_downloaded),
                                child_total_bytes: Some(file_total),
                                child_speed_bps: Some(child_speed),
                                child_eta_secs: Some(child_eta),
                            }).await;
                            apply_speed_limit(started_at, session_downloaded, speed_limit_bps).await;
                        }

                        file.flush().await?;
                    }

                    Ok(downloaded_total)
                }
            }
        })
    }
}
