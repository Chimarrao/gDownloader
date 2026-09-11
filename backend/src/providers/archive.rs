use std::collections::HashSet;

use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use serde_json::Value;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;

use crate::models::{FileChildInfo, FileInfo};

use super::{
    apply_speed_limit, host_matches, path_segments, ProgressUpdate, Provider, ProviderCapabilities,
    ProviderDefaults,
};

const ARCHIVE_HOSTS: &[&str] = &["archive.org", "www.archive.org"];

#[derive(Debug, Clone)]
enum ArchiveTarget {
    Item {
        identifier: String,
    },
    File {
        identifier: String,
        filename: String,
    },
}

#[derive(Debug, Clone)]
struct ArchiveFile {
    filename: String,
    size: u64,
    mime_type: Option<String>,
    source_url: String,
}

pub struct InternetArchiveProvider;

impl InternetArchiveProvider {
    pub fn matches(url: &str) -> bool {
        host_matches(url, ARCHIVE_HOSTS) && matches!(Self::parse_target(url), Some(_))
    }

    fn parse_target(url: &str) -> Option<ArchiveTarget> {
        if !host_matches(url, ARCHIVE_HOSTS) {
            return None;
        }
        let segments = path_segments(url);
        match segments.as_slice() {
            [prefix, identifier] if prefix == "download" && !identifier.is_empty() => {
                Some(ArchiveTarget::Item {
                    identifier: identifier.to_string(),
                })
            }
            [prefix, identifier, filename, ..]
                if prefix == "download" && !identifier.is_empty() && !filename.is_empty() =>
            {
                Some(ArchiveTarget::File {
                    identifier: identifier.to_string(),
                    filename: urlencoding::decode(filename).ok()?.into_owned(),
                })
            }
            _ => None,
        }
    }

    fn file_url(identifier: &str, filename: &str) -> String {
        format!(
            "https://archive.org/download/{}/{}",
            urlencoding::encode(identifier),
            urlencoding::encode(filename)
        )
    }

    async fn item_files(client: &reqwest::Client, identifier: &str) -> Result<Vec<ArchiveFile>> {
        let response = client
            .get(format!(
                "https://archive.org/metadata/{}",
                urlencoding::encode(identifier)
            ))
            .send()
            .await?
            .error_for_status()?;
        let metadata: Value = response.json().await?;
        let files = metadata["files"]
            .as_array()
            .ok_or_else(|| anyhow!("Internet Archive não retornou arquivos para este item"))?;
        let mut entries = files
            .iter()
            .filter_map(|file| {
                let filename = file["name"].as_str()?.trim();
                if filename.is_empty()
                    || filename.ends_with('/')
                    || filename.starts_with("__ia_")
                    || filename == format!("{identifier}_meta.xml")
                    || filename == format!("{identifier}_meta.sqlite")
                {
                    return None;
                }
                let size = file["size"]
                    .as_u64()
                    .or_else(|| file["size"].as_str()?.parse::<u64>().ok())
                    .unwrap_or(0);
                if size == 0 {
                    return None;
                }
                Some((
                    file["source"].as_str().unwrap_or_default().to_string(),
                    ArchiveFile {
                        filename: <Self as ProviderDefaults>::safe_filename(
                            filename,
                            "arquivo_archive",
                        ),
                        size,
                        mime_type: file["format"].as_str().map(str::to_string),
                        source_url: Self::file_url(identifier, filename),
                    },
                ))
            })
            .collect::<Vec<_>>();
        // Itens do Archive incluem metadados e derivados. Quando há originais,
        // a lista padrão deve mostrar só eles — derivados seguem acessíveis por
        // URL direta sem poluir o grupo do usuário.
        let has_original = entries.iter().any(|(source, _)| source == "original");
        if has_original {
            entries.retain(|(source, _)| source == "original");
        }
        Ok(entries.into_iter().map(|(_, file)| file).collect())
    }

    async fn stream_file(
        client: &reqwest::Client,
        file: &ArchiveFile,
        path: &str,
        base_downloaded: u64,
        total_size: u64,
        speed_limit_bps: &super::SpeedLimitBps,
        started_at: tokio::time::Instant,
        session_downloaded: &mut u64,
        child: bool,
        progress_tx: &tokio::sync::mpsc::Sender<ProgressUpdate>,
    ) -> Result<u64> {
        let existing = tokio::fs::metadata(path)
            .await
            .ok()
            .filter(|meta| meta.is_file())
            .map(|meta| meta.len())
            .unwrap_or(0);
        if file.size > 0 && existing >= file.size {
            return Ok(file.size);
        }
        let mut request = client.get(&file.source_url);
        if existing > 0 {
            request = request.header(reqwest::header::RANGE, format!("bytes={existing}-"));
        }
        let response = request.send().await?.error_for_status()?;
        let resumed = existing > 0 && response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
        let mut output = if resumed {
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .await?
        } else {
            tokio::fs::File::create(path).await?
        };
        let mut downloaded = if resumed { existing } else { 0 };
        let file_total = file.size.max(
            response
                .content_length()
                .unwrap_or(0)
                .saturating_add(downloaded),
        );
        let child_started = tokio::time::Instant::now();
        let mut child_session = 0u64;
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            for piece in chunk.chunks(65_536) {
                output.write_all(piece).await?;
                let written = piece.len() as u64;
                downloaded += written;
                *session_downloaded += written;
                child_session += written;
                let elapsed = child_started.elapsed().as_secs_f64();
                let child_speed = if elapsed > 0.0 {
                    (child_session as f64 / elapsed) as u64
                } else {
                    0
                };
                let child_eta = if child_speed > 0 && file_total > downloaded {
                    (file_total - downloaded) / child_speed
                } else {
                    0
                };
                let _ = progress_tx
                    .send(ProgressUpdate {
                        bytes_downloaded: base_downloaded + downloaded,
                        total_bytes: total_size.max(file_total),
                        child_path: None,
                        child_filename: child.then(|| file.filename.clone()),
                        child_bytes_downloaded: child.then_some(downloaded),
                        child_total_bytes: child.then_some(file_total),
                        child_speed_bps: child.then_some(child_speed),
                        child_eta_secs: child.then_some(child_eta),
                    })
                    .await;
                apply_speed_limit(started_at, *session_downloaded, speed_limit_bps).await;
            }
        }
        output.flush().await?;
        Ok(downloaded)
    }
}

impl ProviderDefaults for InternetArchiveProvider {}

impl Provider for InternetArchiveProvider {
    fn name(&self) -> &str {
        "Internet Archive"
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_folder: true,
            ..ProviderCapabilities::default()
        }
    }

    fn get_file_info<'a>(
        &'a self,
        url: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FileInfo>> + Send + 'a>> {
        Box::pin(async move {
            let target = Self::parse_target(url)
                .ok_or_else(|| anyhow!("URL do Internet Archive inválida"))?;
            let client = <Self as ProviderDefaults>::http_client()?;
            let (identifier, requested_file) = match target {
                ArchiveTarget::Item { identifier } => (identifier, None),
                ArchiveTarget::File {
                    identifier,
                    filename,
                } => (identifier, Some(filename)),
            };
            let files = Self::item_files(&client, &identifier).await?;
            if let Some(filename) = requested_file {
                let file = files
                    .into_iter()
                    .find(|file| file.filename == filename)
                    .ok_or_else(|| anyhow!("Arquivo não encontrado no item do Internet Archive"))?;
                return Ok(FileInfo {
                    filename: file.filename,
                    size: file.size,
                    mime_type: file.mime_type,
                    is_folder: false,
                    children: None,
                    ..Default::default()
                });
            }
            if files.is_empty() {
                return Err(anyhow!(
                    "Item do Internet Archive não contém arquivos originais disponíveis"
                ));
            }
            let size = files.iter().map(|file| file.size).sum();
            let children = files
                .into_iter()
                .map(|file| FileChildInfo {
                    filename: file.filename,
                    size: file.size,
                    mime_type: file.mime_type,
                    is_folder: false,
                    path: None,
                    source_url: Some(file.source_url),
                    bytes_downloaded: None,
                    speed_bps: None,
                    eta_secs: None,
                    status: None,
                })
                .collect();
            Ok(FileInfo {
                filename: <Self as ProviderDefaults>::safe_filename(
                    &identifier,
                    "internet_archive",
                ),
                size,
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
            let target = Self::parse_target(url)
                .ok_or_else(|| anyhow!("URL do Internet Archive inválida"))?;
            let client = <Self as ProviderDefaults>::http_client()?;
            let (identifier, requested_file, is_folder) = match target {
                ArchiveTarget::Item { identifier } => (identifier, None, true),
                ArchiveTarget::File {
                    identifier,
                    filename,
                } => (identifier, Some(filename), false),
            };
            let mut files = Self::item_files(&client, &identifier).await?;
            if let Some(filename) = requested_file {
                files.retain(|file| file.filename == filename);
            } else if let Some(selected) = selected_children {
                let selected = selected.into_iter().collect::<HashSet<_>>();
                files.retain(|file| selected.contains(&file.source_url));
            }
            if files.is_empty() {
                return Err(anyhow!(
                    "Nenhum arquivo selecionado disponível no Internet Archive"
                ));
            }
            let total = files.iter().map(|file| file.size).sum();
            if is_folder {
                tokio::fs::create_dir_all(dest_path).await?;
            }
            let started_at = tokio::time::Instant::now();
            let mut session_downloaded = 0u64;
            let mut completed = 0u64;
            for file in &files {
                let output = if is_folder {
                    format!("{}/{}", dest_path.trim_end_matches('/'), file.filename)
                } else {
                    dest_path.to_string()
                };
                let downloaded = Self::stream_file(
                    &client,
                    file,
                    &output,
                    completed,
                    total,
                    &speed_limit_bps,
                    started_at,
                    &mut session_downloaded,
                    is_folder,
                    &progress_tx,
                )
                .await?;
                completed += downloaded;
            }
            Ok(completed)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_item_and_file_urls() {
        assert!(matches!(
            InternetArchiveProvider::parse_target("https://archive.org/download/demo-item"),
            Some(ArchiveTarget::Item { .. })
        ));
        assert!(matches!(
            InternetArchiveProvider::parse_target(
                "https://archive.org/download/demo-item/video.mp4"
            ),
            Some(ArchiveTarget::File { .. })
        ));
    }
}
