use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;
use tokio::time::{sleep, Duration};

use crate::models::FileInfo;

use super::{
    apply_speed_limit, captcha_required_error, extract_fragment_value, host_matches, parse_human_size,
    path_segments, premium_required_error, rate_limit_error, removed_error, ProgressUpdate, Provider, ProviderDefaults,
};

pub struct MoonDLProvider;

impl MoonDLProvider {
    pub fn matches(url: &str) -> bool {
        host_matches(url, &["moondl.com", "www.moondl.com"])
            && matches!(path_segments(url).as_slice(), [code, ..] if Self::looks_like_code(code))
    }

    fn looks_like_code(value: &str) -> bool {
        value.len() >= 8 && value.chars().all(|ch| ch.is_ascii_alphanumeric())
    }

    fn file_code(url: &str) -> Option<String> {
        match path_segments(url).as_slice() {
            [code, ..] if Self::looks_like_code(code) => Some(code.to_string()),
            _ => None,
        }
    }

    fn decode_html(value: &str) -> String {
        value.replace("&gt;", ">")
            .replace("&lt;", "<")
            .replace("&amp;", "&")
            .replace("&quot;", "\"")
            .replace("&#039;", "'")
            .replace("&#133;", "...")
            .trim()
            .to_string()
    }

    fn parse_hidden_inputs(html: &str) -> Vec<(String, String)> {
        let Some(re) = regex::Regex::new(
            r#"(?is)<input[^>]*name=["']([^"']+)["'][^>]*value=["']([^"']*)["'][^>]*>"#,
        )
        .ok() else {
            return Vec::new();
        };

        re.captures_iter(html)
            .map(|captures| {
                (
                    captures[1].trim().to_string(),
                    Self::decode_html(captures[2].trim()),
                )
            })
            .collect()
    }

    fn hidden_input(html: &str, name: &str) -> Option<String> {
        Self::parse_hidden_inputs(html)
            .into_iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value)
    }

    fn extract_filename(html: &str) -> Option<String> {
        if let Some(fname) = Self::hidden_input(html, "fname") {
            if !fname.is_empty() {
                return Some(fname);
            }
        }

        regex::Regex::new(r#"(?is)<title>\s*Download\s+(.+?)\s*-\s*MoonDL\s*</title>"#)
            .ok()
            .and_then(|re| re.captures(html))
            .map(|captures| Self::decode_html(&captures[1]))
            .filter(|value| !value.is_empty())
    }

    fn extract_size(html: &str) -> u64 {
        let patterns = [
            r#"(?is)File size[^0-9]{0,40}([0-9]+(?:[.,][0-9]+)?\s*(?:KB|MB|GB|TB))"#,
            r#"(?is)\(([0-9]+(?:[.,][0-9]+)?\s*(?:KB|MB|GB|TB))\)"#,
        ];

        for pattern in patterns {
            if let Some(value) = regex::Regex::new(pattern)
                .ok()
                .and_then(|re| re.captures(html))
                .map(|captures| Self::decode_html(&captures[1]))
            {
                let parsed = parse_human_size(&value);
                if parsed > 0 {
                    return parsed;
                }
            }
        }

        0
    }

    fn extract_error(html: &str) -> Option<String> {
        let re = regex::Regex::new(r#"(?is)<div class="err">\s*(.*?)\s*</div>"#).ok()?;
        let raw = re.captures(html)?.get(1)?.as_str();
        let cleaned = regex::Regex::new(r#"(?is)<br\s*/?>"#)
            .ok()
            .map(|re| re.replace_all(raw, " ").to_string())
            .unwrap_or_else(|| raw.to_string());
        let text = regex::Regex::new(r#"(?is)<[^>]+>"#)
            .ok()
            .map(|re| re.replace_all(&cleaned, "").to_string())
            .unwrap_or(cleaned);
        let message = Self::decode_html(&text);
        if message.is_empty() {
            None
        } else {
            Some(message)
        }
    }

    fn is_premium_required_message(message: &str) -> bool {
        let lower = message.to_ascii_lowercase();
        lower.contains("upgrade your account")
            || lower.contains("premium")
            || lower.contains("1000 mb only")
            || lower.contains("download files up to 1000 mb only")
    }

    fn extract_wait_seconds(html: &str) -> Option<u64> {
        let patterns = [
            r#"class=["']seconds["'][^>]*>\s*(\d+)\s*<"#,
            r#"var\s+countdown\s*=\s*(\d+)"#,
            r#"var\s+seconds\s*=\s*(\d+)"#,
            r#"estimated_time\s*=\s*(\d+)"#,
        ];

        for pattern in patterns {
            if let Some(value) = regex::Regex::new(pattern)
                .ok()
                .and_then(|re| re.captures(html))
                .and_then(|captures| captures[1].parse::<u64>().ok())
            {
                return Some(value);
            }
        }

        if html.contains("countdown('#countdown .seconds')") {
            return Some(60);
        }

        None
    }

    fn detect_recaptcha_sitekey(html: &str) -> Option<String> {
        regex::Regex::new(r#"g-recaptcha[^>]+data-sitekey=["']([^"']+)["']"#)
            .ok()?
            .captures(html)
            .map(|captures| captures[1].to_string())
    }

    fn extract_direct_download_url(html: &str) -> Option<String> {
        let patterns = [
            r#"(?is)href=["'](https?://[^"']+)["'][^>]*id=["']downloadbtn"#,
            r#"(?is)href=["'](https?://[^"']+)["'][^>]*class=["'][^"']*downloadbtn"#,
            r#"(?is)window\.location\s*=\s*["'](https?://[^"']+)["']"#,
            r#"(?is)document\.location\s*=\s*["'](https?://[^"']+)["']"#,
        ];

        patterns
            .iter()
            .find_map(|pattern| {
                regex::Regex::new(pattern)
                    .ok()
                    .and_then(|re| re.captures(html))
                    .map(|captures| captures[1].to_string())
            })
    }

    fn extract_captcha_token(url: &str) -> Option<String> {
        extract_fragment_value(url, "captcha_token")
    }

    fn build_download1_payload(html: &str, code: &str, referer: &str) -> Vec<(String, String)> {
        let mut payload = vec![
            ("op".to_string(), "download1".to_string()),
            ("usr_login".to_string(), Self::hidden_input(html, "usr_login").unwrap_or_default()),
            ("id".to_string(), code.to_string()),
            (
                "fname".to_string(),
                Self::hidden_input(html, "fname").unwrap_or_else(|| "arquivo_moondl".to_string()),
            ),
            ("referer".to_string(), referer.to_string()),
            ("method_free".to_string(), "1".to_string()),
        ];
        payload.retain(|(_, value)| !value.is_empty());
        payload
    }

    fn build_download2_payload(html: &str, referer: &str, captcha_token: Option<&str>) -> Vec<(String, String)> {
        let mut payload = Self::parse_hidden_inputs(html)
            .into_iter()
            .filter(|(name, _)| matches!(
                name.as_str(),
                "op" | "id" | "rand" | "referer" | "method_free" | "method_premium" | "usr_login" | "fname"
            ))
            .collect::<Vec<_>>();

        if !payload.iter().any(|(name, _)| name == "referer") {
            payload.push(("referer".to_string(), referer.to_string()));
        }
        if let Some(token) = captcha_token {
            payload.push(("g-recaptcha-response".to_string(), token.to_string()));
            payload.push(("h-captcha-response".to_string(), token.to_string()));
        }
        payload
    }

    fn is_binary_download_response(response: &reqwest::Response) -> bool {
        if response.headers().get(reqwest::header::CONTENT_DISPOSITION).is_some() {
            return true;
        }

        response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(|content_type| {
                let lower = content_type.to_ascii_lowercase();
                !lower.starts_with("text/html")
                    && !lower.starts_with("text/plain")
                    && !lower.starts_with("application/json")
            })
            .unwrap_or(false)
    }

    fn is_removed_page(html: &str) -> bool {
        let lower = html.to_ascii_lowercase();
        lower.contains("file was deleted")
            || lower.contains("file not found")
            || lower.contains("404 not found")
    }

    async fn stream_response_to_file(
        response: reqwest::Response,
        dest_path: &str,
        expected_size: u64,
        speed_limit_bps: Option<u64>,
        progress_tx: tokio::sync::mpsc::Sender<ProgressUpdate>,
    ) -> Result<u64> {
        let total = response.content_length().unwrap_or(expected_size);
        let mut file = tokio::fs::File::create(dest_path).await?;
        let mut stream = response.bytes_stream();
        let mut downloaded = 0u64;
        let mut session_downloaded = 0u64;
        let started_at = tokio::time::Instant::now();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            file.write_all(&chunk).await?;
            let len = chunk.len() as u64;
            downloaded += len;
            session_downloaded += len;

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
            apply_speed_limit(started_at, session_downloaded, speed_limit_bps).await;
        }

        file.flush().await?;
        Ok(downloaded)
    }
}

impl ProviderDefaults for MoonDLProvider {}

impl Provider for MoonDLProvider {
    fn name(&self) -> &str { "MoonDL" }

    fn get_file_info<'a>(
        &'a self,
        url: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FileInfo>> + Send + 'a>> {
        Box::pin(async move {
            let client = <Self as ProviderDefaults>::http_client()?;
            let html = client.get(url).send().await?.error_for_status()?.text().await?;

            if Self::is_removed_page(&html) {
                return Err(removed_error("MoonDL"));
            }

            let filename = Self::extract_filename(&html).unwrap_or_else(|| "arquivo_moondl".to_string());
            let size = Self::extract_size(&html);

            Ok(FileInfo {
                filename: <Self as ProviderDefaults>::safe_filename(&filename, "arquivo_moondl"),
                size,
                mime_type: None,
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
        speed_limit_bps: Option<u64>,
        _parallel_parts: usize,
        _selected_children: Option<Vec<String>>,
        progress_tx: tokio::sync::mpsc::Sender<ProgressUpdate>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u64>> + Send + 'a>> {
        Box::pin(async move {
            let code = Self::file_code(url).ok_or_else(|| anyhow!("URL do MoonDL inválida"))?;
            let clean_url = url.split('#').next().unwrap_or(url);
            let captcha_token = Self::extract_captcha_token(url);
            let client = <Self as ProviderDefaults>::http_client()?;

            let initial_page = client.get(clean_url).send().await?.error_for_status()?.text().await?;
            if Self::is_removed_page(&initial_page) {
                return Err(removed_error("MoonDL"));
            }

            let expected_size = Self::extract_size(&initial_page);
            let download1_payload = Self::build_download1_payload(&initial_page, &code, clean_url);
            let download1_html = client
                .post(clean_url)
                .header("Referer", clean_url)
                .form(&download1_payload)
                .send()
                .await?
                .error_for_status()?
                .text()
                .await?;

            if let Some(message) = Self::extract_error(&download1_html) {
                let lower = message.to_ascii_lowercase();
                if Self::is_premium_required_message(&message) {
                    return Err(premium_required_error("MoonDL", &message));
                }
                if lower.contains("wait") || lower.contains("limit") {
                    return Err(rate_limit_error(3600, message));
                }
            }

            if let Some(wait) = Self::extract_wait_seconds(&download1_html) {
                if wait > 300 {
                    return Err(rate_limit_error(wait, "MoonDL ainda está contando o tempo do modo gratuito"));
                }
                sleep(Duration::from_secs(wait.saturating_add(1))).await;
            }

            if let Some(sitekey) = Self::detect_recaptcha_sitekey(&download1_html) {
                let captcha_page = clean_url.to_string();
                let token = captcha_token.as_deref();
                if token.is_none() {
                    return Err(captcha_required_error("recaptcha2", &sitekey, &captcha_page));
                }
            }

            let download2_payload = Self::build_download2_payload(&download1_html, clean_url, captcha_token.as_deref());
            let response = client
                .post(clean_url)
                .header("Referer", clean_url)
                .form(&download2_payload)
                .send()
                .await?;

            if Self::is_binary_download_response(&response) {
                let response = response.error_for_status()?;
                return Self::stream_response_to_file(
                    response,
                    dest_path,
                    expected_size,
                    speed_limit_bps,
                    progress_tx,
                )
                .await;
            }

            let html = response.error_for_status()?.text().await?;
            if let Some(message) = Self::extract_error(&html) {
                if Self::is_premium_required_message(&message) {
                    return Err(premium_required_error("MoonDL", &message));
                }
            }
            if let Some(wait) = Self::extract_wait_seconds(&html) {
                return Err(rate_limit_error(wait, "MoonDL ainda não liberou o download gratuito"));
            }
            if let Some(sitekey) = Self::detect_recaptcha_sitekey(&html) {
                return Err(captcha_required_error("recaptcha2", &sitekey, clean_url));
            }
            if let Some(direct_url) = Self::extract_direct_download_url(&html) {
                let resp = client
                    .get(&direct_url)
                    .header("Referer", clean_url)
                    .send()
                    .await?
                    .error_for_status()?;
                return Self::stream_response_to_file(
                    resp,
                    dest_path,
                    expected_size,
                    speed_limit_bps,
                    progress_tx,
                )
                .await;
            }

            Err(anyhow!("MoonDL não expôs um link final de download"))
        })
    }
}
