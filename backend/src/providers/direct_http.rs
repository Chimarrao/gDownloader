use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, ACCEPT_RANGES, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_TYPE, ETAG, LAST_MODIFIED, RANGE};
use reqwest::StatusCode;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio::time::Instant;

use crate::models::FileInfo;

use super::{
    apply_speed_limit, apply_speed_limit_divided, merge_parts_into, parse_url, DownloadContext,
    Provider, ProviderCapabilities, ProviderDefaults, ProgressUpdate,
};

pub struct DirectHttpProvider;

#[derive(Debug, Clone)]
struct DirectHttpMetadata {
    url: String,
    filename: String,
    size: u64,
    mime_type: Option<String>,
    accepts_ranges: bool,
    etag: Option<String>,
    last_modified: Option<String>,
}

#[derive(Debug, Clone)]
struct PartRange {
    index: usize,
    start: u64,
    end: u64,
}

#[derive(Debug, Clone)]
struct ResumeStore {
    db_path: Option<String>,
    download_key: String,
    url: String,
    etag: Option<String>,
    last_modified: Option<String>,
}

impl DirectHttpProvider {
    pub fn matches(url: &str) -> bool {
        parse_url(url)
            .map(|parsed| matches!(parsed.scheme(), "http" | "https"))
            .unwrap_or(false)
    }

    fn clean_url(url: &str) -> String {
        url.split('#').next().unwrap_or(url).trim().to_string()
    }

    fn download_key(url: &str, dest_path: &str) -> String {
        format!("{}\n{}", Self::clean_url(url), dest_path)
    }

    fn header_string(headers: &reqwest::header::HeaderMap, name: reqwest::header::HeaderName) -> Option<String> {
        headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    }

    fn filename_from_content_disposition(value: &str) -> Option<String> {
        for part in value.split(';').map(str::trim) {
            let lower = part.to_ascii_lowercase();
            if lower.starts_with("filename*=") {
                let raw = part.split_once('=')?.1.trim().trim_matches('"');
                let encoded = raw.strip_prefix("UTF-8''").unwrap_or(raw);
                return urlencoding::decode(encoded)
                    .ok()
                    .map(|decoded| decoded.into_owned())
                    .filter(|filename| !filename.trim().is_empty());
            }
            if lower.starts_with("filename=") {
                return part
                    .split_once('=')
                    .map(|(_, filename)| filename.trim().trim_matches('"').to_string())
                    .filter(|filename| !filename.trim().is_empty());
            }
        }
        None
    }

    fn filename_from_url(url: &str) -> String {
        parse_url(url)
            .and_then(|parsed| {
                parsed
                    .path_segments()
                    .and_then(|mut segments| segments.next_back().map(str::to_string))
            })
            .and_then(|segment| urlencoding::decode(&segment).ok().map(|value| value.into_owned()))
            .map(|filename| <Self as ProviderDefaults>::safe_filename(&filename, "download"))
            .filter(|filename| filename != "download" && !filename.trim().is_empty())
            .unwrap_or_else(|| "download".to_string())
    }

    fn captured_headers(raw: &HashMap<String, String>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for (name, value) in raw {
            let lower = name.to_ascii_lowercase();
            if matches!(
                lower.as_str(),
                "host" | "connection" | "content-length" | "range" | "accept-encoding"
            ) {
                continue;
            }
            if let (Ok(name), Ok(value)) = (
                HeaderName::from_bytes(lower.as_bytes()),
                HeaderValue::from_str(value),
            ) {
                headers.insert(name, value);
            }
        }
        headers
    }

    async fn probe(client: &reqwest::Client, url: &str, headers: &HeaderMap) -> Result<DirectHttpMetadata> {
        let clean_url = Self::clean_url(url);
        let response = client.head(&clean_url).headers(headers.clone()).send().await?;
        let response = if response.status() == StatusCode::METHOD_NOT_ALLOWED
            || response.status() == StatusCode::NOT_IMPLEMENTED
            || response.status() == StatusCode::FORBIDDEN
        {
            client
                .get(&clean_url)
                .headers(headers.clone())
                .header(RANGE, "bytes=0-0")
                .send()
                .await?
        } else {
            response
        }
        .error_for_status()?;

        let headers = response.headers();
        let disposition_filename = Self::header_string(headers, CONTENT_DISPOSITION)
            .as_deref()
            .and_then(Self::filename_from_content_disposition);
        let size = Self::header_string(headers, CONTENT_LENGTH)
            .and_then(|value| value.parse::<u64>().ok())
            .or_else(|| parse_content_range_total(headers))
            .unwrap_or(0);
        let accepts_ranges = Self::header_string(headers, ACCEPT_RANGES)
            .map(|value| value.to_ascii_lowercase().contains("bytes"))
            .unwrap_or(false)
            || response.status() == StatusCode::PARTIAL_CONTENT;

        Ok(DirectHttpMetadata {
            url: clean_url.clone(),
            filename: disposition_filename
                .map(|value| <Self as ProviderDefaults>::safe_filename(&value, "download"))
                .unwrap_or_else(|| Self::filename_from_url(&clean_url)),
            size,
            mime_type: Self::header_string(headers, CONTENT_TYPE),
            accepts_ranges,
            etag: Self::header_string(headers, ETAG),
            last_modified: Self::header_string(headers, LAST_MODIFIED),
        })
    }

    fn build_parts(total_bytes: u64, parallel_parts: usize) -> Vec<PartRange> {
        const MIN_PART_SIZE: u64 = 2 * 1024 * 1024;
        let useful_parts = (total_bytes / MIN_PART_SIZE).max(1) as usize;
        let part_count = parallel_parts.min(useful_parts).max(1);

        (0..part_count)
            .map(|index| PartRange {
                index,
                start: (total_bytes * index as u64) / part_count as u64,
                end: ((total_bytes * (index as u64 + 1)) / part_count as u64).saturating_sub(1),
            })
            .collect()
    }

    async fn load_resume_offsets(store: &ResumeStore) -> HashMap<usize, u64> {
        let Some(db_path) = store.db_path.clone() else {
            return HashMap::new();
        };
        let download_key = store.download_key.clone();
        let etag = store.etag.clone();
        let last_modified = store.last_modified.clone();

        tokio::task::spawn_blocking(move || {
            let conn = rusqlite::Connection::open(db_path).ok()?;
            let _ = conn.busy_timeout(std::time::Duration::from_secs(2));
            crate::db::load_direct_http_parts(
                &conn,
                &download_key,
                etag.as_deref(),
                last_modified.as_deref(),
            )
            .ok()
        })
        .await
        .ok()
        .flatten()
        .unwrap_or_default()
        .into_iter()
        .map(|state| (state.part_index, state.bytes_downloaded))
        .collect()
    }

    async fn save_resume_offset(store: &ResumeStore, part: &PartRange, bytes_downloaded: u64) {
        let Some(db_path) = store.db_path.clone() else {
            return;
        };
        let store = store.clone();
        let part = part.clone();
        let _ = tokio::task::spawn_blocking(move || {
            let conn = rusqlite::Connection::open(db_path)?;
            conn.busy_timeout(std::time::Duration::from_secs(2))?;
            crate::db::save_direct_http_part(
                &conn,
                &store.download_key,
                part.index,
                &store.url,
                store.etag.as_deref(),
                store.last_modified.as_deref(),
                part.start,
                part.end,
                bytes_downloaded,
            )
        })
        .await;
    }

    async fn clear_resume_offsets(store: &ResumeStore) {
        let Some(db_path) = store.db_path.clone() else {
            return;
        };
        let download_key = store.download_key.clone();
        let _ = tokio::task::spawn_blocking(move || {
            let conn = rusqlite::Connection::open(db_path)?;
            conn.busy_timeout(std::time::Duration::from_secs(2))?;
            crate::db::clear_direct_http_parts(&conn, &download_key)
        })
        .await;
    }

    async fn download_single(
        client: &reqwest::Client,
        metadata: &DirectHttpMetadata,
        dest_path: &str,
        speed_limit_bps: super::SpeedLimitBps,
        progress_tx: tokio::sync::mpsc::Sender<ProgressUpdate>,
        store: ResumeStore,
        headers: HeaderMap,
    ) -> Result<u64> {
        let part = PartRange {
            index: 0,
            start: 0,
            end: metadata.size.saturating_sub(1),
        };
        let existing_bytes = tokio::fs::metadata(dest_path)
            .await
            .ok()
            .filter(|meta| meta.is_file())
            .map(|meta| meta.len())
            .unwrap_or(0);
        let saved_offset = Self::load_resume_offsets(&store)
            .await
            .get(&0)
            .copied()
            .unwrap_or(if store.db_path.is_none() { existing_bytes } else { 0 });
        let mut resume_from = saved_offset.min(existing_bytes);

        let mut request = client.get(&metadata.url).headers(headers.clone());
        if resume_from > 0 && metadata.accepts_ranges {
            request = request.header(RANGE, format!("bytes={resume_from}-"));
        } else {
            resume_from = 0;
        }

        let response = request.send().await?;
        let resumed = resume_from > 0 && response.status() == StatusCode::PARTIAL_CONTENT;
        let response = if resume_from > 0 && !resumed {
            let _ = tokio::fs::remove_file(dest_path).await;
            resume_from = 0;
            client
                .get(&metadata.url)
                .headers(headers.clone())
                .send()
                .await?
                .error_for_status()?
        } else {
            response.error_for_status()?
        };

        let total = if metadata.size > 0 {
            metadata.size
        } else if resumed {
            parse_response_total(&response).unwrap_or(0)
        } else {
            response.content_length().unwrap_or(0)
        };
        let mut file = if resumed {
            OpenOptions::new().create(true).append(true).open(dest_path).await?
        } else {
            tokio::fs::File::create(dest_path).await?
        };
        let started_at = Instant::now();
        let mut stream = response.bytes_stream();
        let mut downloaded = resume_from;
        let mut session_downloaded = 0u64;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            for piece in chunk.chunks(65_536) {
                file.write_all(piece).await?;
                let piece_len = piece.len() as u64;
                downloaded = downloaded.saturating_add(piece_len);
                session_downloaded = session_downloaded.saturating_add(piece_len);
                Self::save_resume_offset(&store, &part, downloaded).await;
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
        if total > 0 && downloaded < total {
            return Err(anyhow!("Download HTTP incompleto: {downloaded}/{total} bytes"));
        }
        Self::clear_resume_offsets(&store).await;
        Ok(downloaded)
    }

    async fn download_segmented(
        client: &reqwest::Client,
        metadata: &DirectHttpMetadata,
        dest_path: &str,
        speed_limit_bps: super::SpeedLimitBps,
        parallel_parts: usize,
        progress_tx: tokio::sync::mpsc::Sender<ProgressUpdate>,
        store: ResumeStore,
        headers: HeaderMap,
    ) -> Result<u64> {
        let parts = Self::build_parts(metadata.size, parallel_parts);
        if parts.len() <= 1 {
            return Self::download_single(client, metadata, dest_path, speed_limit_bps, progress_tx, store, headers).await;
        }

        if let Some(parent) = Path::new(dest_path).parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let offsets = Self::load_resume_offsets(&store).await;
        let started_at = Instant::now();
        let initial_downloaded = Arc::new(AtomicU64::new(0));
        let mut normalized_offsets = HashMap::new();

        for part in &parts {
            let part_path = format!("{dest_path}.part{}", part.index);
            let part_len = part.end.saturating_sub(part.start).saturating_add(1);
            let file_len = tokio::fs::metadata(&part_path)
                .await
                .ok()
                .filter(|meta| meta.is_file())
                .map(|meta| meta.len())
                .unwrap_or(0);
            let offset = offsets
                .get(&part.index)
                .copied()
                .unwrap_or(if store.db_path.is_none() { file_len } else { 0 })
                .min(file_len)
                .min(part_len);
            if offset == 0 && file_len > 0 {
                let _ = tokio::fs::remove_file(&part_path).await;
            } else if offset < file_len {
                if let Ok(file) = OpenOptions::new().write(true).open(&part_path).await {
                    let _ = file.set_len(offset).await;
                }
            }
            normalized_offsets.insert(part.index, offset);
            initial_downloaded.fetch_add(offset, Ordering::Relaxed);
            Self::save_resume_offset(&store, part, offset).await;
        }

        let parts_len = parts.len();
        // JoinSet em vez de spawns soltos: se esta função for cancelada/dropada
        // (usuário cancelou o download, ou houve retry), o JoinSet é dropado e ABORTA
        // todas as partes — sem tasks órfãs escrevendo nos .partN e piscando progresso.
        let mut tasks = tokio::task::JoinSet::new();

        for part in parts.clone() {
            let offset = *normalized_offsets.get(&part.index).unwrap_or(&0);
            let part_len = part.end.saturating_sub(part.start).saturating_add(1);
            if offset >= part_len {
                continue;
            }

            let client = client.clone();
            let url = metadata.url.clone();
            let part_path = format!("{dest_path}.part{}", part.index);
            let progress_tx = progress_tx.clone();
            let store = store.clone();
            let headers = headers.clone();
            let total_downloaded = Arc::clone(&initial_downloaded);
            let total_bytes = metadata.size;
            let task_limit = Arc::clone(&speed_limit_bps);

            tasks.spawn(async move {
                let range_start = part.start.saturating_add(offset);
                let response = client
                    .get(&url)
                    .headers(headers)
                    .header(RANGE, format!("bytes={range_start}-{}", part.end))
                    .send()
                    .await?;

                if response.status() != StatusCode::PARTIAL_CONTENT {
                    return Err(anyhow!("Servidor HTTP ignorou Range para a parte {}", part.index));
                }

                let mut file = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&part_path)
                    .await?;
                let mut stream = response.error_for_status()?.bytes_stream();
                let mut part_downloaded = offset;
                let mut session_downloaded = 0u64;

                while let Some(chunk) = stream.next().await {
                    let chunk = chunk?;
                    for piece in chunk.chunks(65_536) {
                        file.write_all(piece).await?;
                        let piece_len = piece.len() as u64;
                        part_downloaded = part_downloaded.saturating_add(piece_len).min(part_len);
                        session_downloaded = session_downloaded.saturating_add(piece_len);
                        let downloaded = total_downloaded.fetch_add(piece_len, Ordering::Relaxed) + piece_len;

                        DirectHttpProvider::save_resume_offset(&store, &part, part_downloaded).await;
                        let _ = progress_tx
                            .send(ProgressUpdate {
                                bytes_downloaded: downloaded.min(total_bytes),
                                total_bytes,
                                child_path: Some(format!("part:{}", part.index)),
                                child_filename: None,
                                child_bytes_downloaded: Some(part_downloaded),
                                child_total_bytes: Some(part_len),
                                child_speed_bps: None,
                                child_eta_secs: None,
                            })
                            .await;
                        apply_speed_limit_divided(started_at, session_downloaded, &task_limit, parts_len).await;
                    }
                }

                file.flush().await?;
                if part_downloaded < part_len {
                    return Err(anyhow!(
                        "Parte HTTP incompleta {}: {}/{} bytes",
                        part.index,
                        part_downloaded,
                        part_len
                    ));
                }
                Ok::<(), anyhow::Error>(())
            });
        }

        // Se qualquer parte falhar, `?` retorna e o JoinSet é dropado → aborta o resto.
        while let Some(joined) = tasks.join_next().await {
            joined??;
        }

        merge_parts_into(
            dest_path,
            &parts.iter().map(|part| format!("{dest_path}.part{}", part.index)).collect::<Vec<_>>(),
        )
        .await?;
        Self::clear_resume_offsets(&store).await;
        Ok(metadata.size)
    }
}

impl ProviderDefaults for DirectHttpProvider {}

impl Provider for DirectHttpProvider {
    fn name(&self) -> &str {
        "Direct HTTP"
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_parallel_parts: true,
            ..ProviderCapabilities::default()
        }
    }

    fn get_file_info<'a>(
        &'a self,
        url: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FileInfo>> + Send + 'a>> {
        Box::pin(async move {
            let client = <Self as ProviderDefaults>::http_client()?;
            let metadata = Self::probe(&client, url, &HeaderMap::new()).await?;
            Ok(FileInfo {
                filename: metadata.filename,
                size: metadata.size,
                mime_type: metadata.mime_type,
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
        _selected_children: Option<Vec<String>>,
        progress_tx: tokio::sync::mpsc::Sender<ProgressUpdate>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u64>> + Send + 'a>> {
        self.download_with_context(
            url,
            dest_path,
            speed_limit_bps,
            parallel_parts,
            None,
            progress_tx,
            DownloadContext::default(),
        )
    }

    fn download_with_context<'a>(
        &'a self,
        url: &'a str,
        dest_path: &'a str,
        speed_limit_bps: super::SpeedLimitBps,
        parallel_parts: usize,
        _selected_children: Option<Vec<String>>,
        progress_tx: tokio::sync::mpsc::Sender<ProgressUpdate>,
        context: DownloadContext,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u64>> + Send + 'a>> {
        Box::pin(async move {
            let client = <Self as ProviderDefaults>::http_client_with_proxy(
                &context.proxy_mode,
                &context.proxy_host,
                context.proxy_port,
                context.proxy_username.as_deref(),
                context.proxy_password.as_deref(),
            )?;
            let headers = Self::captured_headers(&context.request_headers);
            let metadata = Self::probe(&client, url, &headers).await?;
            let store = ResumeStore {
                db_path: context.db_path,
                download_key: Self::download_key(&metadata.url, dest_path),
                url: metadata.url.clone(),
                etag: metadata.etag.clone(),
                last_modified: metadata.last_modified.clone(),
            };

            if metadata.accepts_ranges && metadata.size > 0 && parallel_parts > 1 {
                Self::download_segmented(
                    &client,
                    &metadata,
                    dest_path,
                    speed_limit_bps,
                    parallel_parts,
                    progress_tx,
                    store,
                    headers,
                )
                .await
            } else {
                Self::download_single(
                    &client,
                    &metadata,
                    dest_path,
                    speed_limit_bps,
                    progress_tx,
                    store,
                    headers,
                )
                .await
            }
        })
    }
}

fn parse_content_range_total(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    headers
        .get("content-range")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split('/').last())
        .and_then(|value| value.parse::<u64>().ok())
}

fn parse_response_total(response: &reqwest::Response) -> Option<u64> {
    parse_content_range_total(response.headers())
        .or_else(|| response.content_length())
}
