use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;

use crate::models::FileInfo;
use super::{apply_speed_limit, ProgressUpdate, Provider, ProviderDefaults};

pub struct RapidgatorProvider;

impl RapidgatorProvider {
    pub fn matches(url: &str) -> bool {
        url.contains("rapidgator.net/file/")
    }

    /// Extrai o ID do arquivo da URL.
    fn file_id(url: &str) -> Option<String> {
        // Strip any captcha fragment first
        let clean = url.split('#').next().unwrap_or(url);
        let re_result = regex::Regex::new(r"rapidgator\.net/file/([a-f0-9]+)")
            .ok()?
            .captures(clean)?;
        Some(re_result[1].to_string())
    }

    /// Extrai captcha_token embutido na URL como fragmento: #captcha_token=XXX
    fn extract_captcha_token(url: &str) -> Option<String> {
        let fragment = url.split('#').nth(1)?;
        for part in fragment.split('&') {
            if let Some(val) = part.strip_prefix("captcha_token=") {
                return Some(val.to_string());
            }
        }
        None
    }

    /// Detecta o sitekey do reCaptcha v2 na página.
    fn detect_recaptcha_sitekey(html: &str) -> Option<String> {
        // <div class="g-recaptcha" data-sitekey="XXXX">
        let marker = "data-sitekey=\"";
        let start = html.find(marker)? + marker.len();
        let rest = &html[start..];
        let end = rest.find('"')?;
        Some(rest[..end].to_string())
    }

    /// Parseia tempo de espera do HTML do Rapidgator.
    fn parse_wait_time(html: &str) -> Option<u64> {
        let lower = html.to_lowercase();
        // "Please wait X hours" / "Try again in X hours"
        for phrase in &["please wait ", "try again in ", "wait "] {
            if let Some(pos) = lower.find(phrase) {
                let rest = &lower[pos + phrase.len()..];
                let n: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                if let Ok(n) = n.parse::<u64>() {
                    let after = rest[n.to_string().len()..].trim_start();
                    if after.starts_with("hour") { return Some(n * 3600); }
                    if after.starts_with("minute") { return Some(n * 60); }
                    if after.starts_with("second") { return Some(n); }
                }
            }
        }
        // JSON: "delay":3600
        if let Some(pos) = html.find("\"delay\":") {
            let rest = &html[pos + 8..];
            let n: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(n) = n.parse::<u64>() {
                if n > 0 { return Some(n); }
            }
        }
        None
    }

    fn extract_filename(html: &str) -> Option<String> {
        // <title>Download file FileName.ext</title>
        let lower = html.to_lowercase();
        if let Some(pos) = lower.find("<title>") {
            let rest = &html[pos + 7..];
            if let Some(end) = rest.find("</title>") {
                let title = &rest[..end];
                // Remove "Download file " prefix
                let cleaned = title
                    .trim_start_matches("Download file ")
                    .trim_start_matches("Download ")
                    .trim();
                if !cleaned.is_empty() && !cleaned.to_lowercase().contains("rapidgator") {
                    return Some(cleaned.to_string());
                }
            }
        }
        // h1 or file-info
        for tag in &["<h1>", "<h1 "] {
            if let Some(pos) = html.find(tag) {
                let rest = &html[pos..];
                if let Some(start) = rest.find('>') {
                    let rest2 = &rest[start + 1..];
                    if let Some(end) = rest2.find("</h1>") {
                        let name = rest2[..end].trim().to_string();
                        if !name.is_empty() && !name.to_lowercase().contains("rapidgator") {
                            return Some(name);
                        }
                    }
                }
            }
        }
        None
    }

    fn parse_human_size(s: &str) -> u64 {
        let s = s.trim();
        let mut parts = s.split_whitespace();
        let n: f64 = parts
            .next()
            .map(|raw| raw.replace(',', "."))
            .and_then(|raw| raw.parse().ok())
            .unwrap_or(0.0);
        let unit = parts.next().unwrap_or("").to_ascii_uppercase();
        let mult = match unit.as_str() {
            "KB" => 1024.0,
            "MB" => 1024.0 * 1024.0,
            "GB" => 1024.0 * 1024.0 * 1024.0,
            _ => 1.0,
        };
        (n * mult).round() as u64
    }
}

impl ProviderDefaults for RapidgatorProvider {}

impl Provider for RapidgatorProvider {
    fn name(&self) -> &str { "Rapidgator" }

    fn get_file_info<'a>(&'a self, url: &'a str)
        -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FileInfo>> + Send + 'a>>
    {
        Box::pin(async move {
            let _file_id = Self::file_id(url)
                .ok_or_else(|| anyhow!("URL do Rapidgator inválida"))?;

            let client = <Self as ProviderDefaults>::http_client()?;
            let resp = client.get(url).send().await?.error_for_status()?.text().await?;

            // Rate limit check
            if let Some(secs) = Self::parse_wait_time(&resp) {
                return Err(anyhow!("RATE_LIMIT:{}:Rapidgator: aguarde {} hora(s)", secs, secs / 3600));
            }

            // Captcha check
            if let Some(sitekey) = Self::detect_recaptcha_sitekey(&resp) {
                return Err(anyhow!(
                    "CAPTCHA_REQUIRED:recaptcha2:{}:{}",
                    sitekey,
                    url.split('#').next().unwrap_or(url)
                ));
            }

            let filename = Self::extract_filename(&resp)
                .unwrap_or_else(|| "arquivo_rapidgator".to_string());

            // Extract size from page
            let size = {
                let lower = resp.to_lowercase();
                let mut found = 0u64;
                for unit in &["kb", "mb", "gb"] {
                    if let Some(pos) = lower.find(unit) {
                        let before = &resp[..pos].trim_end();
                        let start = before.rfind(|c: char| !c.is_ascii_digit() && c != '.' && c != ',')
                            .map(|p| p + 1)
                            .unwrap_or(0);
                        let num_str = &before[start..];
                        if !num_str.is_empty() {
                            found = Self::parse_human_size(&format!("{} {}", num_str, unit.to_uppercase()));
                            break;
                        }
                    }
                }
                found
            };

            Ok(FileInfo {
                filename: <Self as ProviderDefaults>::safe_filename(&filename, "arquivo_rapidgator"),
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
            let captcha_token = Self::extract_captcha_token(url);
            let clean_url = url.split('#').next().unwrap_or(url);

            let file_id = Self::file_id(clean_url)
                .ok_or_else(|| anyhow!("URL do Rapidgator inválida"))?;

            let client = <Self as ProviderDefaults>::http_client()?;
            let page = client.get(clean_url).send().await?.error_for_status()?.text().await?;

            // Rate limit
            if let Some(secs) = Self::parse_wait_time(&page) {
                return Err(anyhow!("RATE_LIMIT:{}:Rapidgator: aguarde {} hora(s)", secs, secs / 3600));
            }

            // Captcha required
            if Self::detect_recaptcha_sitekey(&page).is_some() && captcha_token.is_none() {
                let sitekey = Self::detect_recaptcha_sitekey(&page).unwrap_or_default();
                return Err(anyhow!(
                    "CAPTCHA_REQUIRED:recaptcha2:{}:{}",
                    sitekey,
                    clean_url
                ));
            }

            // Start timer — POST to AjaxStartTimer
            let start_url = format!(
                "https://rapidgator.net/download/AjaxStartTimer/{}",
                file_id
            );
            let mut form = vec![
                ("DownloadMd5Form[code]".to_string(), file_id.clone()),
            ];
            if let Some(ref token) = captcha_token {
                form.push(("DownloadMd5Form[captcha]".to_string(), token.clone()));
            }

            let start_resp = client
                .post(&start_url)
                .header("X-Requested-With", "XMLHttpRequest")
                .header("Referer", clean_url)
                .form(&form)
                .send()
                .await?
                .text()
                .await?;

            let start_json: serde_json::Value = serde_json::from_str(&start_resp)
                .unwrap_or_default();

            // Check for rate limit in JSON
            if let Some(delay) = start_json["delay"].as_i64() {
                if delay > 0 {
                    return Err(anyhow!("RATE_LIMIT:{}:Rapidgator: aguarde {}s", delay, delay));
                }
            }

            // Check for next (wait before download)
            if let Some(wait) = start_json["next"].as_i64() {
                if wait > 0 && wait <= 120 {
                    tokio::time::sleep(tokio::time::Duration::from_secs(wait as u64)).await;
                } else if wait > 120 {
                    return Err(anyhow!("RATE_LIMIT:{}:Rapidgator: aguarde {}s", wait, wait));
                }
            }

            let sid = start_json["sid"].as_str().unwrap_or("").to_string();
            if sid.is_empty() {
                return Err(anyhow!("Rapidgator não retornou SID de download. O arquivo pode requerer conta premium."));
            }

            // Get download link
            let dl_url_api = format!(
                "https://rapidgator.net/download/AjaxGetDownloadLink?sid={}",
                sid
            );
            let dl_resp = client
                .get(&dl_url_api)
                .header("X-Requested-With", "XMLHttpRequest")
                .send()
                .await?
                .text()
                .await?;

            let dl_json: serde_json::Value = serde_json::from_str(&dl_resp)
                .unwrap_or_default();

            let direct_url = dl_json["url"].as_str()
                .ok_or_else(|| anyhow!("Rapidgator não retornou URL de download"))?
                .to_string();

            // Download file
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
                let len = chunk.len() as u64;
                downloaded += len;
                session_downloaded += len;

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
