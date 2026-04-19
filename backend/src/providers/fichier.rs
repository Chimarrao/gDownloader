use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;
use tokio::time::Duration;

use crate::models::FileInfo;
use super::{apply_speed_limit, ProgressUpdate, Provider, ProviderDefaults};

pub struct FichierProvider;

impl FichierProvider {
    pub fn matches(url: &str) -> bool {
        url.contains("1fichier.com/")
    }

    fn extract_download_page(url: &str) -> Option<String> {
        if !Self::matches(url) {
            return None;
        }

        let normalized = url.trim();
        if normalized.is_empty() {
            return None;
        }

        Some(normalized.to_string())
    }

    fn extract_between(haystack: &str, start: &str, end: &str) -> Option<String> {
        let start_idx = haystack.find(start)? + start.len();
        let rest = &haystack[start_idx..];
        let end_idx = rest.find(end)?;
        Some(rest[..end_idx].trim().to_string())
    }

    fn decode_basic_html_entities(value: &str) -> String {
        value
            .replace("&amp;", "&")
            .replace("&quot;", "\"")
            .replace("&#039;", "'")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&nbsp;", " ")
    }

    fn extract_filename_and_size(html: &str, fallback_name: &str) -> (String, u64) {
        let filename = Self::extract_between(
            html,
            "<span style=\"font-weight:bold\">",
            "</span>",
        )
        .map(|s| Self::decode_basic_html_entities(&s))
        .filter(|s| !s.is_empty())
        .or_else(|| {
            Self::extract_between(html, "<title>", "</title>")
                .map(|s| s.replace(" - 1fichier.com", "").trim().to_string())
        })
        .unwrap_or_else(|| fallback_name.to_string());

        let human_size = Self::extract_between(
            html,
            "<span style=\"font-size:0.9em;font-style:italic\">",
            "</span>",
        )
        .unwrap_or_default();

        (filename, Self::parse_human_size(&human_size))
    }

    fn parse_human_size(value: &str) -> u64 {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return 0;
        }

        let mut parts = trimmed.split_whitespace();
        let number = parts
            .next()
            .map(|raw| raw.replace(',', "."))
            .and_then(|raw| raw.parse::<f64>().ok())
            .unwrap_or(0.0);
        let unit = parts.next().unwrap_or("").to_ascii_uppercase();

        let multiplier = match unit.as_str() {
            "B" => 1f64,
            "KB" => 1024f64,
            "MB" => 1024f64.powi(2),
            "GB" => 1024f64.powi(3),
            "TB" => 1024f64.powi(4),
            _ => 1f64,
        };

        (number * multiplier).round() as u64
    }

    fn extract_wait_seconds(html: &str) -> Option<u64> {
        let marker = "var ct = ";
        let start = html.find(marker)? + marker.len();
        let rest = &html[start..];
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        digits.parse::<u64>().ok()
    }

    fn has_free_slot_error(html: &str) -> bool {
        html.contains("All free guest slots are currently in use")
            || html.contains("Sign in instantly to continue your download")
    }

    fn extract_direct_link(html: &str) -> Option<String> {
        let mut cursor = html;
        while let Some(pos) = cursor.find("href=\"") {
            let rest = &cursor[pos + 6..];
            let end = rest.find('"')?;
            let href = &rest[..end];
            let lower = href.to_ascii_lowercase();
            if href.starts_with("http")
                && !lower.contains("/login")
                && !lower.contains("/register")
                && !lower.contains("/hlp")
                && !lower.contains("/tarifs")
                && !lower.contains("/cgu")
            {
                return Some(href.to_string());
            }
            cursor = &rest[end + 1..];
        }

        None
    }

    fn is_binary_response(resp: &reqwest::Response) -> bool {
        resp.headers()
            .get("content-disposition")
            .is_some()
            || resp
                .headers()
                .get("content-type")
                .and_then(|value| value.to_str().ok())
                .map(|value| !value.to_ascii_lowercase().contains("text/html"))
                .unwrap_or(false)
    }

    fn free_slot_error() -> anyhow::Error {
        anyhow!("1fichier sem slot gratuito disponível no momento. O host exige aguardar ou entrar com conta.")
    }
}

impl ProviderDefaults for FichierProvider {}

impl Provider for FichierProvider {
    fn name(&self) -> &str { "1Fichier" }

    fn get_file_info<'a>(&'a self, url: &'a str)
        -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FileInfo>> + Send + 'a>>
    {
        Box::pin(async move {
            let page_url = Self::extract_download_page(url)
                .ok_or_else(|| anyhow!("URL do 1fichier inválida: {url}"))?;
            let client = <Self as ProviderDefaults>::http_client()?;
            let html = client.get(&page_url).send().await?.error_for_status()?.text().await?;
            let fallback = "arquivo_1fichier";
            let (filename, size) = Self::extract_filename_and_size(&html, fallback);

            Ok(FileInfo {
                filename: <Self as ProviderDefaults>::safe_filename(&filename, fallback),
                size,
                mime_type: None,
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
        _parallel_parts: usize,
        _selected_children: Option<Vec<String>>,
        progress_tx: tokio::sync::mpsc::Sender<ProgressUpdate>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u64>> + Send + 'a>>
    {
        Box::pin(async move {
            let page_url = Self::extract_download_page(url)
                .ok_or_else(|| anyhow!("URL do 1fichier inválida: {url}"))?;
            let client = <Self as ProviderDefaults>::http_client()?;
            let landing = client.get(&page_url).send().await?.error_for_status()?.text().await?;

            if Self::has_free_slot_error(&landing) {
                return Err(Self::free_slot_error());
            }

            let wait_seconds = Self::extract_wait_seconds(&landing).unwrap_or(0);
            if wait_seconds > 0 {
                tokio::time::sleep(Duration::from_secs(wait_seconds.min(90))).await;
            }

            let response = client
                .post(&page_url)
                .form(&[("dl_no_ssl", "on")])
                .send()
                .await?
                .error_for_status()?;

            if Self::is_binary_response(&response) {
                let total = response.content_length().unwrap_or(0);
                let mut file = tokio::fs::File::create(dest_path).await?;
                let mut stream = response.bytes_stream();
                let mut downloaded = 0u64;
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
                return Ok(downloaded);
            }

            let html = response.text().await?;
            if Self::has_free_slot_error(&html) {
                return Err(Self::free_slot_error());
            }

            let direct_url = Self::extract_direct_link(&html)
                .ok_or_else(|| anyhow!("1fichier exigiu uma etapa adicional que ainda não foi resolvida automaticamente"))?;

            let resp = client.get(&direct_url).send().await?.error_for_status()?;
            let total = resp.content_length().unwrap_or(0);
            let mut file = tokio::fs::File::create(dest_path).await?;
            let mut stream = resp.bytes_stream();
            let mut downloaded = 0u64;
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

#[cfg(test)]
mod tests {
    use super::FichierProvider;

    #[test]
    fn parses_wait_seconds() {
        let html = r#"
            <script>
                var ct = 60;
            </script>
        "#;
        assert_eq!(FichierProvider::extract_wait_seconds(html), Some(60));
    }

    #[test]
    fn parses_filename_and_size() {
        let html = r#"
            <span style="font-weight:bold">Outcome.2026.mp4</span>
            <span style="font-size:0.9em;font-style:italic">1.66 GB</span>
        "#;

        let (filename, size) = FichierProvider::extract_filename_and_size(html, "fallback.bin");
        assert_eq!(filename, "Outcome.2026.mp4");
        assert!(size > 1_700_000_000 && size < 1_900_000_000);
    }

    #[test]
    fn detects_free_slot_error() {
        let html = "All free guest slots are currently in use.";
        assert!(FichierProvider::has_free_slot_error(html));
    }
}
