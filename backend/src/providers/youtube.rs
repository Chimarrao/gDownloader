use anyhow::{anyhow, Result};
use regex::Regex;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::Command;
use tokio::time::{timeout, Duration};

use crate::models::{DownloadStatus, FileChildInfo, FileInfo};

use super::{DownloadContext, Provider, ProviderDefaults, ProgressUpdate};

pub struct YouTubeProvider;

impl ProviderDefaults for YouTubeProvider {}

const SYNTHETIC_PROGRESS_TOTAL: u64 = 10_000;

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct YtdlpInfo {
    id: Option<String>,
    title: Option<String>,
    thumbnail: Option<String>,
    duration: Option<f64>,
    webpage_url: Option<String>,
    entries: Option<Vec<YtdlpEntry>>,
    formats: Option<Vec<YtdlpFormat>>,
    uploader: Option<String>,
    channel_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct YtdlpChannelInfo {
    thumbnail: Option<String>,
    title: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct YtdlpEntry {
    id: Option<String>,
    title: Option<String>,
    url: Option<String>,
    webpage_url: Option<String>,
    duration: Option<f64>,
    thumbnail: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct YtdlpFormat {
    format_id: Option<String>,
    ext: Option<String>,
    resolution: Option<String>,
    width: Option<u64>,
    height: Option<u64>,
    fps: Option<f64>,
    filesize: Option<u64>,
    filesize_approx: Option<u64>,
    vcodec: Option<String>,
    acodec: Option<String>,
    tbr: Option<f64>,
    format_note: Option<String>,
}

impl YouTubeProvider {
    pub fn matches(url: &str) -> bool {
        super::parse_url(url)
            .and_then(|parsed| parsed.host_str().map(str::to_ascii_lowercase))
            .map(|host| {
                host == "youtu.be"
                    || host.ends_with(".youtube.com")
                    || host == "youtube.com"
                    || host == "music.youtube.com"
            })
            .unwrap_or(false)
    }

    fn ytdlp_bin() -> String {
        std::env::var("GDOWNLOADER_YTDLP_BIN").unwrap_or_else(|_| "yt-dlp".to_string())
    }

    fn clean_url(url: &str) -> String {
        url.split("#ytdlp_").next().unwrap_or(url).trim().to_string()
    }

    fn fragment_value(url: &str, key: &str) -> Option<String> {
        let fragment = url.split('#').nth(1)?;
        for part in fragment.split('&') {
            let (name, value) = part.split_once('=')?;
            if name == key {
                return urlencoding::decode(value).ok().map(|v| v.into_owned());
            }
        }
        None
    }

    fn selected_format(selected_children: &Option<Vec<String>>) -> Option<String> {
        selected_children.as_ref().and_then(|children| {
            children.iter().find_map(|child| Self::fragment_value(child, "ytdlp_format"))
        })
    }

    fn selected_value(selected_children: &Option<Vec<String>>, key: &str) -> Option<String> {
        selected_children.as_ref().and_then(|children| {
            children.iter().find_map(|child| Self::fragment_value(child, key))
        })
    }

    fn selected_flag(selected_children: &Option<Vec<String>>, key: &str) -> bool {
        Self::selected_value(selected_children, key)
            .map(|value| matches!(value.as_str(), "1" | "true" | "yes" | "on"))
            .unwrap_or(false)
    }

    fn normalize_merge_format(value: &str) -> Option<String> {
        let normalized = value.trim().trim_start_matches('.').to_ascii_lowercase();
        match normalized.as_str() {
            "mp4" | "mkv" | "webm" => Some(normalized),
            _ => None,
        }
    }

    fn selected_playlist_urls(selected_children: &Option<Vec<String>>) -> Option<Vec<String>> {
        let urls = selected_children
            .as_ref()?
            .iter()
            .filter(|child| !child.contains("#ytdlp_format="))
            .map(|child| Self::clean_url(child))
            .collect::<Vec<_>>();
        if urls.is_empty() { None } else { Some(urls) }
    }

    fn output_template(dest_path: &str) -> String {
        dest_path.to_string()
    }

    fn format_label(format: &YtdlpFormat) -> String {
        let id = format.format_id.as_deref().unwrap_or("best");
        let ext = format.ext.as_deref().unwrap_or("");
        let resolution = format
            .resolution
            .as_deref()
            .filter(|value| !value.is_empty() && *value != "audio only")
            .map(str::to_string)
            .or_else(|| format.height.map(|height| format!("{height}p")))
            .unwrap_or_else(|| "Audio".to_string());
        let note = format.format_note.as_deref().unwrap_or("");
        let codec = match (
            format.vcodec.as_deref().unwrap_or("none") != "none",
            format.acodec.as_deref().unwrap_or("none") != "none",
        ) {
            (true, true) => "video+audio",
            (true, false) => "video",
            (false, true) => "audio",
            _ => "media",
        };
        let fps = format
            .fps
            .filter(|value| *value >= 1.0)
            .map(|value| format!("{}fps", value.round() as u64))
            .unwrap_or_default();
        [resolution, fps, ext.to_string(), codec.to_string(), note.to_string(), format!("#{id}")]
            .into_iter()
            .filter(|part| !part.trim().is_empty())
            .collect::<Vec<_>>()
            .join(" · ")
    }

    fn resolution_label(format: &YtdlpFormat) -> String {
        let base = if let (Some(width), Some(height)) = (format.width, format.height) {
            if width > 0 && height > 0 {
                format!("{width}x{height}")
            } else {
                String::new()
            }
        } else {
            format
            .resolution
            .as_deref()
            .filter(|value| !value.is_empty() && *value != "audio only")
            .map(str::to_string)
            .or_else(|| format.height.map(|height| format!("{height}p")))
            .unwrap_or_else(|| "Audio".to_string())
        };
        let fps = format
            .fps
            .filter(|value| *value >= 1.0)
            .map(|value| format!(" {}fps", value.round() as u64))
            .unwrap_or_default();
        format!("{base}{fps}")
    }

    fn format_size(format: &YtdlpFormat, duration_secs: u64) -> u64 {
        format
            .filesize
            .or(format.filesize_approx)
            .or_else(|| {
                let tbr = format.tbr?;
                if duration_secs == 0 {
                    return None;
                }
                Some(((tbr * 1000.0 / 8.0) * duration_secs as f64).round() as u64)
            })
            .unwrap_or(0)
    }

    fn best_audio_format<'a>(formats: &'a [YtdlpFormat], duration_secs: u64) -> Option<&'a YtdlpFormat> {
        formats
            .iter()
            .filter(|format| {
                format.vcodec.as_deref().unwrap_or("none") == "none"
                    && format.acodec.as_deref().unwrap_or("none") != "none"
            })
            .max_by(|left, right| {
                left.tbr
                    .unwrap_or(0.0)
                    .partial_cmp(&right.tbr.unwrap_or(0.0))
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| Self::format_size(left, duration_secs).cmp(&Self::format_size(right, duration_secs)))
            })
    }

    fn best_video_format<'a>(formats: &'a [YtdlpFormat], duration_secs: u64) -> Option<&'a YtdlpFormat> {
        formats
            .iter()
            .filter(|format| format.vcodec.as_deref().unwrap_or("none") != "none")
            .max_by(|left, right| {
                left.height
                    .unwrap_or(0)
                    .cmp(&right.height.unwrap_or(0))
                    .then_with(|| {
                        left.fps
                            .unwrap_or(0.0)
                            .partial_cmp(&right.fps.unwrap_or(0.0))
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .then_with(|| Self::format_size(left, duration_secs).cmp(&Self::format_size(right, duration_secs)))
            })
    }

    fn build_format_children(url: &str, formats: &[YtdlpFormat], duration_secs: u64) -> Vec<FileChildInfo> {
        let best_video = Self::best_video_format(formats, duration_secs);
        let best_audio = Self::best_audio_format(formats, duration_secs);
        let best_resolution = best_video
            .map(Self::resolution_label)
            .unwrap_or_else(|| "Melhor disponivel".to_string());
        let best_size = best_video.map(|format| Self::format_size(format, duration_secs)).unwrap_or(0)
            + best_audio.map(|format| Self::format_size(format, duration_secs)).unwrap_or(0);

        let mut children = vec![FileChildInfo {
            filename: format!("Melhor qualidade - {best_resolution}"),
            size: best_size,
            mime_type: Some("video/*".to_string()),
            is_folder: false,
            path: Some("bestvideo+bestaudio".to_string()),
            source_url: Some(format!(
                "{}#ytdlp_format={}",
                Self::clean_url(url),
                urlencoding::encode("bestvideo+bestaudio")
            )),
            bytes_downloaded: Some(0),
            speed_bps: Some(0),
            eta_secs: Some(0),
            status: Some(DownloadStatus::Pending),
        }];

        let mut usable = formats
            .iter()
            .filter(|format| format.format_id.as_deref().unwrap_or("").len() > 0)
            .filter(|format| format.vcodec.as_deref().unwrap_or("none") != "none" || format.acodec.as_deref().unwrap_or("none") != "none")
            .collect::<Vec<_>>();
        usable.sort_by(|a, b| {
            b.height
                .unwrap_or(0)
                .cmp(&a.height.unwrap_or(0))
                .then_with(|| Self::format_size(b, duration_secs).cmp(&Self::format_size(a, duration_secs)))
        });

        for format in usable.into_iter().take(80) {
            let id = format.format_id.as_deref().unwrap_or("best").to_string();
            let has_video = format.vcodec.as_deref().unwrap_or("none") != "none";
            let has_audio = format.acodec.as_deref().unwrap_or("none") != "none";
            let resolved = if has_video && !has_audio {
                format!("{id}+bestaudio")
            } else {
                id.clone()
            };
            children.push(FileChildInfo {
                filename: Self::format_label(format),
                size: Self::format_size(format, duration_secs),
                mime_type: Some(if has_audio && !has_video { "audio/*" } else { "video/*" }.to_string()),
                is_folder: false,
                path: Some(id),
                source_url: Some(format!(
                    "{}#ytdlp_format={}",
                    Self::clean_url(url),
                    urlencoding::encode(&resolved)
                )),
                bytes_downloaded: Some(0),
                speed_bps: Some(0),
                eta_secs: Some(0),
                status: Some(DownloadStatus::Pending),
            });
        }
        children
    }

    fn playlist_entry_url(entry: &YtdlpEntry) -> Option<String> {
        entry.webpage_url.clone().or_else(|| {
            entry.url.as_ref().map(|url| {
                if url.starts_with("http://") || url.starts_with("https://") {
                    url.clone()
                } else if let Some(id) = entry.id.as_deref().filter(|id| !id.is_empty()) {
                    format!("https://www.youtube.com/watch?v={id}")
                } else {
                    format!("https://www.youtube.com/watch?v={url}")
                }
            })
        })
    }

    async fn read_info(url: &str, flat_playlist: bool, context: Option<&DownloadContext>) -> Result<YtdlpInfo> {
        let mut args = Vec::<String>::new();
        if let Some(context) = context {
            Self::apply_cookies_args(&mut args, context);
            Self::apply_proxy_args(&mut args, context);
        }
        args.extend(["-J".to_string(), "--no-warnings".to_string()]);
        if flat_playlist {
            args.push("--flat-playlist".to_string());
        } else {
            args.push("--no-playlist".to_string());
        }
        args.push(url.to_string());

        let output = timeout(
            Duration::from_secs(45),
            Command::new(Self::ytdlp_bin()).args(args).output(),
        )
        .await
        .map_err(|_| anyhow!("yt-dlp demorou demais para ler metadados do YouTube"))??;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(anyhow!(
                "{}",
                if stderr.is_empty() { "yt-dlp falhou ao ler o link do YouTube" } else { &stderr }
            ));
        }

        Ok(serde_json::from_slice(&output.stdout)?)
    }

    async fn read_channel_info(channel_id: &str) -> Option<YtdlpChannelInfo> {
        let channel_url = format!("https://www.youtube.com/channel/{channel_id}");
        let output = timeout(
            Duration::from_secs(10),
            Command::new(Self::ytdlp_bin())
                .args([
                    "-J",
                    "--flat-playlist",
                    "--playlist-items",
                    "0",
                    "--no-warnings",
                    &channel_url,
                ])
                .output(),
        )
        .await
        .ok()?
        .ok()?;

        if !output.status.success() {
            return None;
        }

        serde_json::from_slice::<YtdlpChannelInfo>(&output.stdout).ok()
    }

    async fn info_for(url: &str, context: Option<&DownloadContext>) -> Result<FileInfo> {
        let clean_url = Self::clean_url(url);
        let is_playlist = clean_url.contains("list=");
        if is_playlist {
            let info = Self::read_info(&clean_url, true, context).await?;
            let mut children = info
                .entries
                .unwrap_or_default()
                .into_iter()
                .filter_map(|entry| {
                    let source_url = Self::playlist_entry_url(&entry)?;
                    Some(FileChildInfo {
                        filename: <Self as ProviderDefaults>::safe_filename(
                            entry.title.as_deref().unwrap_or("Video"),
                            "Video",
                        ),
                        size: 0,
                        mime_type: Some("video/*".to_string()),
                        is_folder: false,
                        path: entry.id.clone(),
                        source_url: Some(source_url),
                        bytes_downloaded: Some(0),
                        speed_bps: Some(0),
                        eta_secs: Some(0),
                        status: Some(DownloadStatus::Pending),
                    })
                })
                .collect::<Vec<_>>();
            if children.is_empty() {
                return Err(anyhow!("Playlist do YouTube sem videos acessiveis"));
            }
            let filename = <Self as ProviderDefaults>::safe_filename(
                info.title.as_deref().unwrap_or("YouTube Playlist"),
                "YouTube Playlist",
            );
            return Ok(FileInfo {
                filename,
                size: 0,
                mime_type: Some("application/vnd.youtube.playlist".to_string()),
                is_folder: true,
                children: Some(std::mem::take(&mut children)),
                ..Default::default()
            });
        }

        let info = Self::read_info(&clean_url, false, context).await?;
        let title = <Self as ProviderDefaults>::safe_filename(
            info.title.as_deref().unwrap_or("YouTube Video"),
            "YouTube Video",
        );
        let merge_format = context
            .and_then(|context| Self::normalize_merge_format(context.youtube_merge_format.trim()))
            .unwrap_or_else(|| "mp4".to_string());
        let filename = format!("{title}.{merge_format}");
        let duration_secs = info.duration.unwrap_or(0.0).max(0.0).round() as u64;
        let children = Self::build_format_children(
            &clean_url,
            &info.formats.unwrap_or_default(),
            duration_secs,
        );

        // Avatar do canal: reaproveita o que já está em cache para evitar a
        // chamada extra (cara) do yt-dlp. Só busca quando não há cache.
        let cached_channel_thumb = context
            .and_then(|context| context.cached_channel_thumbnail_url.clone())
            .filter(|value| !value.is_empty());
        let channel_thumbnail_url = if cached_channel_thumb.is_some() {
            cached_channel_thumb
        } else if let Some(cid) = info.channel_id.clone() {
            Self::read_channel_info(&cid).await.and_then(|c| c.thumbnail)
        } else {
            None
        };

        Ok(FileInfo {
            filename,
            size: children.iter().map(|child| child.size).max().unwrap_or(0),
            mime_type: Some("video/*".to_string()),
            is_folder: false,
            children: Some(children),
            thumbnail_url: info.thumbnail.clone(),
            channel_name: info.uploader.clone(),
            channel_thumbnail_url,
        })
    }

    fn apply_cookies_args(args: &mut Vec<String>, context: &DownloadContext) {
        if !context.youtube_use_cookies {
            return;
        }
        if !context.youtube_cookies_file.trim().is_empty() {
            args.push("--cookies".to_string());
            args.push(context.youtube_cookies_file.clone());
            return;
        }
        args.push("--cookies-from-browser".to_string());
        args.push(if context.youtube_cookie_browser.trim().is_empty() {
            "chrome".to_string()
        } else {
            context.youtube_cookie_browser.clone()
        });
    }

    fn apply_proxy_args(args: &mut Vec<String>, context: &DownloadContext) {
        let proxy = match context.proxy_mode.as_str() {
            "tor" => {
                let host = if context.proxy_host.trim().is_empty() { "127.0.0.1" } else { &context.proxy_host };
                let port = if context.proxy_port == 0 { 9050 } else { context.proxy_port };
                if let Some(username) = context.proxy_username.as_deref() {
                    let password = context.proxy_password.as_deref().unwrap_or("");
                    format!("socks5://{username}:{password}@{host}:{port}")
                } else {
                    format!("socks5://{host}:{port}")
                }
            }
            "socks5" => format!("socks5://{}:{}", context.proxy_host, context.proxy_port),
            "http" | "https" => format!("http://{}:{}", context.proxy_host, context.proxy_port),
            _ => String::new(),
        };
        if !proxy.is_empty() {
            args.push("--proxy".to_string());
            args.push(proxy);
        }
    }

    fn parse_speed_bps(value: &str) -> u64 {
        let trimmed = value.trim();
        if trimmed.is_empty() || trimmed == "N/A" || trimmed == "--" {
            return 0;
        }
        let Some(captures) = Regex::new(r#"([0-9.]+)\s*([KMGT]?)(?:i?B)?/s"#)
            .ok()
            .and_then(|re| re.captures(trimmed))
        else {
            return 0;
        };
        let number = captures
            .get(1)
            .and_then(|value| value.as_str().parse::<f64>().ok())
            .unwrap_or(0.0);
        let multiplier = match captures.get(2).map(|value| value.as_str().to_ascii_uppercase()).as_deref() {
            Some("T") => 1024.0_f64.powi(4),
            Some("G") => 1024.0_f64.powi(3),
            Some("M") => 1024.0_f64.powi(2),
            Some("K") => 1024.0,
            _ => 1.0,
        };
        (number * multiplier).round().max(0.0) as u64
    }

    fn parse_eta_secs(value: &str) -> u64 {
        let value = value.trim();
        if value.is_empty() || value == "N/A" || value == "--" {
            return 0;
        }
        let parts = value
            .split(':')
            .filter_map(|part| part.parse::<u64>().ok())
            .collect::<Vec<_>>();
        match parts.as_slice() {
            [hours, minutes, seconds] => hours * 3600 + minutes * 60 + seconds,
            [minutes, seconds] => minutes * 60 + seconds,
            [seconds] => *seconds,
            _ => 0,
        }
    }

    fn parse_progress(line: &str) -> Option<(u64, u64, u64, u64)> {
        let re = Regex::new(r#"GDLPROG\s*([0-9.]+)%\s+(.+?)\s+(\S+)\s*$"#).ok()?;
        let captures = re.captures(line)?;
        let percent = captures.get(1)?.as_str().parse::<f64>().unwrap_or(0.0).clamp(0.0, 100.0);
        let speed = Self::parse_speed_bps(captures.get(2)?.as_str());
        let eta = Self::parse_eta_secs(captures.get(3)?.as_str());
        let total = SYNTHETIC_PROGRESS_TOTAL;
        let downloaded = ((percent / 100.0) * SYNTHETIC_PROGRESS_TOTAL as f64).round() as u64;
        Some((downloaded, total, speed, eta))
    }

    async fn output_size(dest_path: &str, started_at: std::time::SystemTime) -> u64 {
        if let Ok(metadata) = tokio::fs::metadata(dest_path).await {
            if metadata.is_file() {
                return metadata.len();
            }
        }

        let path = PathBuf::from(dest_path);
        let Some(parent) = path.parent() else {
            return 0;
        };
        let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
            return 0;
        };

        let mut newest_size = 0u64;
        let mut newest_modified = started_at;
        let Ok(mut entries) = tokio::fs::read_dir(parent).await else {
            return 0;
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let Ok(metadata) = entry.metadata().await else {
                continue;
            };
            if !metadata.is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with(stem) {
                continue;
            }
            let modified = metadata.modified().unwrap_or(started_at);
            if modified >= newest_modified {
                newest_modified = modified;
                newest_size = metadata.len();
            }
        }
        newest_size
    }

    async fn send_stage(
        progress_tx: &tokio::sync::mpsc::Sender<ProgressUpdate>,
        child_key: &str,
        stage: &str,
        bytes_downloaded: u64,
        total_bytes: u64,
    ) {
        let _ = progress_tx
            .send(ProgressUpdate {
                bytes_downloaded,
                total_bytes,
                child_path: Some(child_key.to_string()),
                child_filename: Some(stage.to_string()),
                child_bytes_downloaded: Some(bytes_downloaded),
                child_total_bytes: Some(total_bytes),
                child_speed_bps: Some(0),
                child_eta_secs: Some(0),
            })
            .await;
    }

    fn phase_bounds(phase_count: u32, has_split_media: bool) -> (u64, u64) {
        if !has_split_media {
            return (0, 9_500);
        }

        match phase_count {
            0 | 1 => (0, 4_500),
            2 => (4_500, 4_500),
            _ => (9_000, 0),
        }
    }

    fn phase_progress(downloaded: u64, phase_count: u32, has_split_media: bool) -> u64 {
        let (base, span) = Self::phase_bounds(phase_count, has_split_media);
        if span == 0 {
            base
        } else {
            base + downloaded.min(SYNTHETIC_PROGRESS_TOTAL).saturating_mul(span) / SYNTHETIC_PROGRESS_TOTAL
        }
    }

    fn merge_progress(has_split_media: bool) -> u64 {
        if has_split_media {
            9_250
        } else {
            9_750
        }
    }
}

impl Provider for YouTubeProvider {
    fn name(&self) -> &str {
        "YouTube"
    }

    fn get_file_info<'a>(&'a self, url: &'a str) -> Pin<Box<dyn std::future::Future<Output = Result<FileInfo>> + Send + 'a>> {
        Box::pin(async move { Self::info_for(url, None).await })
    }

    fn get_file_info_with_context<'a>(
        &'a self,
        url: &'a str,
        context: DownloadContext,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<FileInfo>> + Send + 'a>> {
        Box::pin(async move { Self::info_for(url, Some(&context)).await })
    }

    fn download<'a>(
        &'a self,
        url: &'a str,
        dest_path: &'a str,
        speed_limit_bps: Option<u64>,
        _parallel_parts: usize,
        selected_children: Option<Vec<String>>,
        progress_tx: tokio::sync::mpsc::Sender<ProgressUpdate>,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<u64>> + Send + 'a>> {
        self.download_with_context(
            url,
            dest_path,
            speed_limit_bps,
            1,
            selected_children,
            progress_tx,
            DownloadContext::default(),
        )
    }

    fn download_with_context<'a>(
        &'a self,
        url: &'a str,
        dest_path: &'a str,
        speed_limit_bps: Option<u64>,
        _parallel_parts: usize,
        selected_children: Option<Vec<String>>,
        progress_tx: tokio::sync::mpsc::Sender<ProgressUpdate>,
        context: DownloadContext,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<u64>> + Send + 'a>> {
        Box::pin(async move {
            if let Some(parent) = Path::new(dest_path).parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            let selected_playlist_urls = Self::selected_playlist_urls(&selected_children);
            let urls = selected_playlist_urls.unwrap_or_else(|| vec![Self::clean_url(url)]);
            let format = Self::selected_format(&selected_children)
                .or_else(|| Self::fragment_value(url, "ytdlp_format"))
                .unwrap_or_else(|| "bestvideo+bestaudio/best".to_string());
            let has_split_media = format.contains('+') || format.contains("bestvideo");
            let selected_merge_format = Self::selected_value(&selected_children, "ytdlp_merge_format")
                .or_else(|| Self::fragment_value(url, "ytdlp_merge_format"))
                .and_then(|value| Self::normalize_merge_format(&value));
            let write_thumbnail = Self::selected_flag(&selected_children, "ytdlp_write_thumbnail")
                || Self::fragment_value(url, "ytdlp_write_thumbnail")
                    .map(|value| matches!(value.as_str(), "1" | "true" | "yes" | "on"))
                    .unwrap_or(false);
            let write_subs_selected = Self::selected_flag(&selected_children, "ytdlp_write_subs")
                || Self::fragment_value(url, "ytdlp_write_subs")
                    .map(|value| matches!(value.as_str(), "1" | "true" | "yes" | "on"))
                    .unwrap_or(false);
            let multi_audio = Self::selected_flag(&selected_children, "ytdlp_multi_audio")
                || Self::fragment_value(url, "ytdlp_multi_audio")
                    .map(|value| matches!(value.as_str(), "1" | "true" | "yes" | "on"))
                    .unwrap_or(false);
            let progress_child_key = selected_children
                .as_ref()
                .and_then(|children| children.first().cloned())
                .unwrap_or_else(|| format.clone());

            let mut grand_total = 0u64;
            let mut grand_downloaded = 0u64;
            let started_at = std::time::SystemTime::now();

            for item_url in urls {
                let mut args = Vec::<String>::new();
                Self::apply_cookies_args(&mut args, &context);
                Self::apply_proxy_args(&mut args, &context);
                let merge_format = selected_merge_format
                    .as_deref()
                    .or_else(|| {
                        let value = context.youtube_merge_format.trim();
                        if value.is_empty() { None } else { Some(value) }
                    })
                    .and_then(Self::normalize_merge_format)
                    .unwrap_or_else(|| "mp4".to_string());
                args.extend([
                    "-f".to_string(),
                    format.clone(),
                    "--merge-output-format".to_string(),
                    merge_format,
                    "--newline".to_string(),
                    "--progress-template".to_string(),
                    "download:GDLPROG %(progress._percent_str)s %(progress._speed_str)s %(progress._eta_str)s".to_string(),
                    "--retries".to_string(),
                    "3".to_string(),
                    "--fragment-retries".to_string(),
                    "3".to_string(),
                    "-c".to_string(),
                    "-o".to_string(),
                    Self::output_template(dest_path),
                ]);

                if let Some(limit) = speed_limit_bps.filter(|value| *value > 0) {
                    args.push("--limit-rate".to_string());
                    args.push(limit.to_string());
                }

                if write_thumbnail {
                    args.push("--write-thumbnail".to_string());
                }

                if multi_audio {
                    args.push("--audio-multistreams".to_string());
                }

                if context.youtube_download_subs || write_subs_selected {
                    args.push("--write-subs".to_string());
                    args.push("--write-auto-sub".to_string());
                    args.push("--sub-lang".to_string());
                    args.push(if context.youtube_sub_langs.trim().is_empty() {
                        "pt,en".to_string()
                    } else {
                        context.youtube_sub_langs.clone()
                    });
                    if context.youtube_embed_subs {
                        args.push("--embed-subs".to_string());
                    }
                }

                if context.youtube_split_chapters {
                    args.push("--split-chapters".to_string());
                }

                if let Ok(ffmpeg_bin) = std::env::var("GDOWNLOADER_FFMPEG_BIN") {
                    if !ffmpeg_bin.trim().is_empty() {
                        args.push("--ffmpeg-location".to_string());
                        args.push(ffmpeg_bin);
                    }
                }

                args.push(item_url.clone());

                let mut child = Command::new(Self::ytdlp_bin())
                    .args(args)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .kill_on_drop(true)
                    .spawn()?;

                let stdout = child.stdout.take().ok_or_else(|| anyhow!("Falha ao ler saida do yt-dlp"))?;
                let stderr = child.stderr.take();
                let stderr_task = tokio::spawn(async move {
                    let mut buffer = String::new();
                    if let Some(stderr) = stderr {
                        let mut reader = BufReader::new(stderr);
                        let _ = reader.read_to_string(&mut buffer).await;
                    }
                    buffer
                });
                let mut lines = BufReader::new(stdout).lines();
                let item_name = Path::new(&item_url)
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("YouTube")
                    .to_string();
                let mut item_downloaded = 0u64;
                let mut item_total = 0u64;
                let mut phase_count = 0u32;
                let mut current_stage = String::new();
                let mut last_synthetic_downloaded = 0u64;

                while let Some(line) = lines.next_line().await? {
                    let trimmed = line.trim();
                    if trimmed.starts_with("[download] Destination:") || trimmed.starts_with("[download] Resuming download at byte") {
                        phase_count = phase_count.saturating_add(1);
                        current_stage = if phase_count <= 1 {
                            "Baixando vídeo"
                        } else {
                            "Baixando áudio"
                        }
                        .to_string();
                        last_synthetic_downloaded = last_synthetic_downloaded.max(Self::phase_progress(
                            item_downloaded,
                            phase_count,
                            has_split_media,
                        ));
                        Self::send_stage(
                            &progress_tx,
                            &progress_child_key,
                            &current_stage,
                            grand_downloaded + last_synthetic_downloaded,
                            grand_total.max(SYNTHETIC_PROGRESS_TOTAL),
                        )
                        .await;
                    } else if trimmed.starts_with("[Merger]") || trimmed.starts_with("[ffmpeg]") || trimmed.contains("Merging formats") {
                        current_stage = "Mesclando arquivos".to_string();
                        last_synthetic_downloaded = last_synthetic_downloaded.max(Self::merge_progress(has_split_media));
                        Self::send_stage(
                            &progress_tx,
                            &progress_child_key,
                            &current_stage,
                            grand_downloaded + last_synthetic_downloaded,
                            grand_total.max(item_total).max(SYNTHETIC_PROGRESS_TOTAL),
                        )
                        .await;
                    }

                    if let Some((downloaded, total, speed, eta)) = Self::parse_progress(&line) {
                        if total > 0 {
                            item_total = total;
                            grand_total = grand_total.max(grand_downloaded + item_total);
                        }
                        item_downloaded = downloaded;
                        let synthetic_downloaded = Self::phase_progress(downloaded, phase_count, has_split_media);
                        last_synthetic_downloaded = last_synthetic_downloaded.max(synthetic_downloaded);
                        let _ = progress_tx
                            .send(ProgressUpdate {
                                bytes_downloaded: grand_downloaded + last_synthetic_downloaded,
                                total_bytes: grand_total,
                                child_path: Some(progress_child_key.clone()),
                                child_filename: Some(if current_stage.is_empty() {
                                    item_name.clone()
                                } else {
                                    current_stage.clone()
                                }),
                                child_bytes_downloaded: Some(last_synthetic_downloaded),
                                child_total_bytes: Some(item_total),
                                child_speed_bps: Some(speed),
                                child_eta_secs: Some(eta),
                            })
                            .await;
                    }
                }

                let status = child.wait().await?;
                let stderr = stderr_task.await.unwrap_or_default();
                if !status.success() {
                    let stderr = stderr.trim().to_string();
                    return Err(anyhow!(
                        "{}",
                        if stderr.is_empty() { "yt-dlp falhou ao baixar o video" } else { &stderr }
                    ));
                }
                let completed_synthetic = last_synthetic_downloaded
                    .max(Self::phase_progress(item_downloaded.max(item_total), phase_count, has_split_media))
                    .max(SYNTHETIC_PROGRESS_TOTAL);
                grand_downloaded = grand_downloaded.saturating_add(completed_synthetic);
                grand_total = grand_total.max(grand_downloaded);
                let final_size = Self::output_size(dest_path, started_at).await;
                if final_size > 0 {
                    grand_downloaded = final_size;
                }
                Self::send_stage(
                    &progress_tx,
                    &progress_child_key,
                    "Concluído",
                    SYNTHETIC_PROGRESS_TOTAL,
                    SYNTHETIC_PROGRESS_TOTAL,
                )
                .await;
            }

            Ok(grand_downloaded)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{YouTubeProvider, SYNTHETIC_PROGRESS_TOTAL};

    #[test]
    fn matches_youtube_short_and_watch_urls() {
        assert!(YouTubeProvider::matches("https://youtu.be/xF8l17MJkMk?si=abc"));
        assert!(YouTubeProvider::matches("https://www.youtube.com/watch?v=xF8l17MJkMk"));
        assert!(YouTubeProvider::matches("https://music.youtube.com/watch?v=xF8l17MJkMk"));
        assert!(!YouTubeProvider::matches("https://example.com/watch?v=xF8l17MJkMk"));
    }

    #[test]
    fn parses_ytdlp_progress_with_real_total() {
        let (downloaded, total, speed, eta) =
            YouTubeProvider::parse_progress("GDLPROG  42.5% 1.5MiB/s 00:09").unwrap();
        assert_eq!(downloaded, 4250);
        assert_eq!(total, SYNTHETIC_PROGRESS_TOTAL);
        assert_eq!(speed, 1_572_864);
        assert_eq!(eta, 9);
    }

    #[test]
    fn parses_ytdlp_progress_with_synthetic_total_when_total_is_na() {
        let (downloaded, total, _, _) =
            YouTubeProvider::parse_progress("GDLPROG  25.0% N/A 00:00").unwrap();
        assert_eq!(downloaded, SYNTHETIC_PROGRESS_TOTAL / 4);
        assert_eq!(total, SYNTHETIC_PROGRESS_TOTAL);
    }

    #[test]
    fn split_media_phase_progress_does_not_overlap_ranges() {
        assert_eq!(YouTubeProvider::phase_progress(SYNTHETIC_PROGRESS_TOTAL, 1, true), 4_500);
        assert_eq!(YouTubeProvider::phase_progress(0, 2, true), 4_500);
        assert_eq!(YouTubeProvider::phase_progress(SYNTHETIC_PROGRESS_TOTAL, 2, true), 9_000);
        assert_eq!(YouTubeProvider::merge_progress(true), 9_250);
    }

    #[test]
    fn reads_selected_merge_format_from_child_fragment() {
        let children = Some(vec![
            "https://www.youtube.com/watch?v=x#ytdlp_format=bestvideo%2Bbestaudio&ytdlp_merge_format=mkv".to_string(),
        ]);
        assert_eq!(
            YouTubeProvider::selected_value(&children, "ytdlp_merge_format").as_deref(),
            Some("mkv")
        );
        assert_eq!(YouTubeProvider::normalize_merge_format("MKV").as_deref(), Some("mkv"));
        assert_eq!(YouTubeProvider::normalize_merge_format("avi"), None);
    }
}
