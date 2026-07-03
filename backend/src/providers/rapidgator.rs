use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use serde_json::Value;
use tokio::io::AsyncWriteExt;

use crate::models::FileInfo;
use super::{
    apply_speed_limit, captcha_required_error, extract_fragment_value, host_matches, parse_human_size,
    path_segments, premium_required_error, rate_limit_error, removed_error, ProgressUpdate, Provider, ProviderDefaults,
};

pub struct RapidgatorProvider;

impl RapidgatorProvider {
    pub fn matches(url: &str) -> bool {
        host_matches(url, &["rapidgator.net", "www.rapidgator.net"])
            && matches!(path_segments(url).as_slice(), [first, _id, ..] if first == "file")
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
        extract_fragment_value(url, "captcha_token")
    }

    /// Detecta o sitekey do reCaptcha v2 na página.
    fn detect_recaptcha_sitekey(html: &str) -> Option<String> {
        let re = regex::Regex::new(r#"g-recaptcha[^>]+data-sitekey=["']([^"']+)["']"#).ok()?;
        re.captures(html).map(|captures| captures[1].to_string())
    }

    fn detect_hcaptcha_sitekey(html: &str) -> Option<String> {
        let re = regex::Regex::new(r#"h-captcha[^>]+data-sitekey=["']([^"']+)["']"#).ok()?;
        re.captures(html).map(|captures| captures[1].to_string())
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
        parse_human_size(s)
    }

    fn extract_size(html: &str) -> u64 {
        let lower = html.to_lowercase();
        if let Some(pos) = lower.find("file size:") {
            let slice = &html[pos..html.len().min(pos + 300)];
            let size_re = regex::Regex::new(r#"([0-9]+(?:[.,][0-9]+)?)\s*(KB|MB|GB|TB)"#).ok();
            if let Some(captures) = size_re.and_then(|re| re.captures(slice)) {
                return Self::parse_human_size(&format!("{} {}", &captures[1], &captures[2]));
            }
        }

        for unit in &["kb", "mb", "gb", "tb"] {
            if let Some(pos) = lower.find(unit) {
                let before = &html[..pos].trim_end();
                let start = before
                    .rfind(|c: char| !c.is_ascii_digit() && c != '.' && c != ',')
                    .map(|p| p + 1)
                    .unwrap_or(0);
                let num_str = &before[start..];
                if !num_str.is_empty() {
                    return Self::parse_human_size(&format!("{} {}", num_str, unit.to_uppercase()));
                }
            }
        }

        0
    }

    fn is_removed_page(html: &str) -> bool {
        let lower = html.to_lowercase();
        lower.contains("file not found")
            || lower.contains("file was removed")
            || lower.contains("download file not found")
            || lower.contains("error 404")
    }

    fn extract_numeric_fid(html: &str) -> Option<String> {
        let re = regex::Regex::new(r#"var\s+fid\s*=\s*(\d+)"#).ok()?;
        re.captures(html).map(|captures| captures[1].to_string())
    }

    fn extract_js_string_var(html: &str, name: &str) -> Option<String> {
        let pattern = format!(r#"var\s+{}\s*=\s*'([^']*)'"#, regex::escape(name));
        let re = regex::Regex::new(&pattern).ok()?;
        re.captures(html).map(|captures| captures[1].to_string())
    }

    fn extract_js_number_var(html: &str, name: &str) -> Option<u64> {
        let pattern = format!(r#"var\s+{}\s*=\s*(\d+)"#, regex::escape(name));
        let re = regex::Regex::new(&pattern).ok()?;
        re.captures(html)
            .and_then(|captures| captures.get(1))
            .and_then(|value| value.as_str().parse::<u64>().ok())
    }

    fn absolute_url(path_or_url: &str) -> String {
        if path_or_url.starts_with("http://") || path_or_url.starts_with("https://") {
            return path_or_url.to_string();
        }

        format!("https://rapidgator.net{}", path_or_url)
    }

    fn is_free_limit_block(html: &str, size: u64) -> bool {
        size > 1024 * 1024 * 1024
            && html.to_lowercase().contains("download files up to 1 gb in free mode")
    }

    fn extract_ready_download_link(html: &str) -> Option<String> {
        let from_var = regex::Regex::new(r#"download_link\s*=\s*'([^']+)'"#)
            .ok()
            .and_then(|re| re.captures(html))
            .map(|captures| captures[1].to_string());

        from_var.or_else(|| {
            let re = regex::Regex::new(r#"href=["'](https?://[^"']+)["'][^>]*class=["'][^"']*btn-download"#).ok()?;
            re.captures(html).map(|captures| captures[1].to_string())
        })
    }

    async fn resolve_captcha_page(
        client: &reqwest::Client,
        captcha_page_url: &str,
        referer: &str,
        captcha_token: Option<&str>,
    ) -> Result<Option<String>> {
        if let Some(token) = captcha_token {
            let response = client
                .post(captcha_page_url)
                .header("Referer", referer)
                .form(&[
                    ("g-recaptcha-response", token),
                    ("h-captcha-response", token),
                    ("captcha_token", token),
                ])
                .send()
                .await?;

            let final_url = response.url().to_string();
            let html = response.error_for_status()?.text().await?;

            if !final_url.contains("/download/captcha") {
                return Ok(Some(final_url));
            }

            if let Some(direct_url) = Self::extract_ready_download_link(&html) {
                return Ok(Some(direct_url));
            }
        }

        let captcha_html = client
            .get(captcha_page_url)
            .header("Referer", referer)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;

        if let Some(sitekey) = Self::detect_recaptcha_sitekey(&captcha_html) {
            return Err(captcha_required_error("recaptcha2", &sitekey, captcha_page_url));
        }

        if let Some(sitekey) = Self::detect_hcaptcha_sitekey(&captcha_html) {
            return Err(captcha_required_error("hcaptcha", &sitekey, captcha_page_url));
        }

        Ok(Self::extract_ready_download_link(&captcha_html))
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

            if Self::is_removed_page(&resp) {
                return Err(removed_error("Rapidgator"));
            }

            // Rate limit check
            if let Some(secs) = Self::parse_wait_time(&resp) {
                return Err(rate_limit_error(secs, format!("Rapidgator: aguarde {} hora(s)", secs / 3600)));
            }

            // Captcha check
            if let Some(sitekey) = Self::detect_recaptcha_sitekey(&resp) {
                return Err(captcha_required_error(
                    "recaptcha2",
                    &sitekey,
                    url.split('#').next().unwrap_or(url),
                ));
            }
            if let Some(sitekey) = Self::detect_hcaptcha_sitekey(&resp) {
                return Err(captcha_required_error(
                    "hcaptcha",
                    &sitekey,
                    url.split('#').next().unwrap_or(url),
                ));
            }

            let filename = Self::extract_filename(&resp)
                .unwrap_or_else(|| "arquivo_rapidgator".to_string());
            let size = Self::extract_size(&resp);

            Ok(FileInfo {
                filename: <Self as ProviderDefaults>::safe_filename(&filename, "arquivo_rapidgator"),
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
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u64>> + Send + 'a>>
    {
        Box::pin(async move {
            let captcha_token = Self::extract_captcha_token(url);
            let clean_url = url.split('#').next().unwrap_or(url);

            Self::file_id(clean_url)
                .ok_or_else(|| anyhow!("URL do Rapidgator inválida"))?;

            let client = <Self as ProviderDefaults>::http_client()?;
            let page = client.get(clean_url).send().await?.error_for_status()?.text().await?;

            if Self::is_removed_page(&page) {
                return Err(removed_error("Rapidgator"));
            }

            // Rate limit
            if let Some(secs) = Self::parse_wait_time(&page) {
                return Err(rate_limit_error(secs, format!("Rapidgator: aguarde {} hora(s)", secs / 3600)));
            }

            if captcha_token.is_none() && Self::detect_recaptcha_sitekey(&page).is_some() {
                let sitekey = Self::detect_recaptcha_sitekey(&page).unwrap_or_default();
                return Err(captcha_required_error("recaptcha2", &sitekey, clean_url));
            }
            if captcha_token.is_none() && Self::detect_hcaptcha_sitekey(&page).is_some() {
                let sitekey = Self::detect_hcaptcha_sitekey(&page).unwrap_or_default();
                return Err(captcha_required_error("hcaptcha", &sitekey, clean_url));
            }

            let size = Self::extract_size(&page);
            let numeric_fid = Self::extract_numeric_fid(&page)
                .ok_or_else(|| anyhow!("Rapidgator não expôs o identificador interno deste arquivo"))?;
            let start_timer_url = Self::extract_js_string_var(&page, "startTimerUrl").unwrap_or_default();
            let get_download_url = Self::extract_js_string_var(&page, "getDownloadUrl")
                .unwrap_or_else(|| "/download/AjaxGetDownloadLink".to_string());
            let captcha_page_url = Self::extract_js_string_var(&page, "captchaUrl").unwrap_or_default();
            let page_wait_secs = Self::extract_js_number_var(&page, "secs").unwrap_or(0);

            if start_timer_url.is_empty() {
                if Self::is_free_limit_block(&page, size) {
                    return Err(premium_required_error(
                        "Rapidgator",
                        "o modo grátis atual só libera até 1 GB",
                    ));
                }

                if let Some(direct_url) = Self::extract_ready_download_link(&page) {
                    let resp = client.get(&direct_url).send().await?.error_for_status()?;
                    let total = resp.content_length().unwrap_or(size);
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
                    return Ok(downloaded);
                }

                return Err(anyhow!(
                    "Rapidgator não liberou um fluxo gratuito para este arquivo. O host pode exigir premium, captcha adicional ou outro espelho."
                ));
            }

            let start_resp = client
                .get(Self::absolute_url(&start_timer_url))
                .header("X-Requested-With", "XMLHttpRequest")
                .header("Referer", clean_url)
                .query(&[("fid", numeric_fid.as_str())])
                .send()
                .await?
                .json::<Value>()
                .await?;
            let start_json = start_resp;

            // Check for rate limit in JSON
            if let Some(delay) = start_json["delay"].as_i64() {
                if delay > 0 {
                    return Err(rate_limit_error(delay as u64, format!("Rapidgator: aguarde {}s", delay)));
                }
            }

            if start_json["state"].as_str() == Some("error") {
                let code = start_json["code"].as_str().unwrap_or("erro desconhecido");
                return Err(anyhow!("Rapidgator recusou o início do download: {code}"));
            }

            let wait = start_json["secs"]
                .as_u64()
                .or_else(|| start_json["wait"].as_u64())
                .or_else(|| start_json["delay"].as_u64())
                .unwrap_or(page_wait_secs);
            if wait > 0 && wait <= 180 {
                tokio::time::sleep(tokio::time::Duration::from_secs(wait)).await;
            } else if wait > 180 {
                return Err(rate_limit_error(wait, format!("Rapidgator: aguarde {}s", wait)));
            }

            let sid = start_json["sid"].as_str().unwrap_or("").to_string();
            if sid.is_empty() {
                return Err(premium_required_error(
                    "Rapidgator",
                    "o host não retornou SID de download no modo gratuito",
                ));
            }

            let dl_resp = client
                .get(Self::absolute_url(&get_download_url))
                .header("X-Requested-With", "XMLHttpRequest")
                .header("Referer", clean_url)
                .query(&[("sid", sid.as_str())])
                .send()
                .await?
                .json::<Value>()
                .await?;
            let dl_json = dl_resp;

            if dl_json["state"].as_str() == Some("error") {
                let code = dl_json["code"].as_str().unwrap_or("erro desconhecido");
                if code.to_lowercase().contains("captcha") && !captcha_page_url.is_empty() {
                    let resolved = Self::resolve_captcha_page(
                        &client,
                        &Self::absolute_url(&captcha_page_url),
                        clean_url,
                        captcha_token.as_deref(),
                    ).await?;
                    if let Some(direct_url) = resolved {
                        let resp = client.get(&direct_url).send().await?.error_for_status()?;
                        let total = resp.content_length().unwrap_or(size);
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
                        return Ok(downloaded);
                    }
                }

                return Err(premium_required_error(
                    "Rapidgator",
                    &format!("o host não liberou o link final ({code})"),
                ));
            }

            let direct_url = dl_json["download_link"]
                .as_str()
                .or_else(|| dl_json["url"].as_str())
                .map(str::to_string)
                .or_else(|| {
                    if captcha_page_url.is_empty() {
                        None
                    } else {
                        Some(Self::absolute_url(&captcha_page_url))
                    }
                })
                .ok_or_else(|| premium_required_error("Rapidgator", "o host não retornou URL de download"))?;

            let direct_url = if direct_url.contains("/download/captcha") {
                Self::resolve_captcha_page(&client, &direct_url, clean_url, captcha_token.as_deref())
                    .await?
                    .ok_or_else(|| premium_required_error("Rapidgator", "o host ainda não liberou o link após o captcha"))?
            } else {
                direct_url
            };

            // Download file
            let resp = client.get(&direct_url).send().await?.error_for_status()?;
            let total = resp.content_length().unwrap_or(size);
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

#[cfg(test)]
#[path = "tests/rapidgator.rs"]
mod tests;
