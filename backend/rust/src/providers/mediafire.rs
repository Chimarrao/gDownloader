use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use scraper::{Html, Selector};
use serde_json::Value;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;

use crate::models::{FileChildInfo, FileInfo};
use super::{apply_speed_limit, host_matches, parse_url, try_parallel_download, ProgressUpdate, Provider, ProviderDefaults};

pub struct MediaFireProvider;

#[derive(Debug, Clone)]
struct MediaFireFolderFileEntry {
    filename: String,
    path: String,
    size: u64,
    mime_type: Option<String>,
    source_url: String,
}

impl MediaFireProvider {
    pub fn matches(url: &str) -> bool {
        host_matches(url, &["mediafire.com", "www.mediafire.com"])
    }

    pub fn is_folder_url(url: &str) -> bool {
        url.contains("/folder/")
    }

    pub fn extract_folder_key(url: &str) -> Option<String> {
        let pos = url.find("/folder/")?;
        let after = &url[pos + 8..];
        let folder_key = after
            .split('/')
            .next()?
            .split('?')
            .next()?
            .split('#')
            .next()?
            .trim();

        if folder_key.is_empty() {
            return None;
        }

        Some(folder_key.to_string())
    }

    pub fn extract_selected_subfolder_key(url: &str) -> Option<String> {
        let fragment = url.split('#').nth(1)?.trim();
        if fragment.is_empty() {
            return None;
        }

        Some(fragment.to_string())
    }

    pub fn extract_filename_from_url(url: &str) -> Option<String> {
        let pos = url.find("/file/")?;
        let after = &url[pos + 6..];
        let mut parts = after.split('/').filter(|segment| !segment.is_empty());
        let _quickkey = parts.next()?;
        let filename = parts.next()?.trim();

        if filename.is_empty() {
            return None;
        }

        Some(filename.to_string())
    }

    pub fn extract_direct_link(html: &str) -> Option<String> {
        let document = Html::parse_document(html);

        if let Ok(selector) = Selector::parse("#downloadButton") {
            if let Some(el) = document.select(&selector).next() {
                if let Some(href) = el.value().attr("href") {
                    if href.starts_with("http") {
                        return Some(href.to_string());
                    }
                }
            }
        }

        if let Ok(selector) = Selector::parse("a[href]") {
            for el in document.select(&selector) {
                if let Some(href) = el.value().attr("href") {
                    let mediafire_host = parse_url(href)
                        .and_then(|parsed| parsed.host_str().map(str::to_string))
                        .map(|host| host == "mediafire.com" || host.ends_with(".mediafire.com"))
                        .unwrap_or(false);
                    if href.starts_with("http")
                        && mediafire_host
                        && (href.contains("/get/") || href.contains("/download/"))
                    {
                        return Some(href.to_string());
                    }
                }
            }
        }

        None
    }

    async fn resolve_direct_download_url(client: &reqwest::Client, page_url: &str) -> Result<String> {
        let html = client.get(page_url).send().await?.text().await?;
        Self::extract_direct_link(&html)
            .ok_or_else(|| anyhow!("Link de download não encontrado na página do MediaFire"))
    }

    async fn fetch_folder_info(client: &reqwest::Client, folder_key: &str) -> Result<Value> {
        Ok(client
            .get("https://www.mediafire.com/api/1.4/folder/get_info.php")
            .query(&[
                ("folder_key", folder_key),
                ("response_format", "json"),
            ])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }

    async fn fetch_folder_files(client: &reqwest::Client, folder_key: &str) -> Result<Vec<Value>> {
        let mut chunk = 1usize;
        let mut files = Vec::new();

        loop {
            let chunk_str = chunk.to_string();
            let json: Value = client
                .get("https://www.mediafire.com/api/1.4/folder/get_content.php")
                .query(&[
                    ("folder_key", folder_key),
                    ("content_type", "files"),
                    ("response_format", "json"),
                    ("chunk", chunk_str.as_str()),
                ])
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;

            let folder_content = &json["response"]["folder_content"];

            if let Some(page_files) = folder_content["files"].as_array() {
                files.extend(page_files.iter().cloned());
            }

            if folder_content["more_chunks"].as_str().unwrap_or("no") != "yes" {
                break;
            }

            chunk += 1;
        }

        Ok(files)
    }

    async fn fetch_folder_subfolders(client: &reqwest::Client, folder_key: &str) -> Result<Vec<Value>> {
        let mut chunk = 1usize;
        let mut folders = Vec::new();

        loop {
            let chunk_str = chunk.to_string();
            let json: Value = client
                .get("https://www.mediafire.com/api/1.4/folder/get_content.php")
                .query(&[
                    ("folder_key", folder_key),
                    ("content_type", "folders"),
                    ("response_format", "json"),
                    ("chunk", chunk_str.as_str()),
                ])
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;

            let folder_content = &json["response"]["folder_content"];

            if let Some(page_folders) = folder_content["folders"].as_array() {
                folders.extend(page_folders.iter().cloned());
            }

            if folder_content["more_chunks"].as_str().unwrap_or("no") != "yes" {
                break;
            }

            chunk += 1;
        }

        Ok(folders)
    }

    async fn resolve_effective_folder_key(client: &reqwest::Client, url: &str) -> Result<String> {
        let root_key = Self::extract_folder_key(url)
            .ok_or_else(|| anyhow!("URL de pasta do MediaFire inválida: {url}"))?;

        let Some(selected_key) = Self::extract_selected_subfolder_key(url) else {
            return Ok(root_key);
        };

        let folders_json: Value = client
            .get("https://www.mediafire.com/api/1.4/folder/get_content.php")
            .query(&[
                ("folder_key", root_key.as_str()),
                ("content_type", "folders"),
                ("response_format", "json"),
            ])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let matches_child = folders_json["response"]["folder_content"]["folders"]
            .as_array()
            .map(|folders| {
                folders.iter().any(|folder| {
                    folder["folderkey"]
                        .as_str()
                        .map(|folder_key| folder_key == selected_key)
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false);

        if matches_child {
            Ok(selected_key)
        } else {
            Ok(root_key)
        }
    }

    fn mediafire_file_size(file: &Value) -> u64 {
        file["size"]
            .as_str()
            .and_then(|raw| raw.parse::<u64>().ok())
            .unwrap_or(0)
    }

    async fn collect_folder_entries_recursive(
        client: &reqwest::Client,
        folder_key: &str,
        prefix: String,
    ) -> Result<Vec<MediaFireFolderFileEntry>> {
        let mut out = Vec::new();
        let mut pending = vec![(folder_key.to_string(), prefix)];

        while let Some((current_folder_key, current_prefix)) = pending.pop() {
            let files = Self::fetch_folder_files(client, &current_folder_key).await?;
            out.extend(files.iter().map(|file| {
                let filename = <Self as ProviderDefaults>::safe_filename(
                    file["filename"].as_str().unwrap_or("arquivo_mediafire"),
                    "arquivo_mediafire",
                );
                let path = if current_prefix.is_empty() {
                    filename.clone()
                } else {
                    format!("{}/{}", current_prefix, filename)
                };

                MediaFireFolderFileEntry {
                    filename,
                    path,
                    size: Self::mediafire_file_size(file),
                    mime_type: file["mimetype"].as_str().map(String::from),
                    source_url: file["links"]["normal_download"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                }
            }));

            let folders = Self::fetch_folder_subfolders(client, &current_folder_key).await?;
            for folder in folders {
                let child_key = folder["folderkey"]
                    .as_str()
                    .ok_or_else(|| anyhow!("Subpasta do MediaFire sem folderkey"))?;
                let folder_name = <Self as ProviderDefaults>::safe_filename(
                    folder["name"].as_str().unwrap_or("pasta_mediafire"),
                    "pasta_mediafire",
                );
                let next_prefix = if current_prefix.is_empty() {
                    folder_name
                } else {
                    format!("{}/{}", current_prefix, folder_name)
                };
                pending.push((child_key.to_string(), next_prefix));
            }
        }

        Ok(out)
    }

}

impl ProviderDefaults for MediaFireProvider {}

impl Provider for MediaFireProvider {
    fn name(&self) -> &str { "MediaFire" }

    fn get_file_info<'a>(&'a self, url: &'a str)
        -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FileInfo>> + Send + 'a>>
    {
        Box::pin(async move {
            let client = <Self as ProviderDefaults>::http_client()?;

            if Self::is_folder_url(url) {
                let folder_key = Self::resolve_effective_folder_key(&client, url).await?;
                let folder_info = Self::fetch_folder_info(&client, &folder_key).await?;
                let files = Self::collect_folder_entries_recursive(&client, &folder_key, String::new()).await?;

                let folder_name = folder_info["response"]["folder_info"]["name"]
                    .as_str()
                    .unwrap_or("pasta_mediafire");

                let children = files
                    .iter()
                    .map(|file| FileChildInfo {
                        filename: file.filename.clone(),
                        size: file.size,
                        mime_type: file.mime_type.clone(),
                        is_folder: false,
                        path: Some(file.path.clone()),
                        source_url: Some(file.source_url.clone()),
                        bytes_downloaded: None,
                        speed_bps: None,
                        eta_secs: None,
                        status: None,
                    })
                    .collect::<Vec<_>>();

                let total_size = children.iter().map(|child| child.size).sum();

                return Ok(FileInfo {
                    filename: <Self as ProviderDefaults>::safe_filename(folder_name, "pasta_mediafire"),
                    size: total_size,
                    mime_type: None,
                    is_folder: true,
                    children: Some(children),
                    ..Default::default()
                });
            }

            let direct_url = Self::resolve_direct_download_url(&client, url).await?;
            let resp = match client.head(&direct_url).send().await {
                Ok(resp) => Some(resp),
                Err(_) => {
                    client
                        .get(&direct_url)
                        .header("Range", "bytes=0-0")
                        .send()
                        .await
                        .ok()
                }
            };

            let (size, mime_type) = if let Some(resp) = resp {
                let size = resp.content_length().unwrap_or_else(|| {
                    resp.headers()
                        .get("content-range")
                        .and_then(|value| value.to_str().ok())
                        .and_then(|value| value.split('/').last())
                        .and_then(|value| value.parse::<u64>().ok())
                        .unwrap_or(0)
                });

                let mime_type = resp
                    .headers()
                    .get("content-type")
                    .and_then(|value| value.to_str().ok())
                    .map(String::from);

                (size, mime_type)
            } else {
                (0, None)
            };

            Ok(FileInfo {
                filename: Self::extract_filename_from_url(url)
                    .unwrap_or_else(|| "arquivo_mediafire".to_string()),
                size,
                mime_type,
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
        speed_limit_bps: super::SpeedLimitBps,
        parallel_parts: usize,
        selected_children: Option<Vec<String>>,
        progress_tx: tokio::sync::mpsc::Sender<ProgressUpdate>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u64>> + Send + 'a>>
    {
        Box::pin(async move {
            let client = <Self as ProviderDefaults>::http_client()?;

            if Self::is_folder_url(url) {
                let folder_key = Self::resolve_effective_folder_key(&client, url).await?;
                let mut files = Self::collect_folder_entries_recursive(&client, &folder_key, String::new()).await?;

                if let Some(selected) = selected_children {
                    let selected_set = selected.into_iter().collect::<std::collections::HashSet<_>>();
                    files.retain(|file| {
                        selected_set.contains(&file.source_url)
                    });
                }

                if files.is_empty() {
                    return Err(anyhow!("Pasta do MediaFire vazia ou sem arquivos acessíveis"));
                }

                tokio::fs::create_dir_all(dest_path).await?;

                let total_size: u64 = files.iter().map(|file| file.size).sum();
                let started_at = tokio::time::Instant::now();
                let mut downloaded_total = 0u64;
                let mut session_downloaded = 0u64;

                for file in &files {
                    let page_url = file.source_url.as_str();

                    let direct_url = Self::resolve_direct_download_url(&client, page_url).await?;
                    let output_path = format!("{}/{}", dest_path.trim_end_matches('/'), file.path);
                    if let Some(parent_dir) = std::path::Path::new(&output_path).parent() {
                        tokio::fs::create_dir_all(parent_dir).await?;
                    }
                    let filename = file.filename.clone();
                    let file_total = file.size;
                    let existing_bytes = tokio::fs::metadata(&output_path)
                        .await
                        .ok()
                        .filter(|meta| meta.is_file())
                        .map(|meta| meta.len())
                        .unwrap_or(0);

                    if file_total > 0 && existing_bytes >= file_total {
                        downloaded_total += file_total;
                        continue;
                    }

                    let mut request = client.get(&direct_url);
                    if existing_bytes > 0 {
                        request = request.header("Range", format!("bytes={existing_bytes}-"));
                    }
                    let resp = request.send().await?.error_for_status()?;
                    let resumed = existing_bytes > 0 && resp.status() == reqwest::StatusCode::PARTIAL_CONTENT;
                    if resumed {
                        downloaded_total += existing_bytes;
                    }

                    let mut file_handle = if resumed {
                        OpenOptions::new().create(true).append(true).open(&output_path).await?
                    } else {
                        tokio::fs::File::create(&output_path).await?
                    };
                    let mut stream = resp.bytes_stream();
                    let mut child_session_downloaded = 0u64;
                    let child_started_at = tokio::time::Instant::now();

                    while let Some(chunk) = stream.next().await {
                        let chunk = chunk?;
                        for piece in chunk.chunks(65_536) {
                            file_handle.write_all(piece).await?;
                            let piece_len = piece.len() as u64;
                            downloaded_total += piece_len;
                            session_downloaded += piece_len;
                            child_session_downloaded += piece_len;

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

                            let _ = progress_tx
                                .send(ProgressUpdate {
                                    bytes_downloaded: downloaded_total,
                                    total_bytes: total_size,
                                    child_path: None,
                                    child_filename: Some(filename.clone()),
                                    child_bytes_downloaded: Some(child_downloaded),
                                    child_total_bytes: Some(file_total),
                                    child_speed_bps: Some(child_speed),
                                    child_eta_secs: Some(child_eta),
                                })
                                .await;
                            apply_speed_limit(started_at, session_downloaded, &speed_limit_bps).await;
                        }
                    }

                    file_handle.flush().await?;
                }

                return Ok(downloaded_total);
            }

                let direct_url = Self::resolve_direct_download_url(&client, url).await?;
                let existing_bytes = tokio::fs::metadata(dest_path)
                    .await
                .ok()
                .filter(|meta| meta.is_file())
                    .map(|meta| meta.len())
                    .unwrap_or(0);

                if existing_bytes == 0 {
                    if let Some(downloaded) = try_parallel_download(
                        &client,
                        &direct_url,
                        dest_path,
                        &speed_limit_bps,
                        parallel_parts,
                        progress_tx.clone(),
                    )
                    .await?
                    {
                        return Ok(downloaded);
                    }
                }

                let (resp, resumed) =
                    <Self as ProviderDefaults>::send_with_resume_fallback(&client, &direct_url, dest_path, existing_bytes).await?;
            let total = if resumed {
                <Self as ProviderDefaults>::response_total_bytes(&resp, existing_bytes)
            } else {
                resp.content_length().unwrap_or(0)
            };

            let mut file = if resumed {
                OpenOptions::new().create(true).append(true).open(dest_path).await?
            } else {
                tokio::fs::File::create(dest_path).await?
            };
            let mut stream = resp.bytes_stream();
            let mut downloaded = if resumed { existing_bytes } else { 0 };
            let mut session_downloaded = 0u64;
            let started_at = tokio::time::Instant::now();

            while let Some(chunk) = stream.next().await {
                let chunk = chunk?;
                for piece in chunk.chunks(65_536) {
                    file.write_all(piece).await?;
                    let piece_len = piece.len() as u64;
                    downloaded += piece_len;
                    session_downloaded += piece_len;

                    let _ = progress_tx
                        .send(ProgressUpdate {
                            bytes_downloaded: downloaded,
                            total_bytes: total,
                            child_path: None,
                            child_filename: None,
                            child_bytes_downloaded: None,
                            child_total_bytes: None,
                            child_speed_bps: None,
                            child_eta_secs: None,
                        })
                        .await;
                    apply_speed_limit(started_at, session_downloaded, &speed_limit_bps).await;
                }
            }

            file.flush().await?;
            Ok(downloaded)
        })
    }
}
