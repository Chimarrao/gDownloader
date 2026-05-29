use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;

use crate::models::FileInfo;

use super::{
    apply_speed_limit, captcha_required_error, extract_fragment_value, host_matches,
    parse_human_size, premium_required_error, ProgressUpdate, Provider, ProviderDefaults,
};

pub struct BruploadProvider;

impl BruploadProvider {
    pub fn matches(url: &str) -> bool {
        host_matches(url, &["brupload.net", "www.brupload.net"]) && Self::file_code(url).is_some()
    }

    fn file_code(url: &str) -> Option<String> {
        let parsed = reqwest::Url::parse(url).ok()?;
        let segments = parsed.path_segments()?;
        let clean = segments
            .filter(|segment| !segment.is_empty())
            .collect::<Vec<_>>();

        match clean.as_slice() {
            [code] if Self::looks_like_code(code) => Some((*code).to_string()),
            ["d", code] if Self::looks_like_code(code) => Some((*code).to_string()),
            _ => None,
        }
    }

    fn looks_like_code(value: &str) -> bool {
        value.len() >= 8 && value.chars().all(|ch| ch.is_ascii_alphanumeric())
    }

    fn decode_html(value: &str) -> String {
        value
            .replace("&gt;", ">")
            .replace("&lt;", "<")
            .replace("&amp;", "&")
            .replace("&quot;", "\"")
            .replace("&#039;", "'")
            .replace("&#39;", "'")
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

        regex::Regex::new(r#"(?is)<title>\s*Download\s+([^<]+?)\s*</title>"#)
            .ok()
            .and_then(|re| re.captures(html))
            .map(|captures| Self::decode_html(&captures[1]))
            .filter(|title| !title.is_empty())
    }

    fn extract_size(html: &str) -> u64 {
        let patterns = [
            r#"(?is)<span class="statd">\s*(?:tamanho|size)\s*</span>\s*<span>\s*([^<]+)\s*</span>"#,
            r#"(?is)(?:tamanho|size)\s*:\s*</[^>]+>\s*<[^>]+>\s*([^<]+)\s*<"#,
            r#"(?is)(?:tamanho|size)\s*:\s*([0-9][0-9.,]*\s*[KMGT]?B)"#,
        ];

        patterns
            .iter()
            .find_map(|pattern| {
                regex::Regex::new(pattern)
                    .ok()
                    .and_then(|re| re.captures(html))
                    .map(|captures| parse_human_size(&Self::decode_html(&captures[1])))
                    .filter(|size| *size > 0)
            })
            .unwrap_or(0)
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
        if message.is_empty() { None } else { Some(message) }
    }

    fn extract_wait_seconds(html: &str) -> Option<u64> {
        [
            r#"class="seconds">\s*(\d+)\s*<"#,
            r#"var\s+estimated_time\s*=\s*(\d+)"#,
            r#"var\s+seconds\s*=\s*(\d+)"#,
        ]
        .iter()
        .find_map(|pattern| {
            regex::Regex::new(pattern)
                .ok()
                .and_then(|re| re.captures(html))
                .and_then(|captures| captures[1].parse::<u64>().ok())
        })
    }

    fn detect_recaptcha_sitekey(html: &str) -> Option<String> {
        let re = regex::Regex::new(r#"g-recaptcha[^>]+data-sitekey=["']([^"']+)["']"#).ok()?;
        re.captures(html).map(|captures| captures[1].to_string())
    }

    fn detect_hcaptcha_sitekey(html: &str) -> Option<String> {
        let re = regex::Regex::new(r#"h-captcha[^>]+data-sitekey=["']([^"']+)["']"#).ok()?;
        re.captures(html).map(|captures| captures[1].to_string())
    }

    fn extract_captcha_token(url: &str) -> Option<String> {
        extract_fragment_value(url, "captcha_token")
    }

    fn extract_direct_download_url(html: &str) -> Option<String> {
        [
            r#"(?is)href=["'](https?://[^"']+)["'][^>]*class=["'][^"']*downloadbtn"#,
            r#"(?is)href=["'](https?://[^"']+)["'][^>]*id=["']downloadbtn"#,
            r#"(?is)window\.location\s*=\s*["'](https?://[^"']+)["']"#,
            r#"(?is)document\.location\s*=\s*["'](https?://[^"']+)["']"#,
        ]
        .iter()
        .find_map(|pattern| {
            regex::Regex::new(pattern)
                .ok()
                .and_then(|re| re.captures(html))
                .map(|captures| captures[1].to_string())
        })
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

    fn build_download1_payload(html: &str, code: &str, referer: &str) -> Vec<(String, String)> {
        let mut payload = vec![
            ("op".to_string(), "download1".to_string()),
            ("usr_login".to_string(), Self::hidden_input(html, "usr_login").unwrap_or_default()),
            ("id".to_string(), code.to_string()),
            (
                "fname".to_string(),
                Self::hidden_input(html, "fname").unwrap_or_else(|| "arquivo_brupload".to_string()),
            ),
            ("referer".to_string(), referer.to_string()),
            (
                "method_free".to_string(),
                Self::hidden_input(html, "method_free")
                    .unwrap_or_else(|| "Download Gratuito >>".to_string()),
            ),
        ];
        payload.retain(|(_, value)| !value.is_empty());
        payload
    }

    fn build_download2_payload(
        html: &str,
        referer: &str,
        captcha_token: Option<&str>,
    ) -> Vec<(String, String)> {
        let mut payload = Self::parse_hidden_inputs(html)
            .into_iter()
            .filter(|(name, _)| {
                matches!(
                    name.as_str(),
                    "op" | "id" | "rand" | "referer" | "method_free" | "method_premium" | "usr_login" | "fname"
                )
            })
            .collect::<Vec<_>>();

        if !payload.iter().any(|(name, _)| name == "referer") {
            payload.push(("referer".to_string(), referer.to_string()));
        }
        if !payload.iter().any(|(name, _)| name == "adblock_detected") {
            payload.push(("adblock_detected".to_string(), "0".to_string()));
        }
        if let Some(token) = captcha_token {
            payload.push(("g-recaptcha-response".to_string(), token.to_string()));
            payload.push(("h-captcha-response".to_string(), token.to_string()));
        }
        payload
    }
}

impl ProviderDefaults for BruploadProvider {}

impl Provider for BruploadProvider {
    fn name(&self) -> &str { "BRUpload" }

    fn get_file_info<'a>(
        &'a self,
        url: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FileInfo>> + Send + 'a>> {
        Box::pin(async move {
            let client = <Self as ProviderDefaults>::http_client()?;
            let code = Self::file_code(url).ok_or_else(|| anyhow!("URL do BRUpload inválida"))?;
            let html = client.get(url).send().await?.error_for_status()?.text().await?;
            let mut filename = Self::extract_filename(&html)
                .unwrap_or_else(|| "arquivo_brupload".to_string());
            let mut size = Self::extract_size(&html);

            if size == 0 {
                let download1_payload = Self::build_download1_payload(&html, &code, url);
                if let Ok(response) = client
                    .post(url)
                    .header("Referer", url)
                    .form(&download1_payload)
                    .send()
                    .await
                {
                    if let Ok(response) = response.error_for_status() {
                        if let Ok(download1_html) = response.text().await {
                            if filename == "arquivo_brupload" {
                                if let Some(parsed_name) = Self::extract_filename(&download1_html) {
                                    filename = parsed_name;
                                }
                            }
                            size = size.max(Self::extract_size(&download1_html));
                        }
                    }
                }
            }

            Ok(FileInfo {
                filename: <Self as ProviderDefaults>::safe_filename(&filename, "arquivo_brupload"),
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
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u64>> + Send + 'a>> {
        Box::pin(async move {
            let client = <Self as ProviderDefaults>::http_client()?;
            let code = Self::file_code(url).ok_or_else(|| anyhow!("URL do BRUpload inválida"))?;
            let captcha_token = Self::extract_captcha_token(url);

            let initial_page = client.get(url).send().await?.error_for_status()?.text().await?;
            let expected_size = Self::extract_size(&initial_page);
            let download1_payload = Self::build_download1_payload(&initial_page, &code, url);
            let download1_html = client
                .post(url)
                .header("Referer", url)
                .form(&download1_payload)
                .send()
                .await?
                .error_for_status()?
                .text()
                .await?;

            if let Some(message) = Self::extract_error(&download1_html) {
                return Err(anyhow!("BRUpload bloqueou o download gratuito: {message}"));
            }

            if captcha_token.is_none() {
                if let Some(sitekey) = Self::detect_recaptcha_sitekey(&download1_html) {
                    return Err(captcha_required_error("recaptcha2", &sitekey, url));
                }
                if let Some(sitekey) = Self::detect_hcaptcha_sitekey(&download1_html) {
                    return Err(captcha_required_error("hcaptcha", &sitekey, url));
                }
            }

            if let Some(wait) = Self::extract_wait_seconds(&download1_html) {
                tokio::time::sleep(tokio::time::Duration::from_secs(wait.min(180).saturating_add(1))).await;
            }

            let download2_payload =
                Self::build_download2_payload(&download1_html, url, captcha_token.as_deref());
            let response = client
                .post(url)
                .header("Referer", url)
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

            let download2_html = response.error_for_status()?.text().await?;

            if let Some(message) = Self::extract_error(&download2_html) {
                return Err(anyhow!("BRUpload rejeitou o download: {message}"));
            }

            if captcha_token.is_none() {
                if let Some(sitekey) = Self::detect_recaptcha_sitekey(&download2_html) {
                    return Err(captcha_required_error("recaptcha2", &sitekey, url));
                }
                if let Some(sitekey) = Self::detect_hcaptcha_sitekey(&download2_html) {
                    return Err(captcha_required_error("hcaptcha", &sitekey, url));
                }
            }

            let direct_url = Self::extract_direct_download_url(&download2_html)
                .ok_or_else(|| premium_required_error(
                    "BRUpload",
                    "o host pode exigir premium, captcha adicional ou outro limite",
                ))?;

            let resp = client.get(&direct_url).send().await?.error_for_status()?;
            Self::stream_response_to_file(resp, dest_path, expected_size, speed_limit_bps, progress_tx).await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::BruploadProvider;

    #[test]
    fn matches_file_page() {
        assert!(BruploadProvider::matches("https://www.brupload.net/1ppok7ga1hfm"));
        assert!(BruploadProvider::matches("https://www.brupload.net/d/1ppok7ga1hfm"));
        assert!(!BruploadProvider::matches("https://www.brupload.net/login.html"));
    }

    #[test]
    fn extracts_filename_from_current_page() {
        let html = r#"
            <title>Download Familia Soprano S01E04 1080p Mini HMAX WEB DD2 264 DUAL Dinho mkv</title>
            <input type="hidden" name="fname" value="Familia.Soprano.S01E04.1080p.mkv">
        "#;
        assert_eq!(
            BruploadProvider::extract_filename(html).as_deref(),
            Some("Familia.Soprano.S01E04.1080p.mkv")
        );
    }

    #[test]
    fn detects_captcha_sitekeys_and_wait_time() {
        let html = r#"
            <div class="g-recaptcha" data-sitekey="sitekey-recaptcha"></div>
            <div class="h-captcha" data-sitekey="sitekey-hcaptcha"></div>
            <script>var seconds = 15;</script>
        "#;

        assert_eq!(BruploadProvider::detect_recaptcha_sitekey(html).as_deref(), Some("sitekey-recaptcha"));
        assert_eq!(BruploadProvider::detect_hcaptcha_sitekey(html).as_deref(), Some("sitekey-hcaptcha"));
        assert_eq!(BruploadProvider::extract_wait_seconds(html), Some(15));
    }
}
