use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use serde_json::Value;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;

use crate::models::{FileChildInfo, FileInfo};
use super::{apply_speed_limit, host_matches, path_segments, ProgressUpdate, Provider, ProviderDefaults};

pub struct DrimeProvider;

impl DrimeProvider {
    pub fn matches(url: &str) -> bool {
        host_matches(url, &["app.drime.cloud"])
            && matches!(path_segments(url).as_slice(), [first, second, _hash, ..] if first == "drive" && second == "s")
    }

    fn extract_share_hash(url: &str) -> Option<String> {
        let pos = url.find("/drive/s/")?;
        let after = &url[pos + 9..];
        let hash = after
            .split('?')
            .next()?
            .split('#')
            .next()?
            .split('/')
            .next()?
            .trim();
        if hash.is_empty() {
            None
        } else {
            Some(hash.to_string())
        }
    }

    async fn fetch_share_page(client: &reqwest::Client, share_hash: &str, page: usize) -> Result<Value> {
        let url = format!(
            "https://app.drime.cloud/api/v1/shareable-links/{share_hash}?withEntries=true&page={page}&order=updated_at:desc"
        );
        Ok(client
            .get(url)
            .header("Accept", "application/json")
            .header("X-Requested-With", "XMLHttpRequest")
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }

    async fn fetch_all_folder_children(client: &reqwest::Client, share_hash: &str) -> Result<(Value, Vec<Value>)> {
        let first_page = Self::fetch_share_page(client, share_hash, 1).await?;
        let last_page = first_page["folderChildren"]["last_page"]
            .as_u64()
            .unwrap_or(1) as usize;

        let mut children = first_page["folderChildren"]["data"]
            .as_array()
            .cloned()
            .unwrap_or_default();

        for page in 2..=last_page {
            let json = Self::fetch_share_page(client, share_hash, page).await?;
            if let Some(items) = json["folderChildren"]["data"].as_array() {
                children.extend(items.iter().cloned());
            }
        }

        Ok((first_page, children))
    }

    fn child_to_info(child: &Value) -> FileChildInfo {
        FileChildInfo {
            filename: child["name"]
                .as_str()
                .unwrap_or("arquivo_drime")
                .to_string(),
            size: child["file_size"].as_u64().unwrap_or(0),
            mime_type: child["mime"].as_str().map(String::from),
            is_folder: child["type"].as_str().unwrap_or("") == "folder",
            path: None,
            source_url: child["id"]
                .as_i64()
                .map(|id| format!("https://app.drime.cloud/api/v1/file-entries/{id}")),
            bytes_downloaded: None,
            speed_bps: None,
            eta_secs: None,
            status: None,
        }
    }

    fn entry_to_file_info(entry: &Value) -> FileInfo {
        FileInfo {
            filename: entry["name"]
                .as_str()
                .unwrap_or("arquivo_drime")
                .to_string(),
            size: entry["file_size"].as_u64().unwrap_or(0),
            mime_type: entry["mime"].as_str().map(String::from),
            is_folder: false,
            children: None,
            ..Default::default()
        }
    }

    async fn open_download_response(
        client: &reqwest::Client,
        entry_id: i64,
        share_id: i64,
        existing_bytes: u64,
        dest_path: &str,
    ) -> Result<(reqwest::Response, bool)> {
        let endpoint = format!(
            "https://app.drime.cloud/api/v1/file-entries/{entry_id}?shareable_link={share_id}"
        );

        if existing_bytes == 0 {
            return Ok((client.get(endpoint).send().await?.error_for_status()?, false));
        }

        let ranged = client
            .get(&endpoint)
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
            let fresh = client.get(endpoint).send().await?.error_for_status()?;
            return Ok((fresh, false));
        }

        Ok((ranged.error_for_status()?, false))
    }
}

impl ProviderDefaults for DrimeProvider {}

impl Provider for DrimeProvider {
    fn name(&self) -> &str { "Drime" }

    fn get_file_info<'a>(&'a self, url: &'a str)
        -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FileInfo>> + Send + 'a>>
    {
        Box::pin(async move {
            let share_hash = Self::extract_share_hash(url)
                .ok_or_else(|| anyhow!("URL do Drime inválida: {url}"))?;
            let client = <Self as ProviderDefaults>::http_client()?;
            let (page, children) = Self::fetch_all_folder_children(&client, &share_hash).await?;
            let entry = &page["link"]["entry"];
            let entry_type = entry["type"].as_str().unwrap_or("");

            if entry_type == "folder" {
                let mapped_children = children
                    .iter()
                    .filter(|child| child["type"].as_str().unwrap_or("") != "folder")
                    .map(Self::child_to_info)
                    .collect::<Vec<_>>();

                let total_size = mapped_children.iter().map(|child| child.size).sum();
                return Ok(FileInfo {
                    filename: entry["name"]
                        .as_str()
                        .unwrap_or("pasta_drime")
                        .to_string(),
                    size: total_size,
                    mime_type: None,
                    is_folder: true,
                    children: Some(mapped_children),
                    ..Default::default()
                });
            }

            Ok(Self::entry_to_file_info(entry))
        })
    }

    fn download<'a>(
        &'a self,
        url: &'a str,
        dest_path: &'a str,
        speed_limit_bps: Option<u64>,
        _parallel_parts: usize,
        selected_children: Option<Vec<String>>,
        progress_tx: tokio::sync::mpsc::Sender<ProgressUpdate>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u64>> + Send + 'a>>
    {
        Box::pin(async move {
            let share_hash = Self::extract_share_hash(url)
                .ok_or_else(|| anyhow!("URL do Drime inválida: {url}"))?;
            let client = <Self as ProviderDefaults>::http_client()?;
            let (page, children) = Self::fetch_all_folder_children(&client, &share_hash).await?;
            let share_id = page["link"]["id"]
                .as_i64()
                .ok_or_else(|| anyhow!("Resposta do Drime sem ID do compartilhamento"))?;
            let entry = &page["link"]["entry"];
            let entry_type = entry["type"].as_str().unwrap_or("");

            if entry_type == "folder" {
                let mut children = children;
                if let Some(selected) = selected_children {
                    let selected_set = selected.into_iter().collect::<std::collections::HashSet<_>>();
                    children.retain(|child| {
                        child["type"].as_str().unwrap_or("") != "folder"
                            && child["id"].as_i64().map(|id| {
                                selected_set.contains(&format!("https://app.drime.cloud/api/v1/file-entries/{id}"))
                            }).unwrap_or(false)
                    });
                }

                if children.is_empty() {
                    return Err(anyhow!("Pasta do Drime vazia ou sem arquivos acessíveis"));
                }

                tokio::fs::create_dir_all(dest_path).await?;
                let total_size: u64 = children.iter()
                    .filter(|child| child["type"].as_str().unwrap_or("") != "folder")
                    .map(|child| child["file_size"].as_u64().unwrap_or(0))
                    .sum();

                let started_at = tokio::time::Instant::now();
                let mut downloaded_total = 0u64;
                let mut session_downloaded = 0u64;

                for (index, child) in children.iter().enumerate() {
                    if child["type"].as_str().unwrap_or("") == "folder" {
                        continue;
                    }

                    let entry_id = child["id"]
                        .as_i64()
                        .ok_or_else(|| anyhow!("Arquivo do Drime sem ID"))?;
                    let default_name = format!("arquivo_drime_{index}");
                    let filename = <Self as ProviderDefaults>::safe_filename(
                        child["name"].as_str().unwrap_or(&default_name),
                        &default_name,
                    );
                    let file_path = format!("{}/{}", dest_path.trim_end_matches('/'), filename);
                    let file_total = child["file_size"].as_u64().unwrap_or(0);
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

                    let (resp, resumed) =
                        Self::open_download_response(&client, entry_id, share_id, existing_bytes, &file_path).await?;
                    let mut file = if resumed {
                        OpenOptions::new().create(true).append(true).open(&file_path).await?
                    } else {
                        tokio::fs::File::create(&file_path).await?
                    };
                    let total_for_child = if resumed {
                        <Self as ProviderDefaults>::response_total_bytes(&resp, existing_bytes)
                    } else {
                        resp.content_length().unwrap_or(file_total)
                    };
                    let mut child_downloaded = if resumed { existing_bytes } else { 0u64 };
                    let mut child_session_downloaded = 0u64;
                    let mut stream = resp.bytes_stream();

                    while let Some(chunk) = stream.next().await {
                        let chunk = chunk?;
                        file.write_all(&chunk).await?;
                        let chunk_len = chunk.len() as u64;
                        child_downloaded += chunk_len;
                        downloaded_total += chunk_len;
                        child_session_downloaded += chunk_len;
                        session_downloaded += chunk_len;

                        let elapsed = started_at.elapsed().as_secs_f64();
                        let speed = if elapsed > 0.0 {
                            (session_downloaded as f64 / elapsed) as u64
                        } else {
                            0
                        };
                        let child_elapsed = started_at.elapsed().as_secs_f64();
                        let child_speed = if child_elapsed > 0.0 {
                            (child_session_downloaded as f64 / child_elapsed) as u64
                        } else {
                            0
                        };
                        let eta = if speed > 0 && total_size > downloaded_total {
                            (total_size - downloaded_total) / speed
                        } else {
                            0
                        };
                        let child_eta = if child_speed > 0 && total_for_child > child_downloaded {
                            (total_for_child - child_downloaded) / child_speed
                        } else {
                            0
                        };

                        let _ = progress_tx.send(ProgressUpdate {
                            bytes_downloaded: downloaded_total,
                            total_bytes: total_size,
                            child_path: None,
                            child_filename: Some(filename.clone()),
                            child_bytes_downloaded: Some(child_downloaded),
                            child_total_bytes: Some(total_for_child),
                            child_speed_bps: Some(child_speed),
                            child_eta_secs: Some(child_eta),
                        }).await;
                        let _ = eta;
                        apply_speed_limit(started_at, session_downloaded, speed_limit_bps).await;
                    }

                    file.flush().await?;
                }

                return Ok(downloaded_total);
            }

            let entry_id = entry["id"]
                .as_i64()
                .ok_or_else(|| anyhow!("Resposta do Drime sem ID do arquivo"))?;
            let existing_bytes = tokio::fs::metadata(dest_path)
                .await
                .ok()
                .filter(|meta| meta.is_file())
                .map(|meta| meta.len())
                .unwrap_or(0);
            let (resp, resumed) =
                Self::open_download_response(&client, entry_id, share_id, existing_bytes, dest_path).await?;
            let total = if resumed {
                <Self as ProviderDefaults>::response_total_bytes(&resp, existing_bytes)
            } else {
                resp.content_length().unwrap_or(entry["file_size"].as_u64().unwrap_or(0))
            };
            let mut file = if resumed {
                OpenOptions::new().create(true).append(true).open(dest_path).await?
            } else {
                tokio::fs::File::create(dest_path).await?
            };
            let mut stream = resp.bytes_stream();
            let mut downloaded = if resumed { existing_bytes } else { 0u64 };
            let mut session_downloaded = 0u64;
            let started_at = tokio::time::Instant::now();

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
                apply_speed_limit(started_at, session_downloaded, speed_limit_bps).await;
            }

            file.flush().await?;
            Ok(downloaded)
        })
    }
}
