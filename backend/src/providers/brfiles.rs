use anyhow::{anyhow, Result};
use futures_util::{stream, StreamExt};
use tokio::io::AsyncWriteExt;

use crate::models::{FileChildInfo, FileInfo};

use super::{
    apply_speed_limit, host_matches, parse_human_size, path_segments, rate_limit_error, ProgressUpdate,
    Provider, ProviderDefaults,
};

#[derive(Debug, Clone)]
struct BrfilesFolderEntry {
    filename: String,
    path: String,
    size: u64,
    source_url: String,
}

pub struct BrfilesProvider;

impl BrfilesProvider {
    pub fn matches(url: &str) -> bool {
        if !host_matches(url, &["brfiles.com", "www.brfiles.com"]) {
            return false;
        }

        matches!(path_segments(url).as_slice(), [first, _code, ..] if first == "f" || first == "d")
    }

    fn is_folder_url(url: &str) -> bool {
        matches!(path_segments(url).as_slice(), [first, _code, ..] if first == "d")
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

    fn parse_human_size(value: &str) -> u64 {
        parse_human_size(value)
    }

    fn strip_html(value: &str) -> String {
        let without_breaks = regex::Regex::new(r#"(?is)<br\s*/?>"#)
            .ok()
            .map(|re| re.replace_all(value, " ").to_string())
            .unwrap_or_else(|| value.to_string());
        let without_tags = regex::Regex::new(r#"(?is)<[^>]+>"#)
            .ok()
            .map(|re| re.replace_all(&without_breaks, " ").to_string())
            .unwrap_or(without_breaks);
        Self::decode_html(&without_tags.replace('\n', " "))
    }

    fn collect_cookie_header(headers: &reqwest::header::HeaderMap) -> Option<String> {
        let mut pairs = Vec::new();
        for value in headers.get_all(reqwest::header::SET_COOKIE).iter() {
            let Ok(raw) = value.to_str() else {
                continue;
            };
            if let Some(first) = raw.split(';').next() {
                let trimmed = first.trim();
                if !trimmed.is_empty() {
                    pairs.push(trimmed.to_string());
                }
            }
        }

        if pairs.is_empty() {
            None
        } else {
            Some(pairs.join("; "))
        }
    }

    fn merge_cookie_headers(left: Option<String>, right: Option<String>) -> Option<String> {
        let mut cookies = std::collections::BTreeMap::<String, String>::new();

        for source in [left, right] {
            let Some(source) = source else {
                continue;
            };

            for pair in source.split(';') {
                let trimmed = pair.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let Some((name, value)) = trimmed.split_once('=') else {
                    continue;
                };
                cookies.insert(name.trim().to_string(), value.trim().to_string());
            }
        }

        if cookies.is_empty() {
            None
        } else {
            Some(
                cookies
                    .into_iter()
                    .map(|(name, value)| format!("{name}={value}"))
                    .collect::<Vec<_>>()
                    .join("; "),
            )
        }
    }

    fn extract_filename(html: &str) -> Option<String> {
        let from_title = regex::Regex::new(r#"(?is)<title>\s*([^<]+?)\s*-\s*BRFiles\s*</title>"#)
            .ok()
            .and_then(|re| re.captures(html))
            .map(|captures| Self::decode_html(&captures[1]));

        from_title.and_then(|value| {
            let clean = value.trim();
            if clean.is_empty() {
                None
            } else {
                Some(clean.to_string())
            }
        })
    }

    fn extract_size(html: &str) -> u64 {
        let value = regex::Regex::new(
            r#"(?is)<p[^>]*class=["'][^"']*tamanho-arquivo[^"']*["'][^>]*>\s*([^<]+)\s*</p>"#,
        )
        .ok()
        .and_then(|re| re.captures(html))
        .map(|captures| Self::decode_html(&captures[1]))
        .unwrap_or_default();

        Self::parse_human_size(&value)
    }

    fn extract_wait_seconds(html: &str) -> Option<u64> {
        regex::Regex::new(r#"var\s+seconds\s*=\s*(\d+)"#)
            .ok()?
            .captures(html)
            .and_then(|captures| captures[1].parse::<u64>().ok())
    }

    fn extract_pt_url(html: &str) -> Option<String> {
        regex::Regex::new(r#"href=['"](https?://(?:www\.)?brfiles\.com[^'"]+\?pt=[^'"]+)['"]"#)
            .ok()?
            .captures(html)
            .map(|captures| captures[1].to_string())
    }

    fn extract_free_button_url(html: &str) -> Option<String> {
        regex::Regex::new(r#"(?is)<a[^>]*class=['"][^'"]*btn-free[^'"]*['"][^>]*href=['"]([^'"]+)['"]"#)
            .ok()
            .and_then(|re| re.captures(html))
            .map(|captures| captures[1].to_string())
    }

    fn extract_folder_name(html: &str) -> Option<String> {
        let from_heading = regex::Regex::new(r#"(?is)<h2[^>]*>\s*Arquivos contidos na pasta\s*([^<]+)\s*</h2>"#)
            .ok()
            .and_then(|re| re.captures(html))
            .map(|captures| Self::decode_html(&captures[1]));

        let from_title = regex::Regex::new(r#"(?is)<title>\s*Pasta de Arquivos\s*-\s*BRFiles\s*</title>"#)
            .ok()
            .and_then(|re| re.captures(html))
            .and(from_heading.clone());

        from_heading.or(from_title).and_then(|value| {
            let clean = value.trim();
            if clean.is_empty() {
                None
            } else {
                Some(clean.to_string())
            }
        })
    }

    fn extract_folder_entries(html: &str) -> Vec<BrfilesFolderEntry> {
        let Some(re) = regex::Regex::new(
            r#"(?is)<a[^>]*class=["'][^"']*responsiveInfoTable[^"']*["'][^>]*href=["'](https?://(?:www\.)?brfiles\.com/f/[^"']+)["'][^>]*>\s*([^<]+)\s*</a>"#,
        )
        .ok() else {
            return Vec::new();
        };

        re.captures_iter(html)
            .filter_map(|captures| {
                let source_url = captures[1].trim().to_string();
                let filename = <Self as ProviderDefaults>::safe_filename(
                    &Self::decode_html(&captures[2]),
                    "arquivo_brfiles",
                );

                if source_url.is_empty() || filename.is_empty() {
                    return None;
                }

                Some(BrfilesFolderEntry {
                    path: filename.clone(),
                    filename,
                    size: 0,
                    source_url,
                })
            })
            .collect()
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

    fn extract_rate_limit_seconds(html: &str) -> Option<u64> {
        let text = Self::strip_html(html).to_lowercase();
        let relevant = text.contains("aguarde")
            || text.contains("limite")
            || text.contains("download simult")
            || text.contains("outro download")
            || text.contains("ip");
        if !relevant {
            return None;
        }

        let mut total = 0u64;
        let mut matched = false;

        for captures in regex::Regex::new(r#"(\d+)\s*hora"#)
            .ok()?
            .captures_iter(&text)
        {
            total += captures[1].parse::<u64>().unwrap_or(0) * 3600;
            matched = true;
        }
        for captures in regex::Regex::new(r#"(\d+)\s*minuto"#)
            .ok()?
            .captures_iter(&text)
        {
            total += captures[1].parse::<u64>().unwrap_or(0) * 60;
            matched = true;
        }
        for captures in regex::Regex::new(r#"(\d+)\s*segundo"#)
            .ok()?
            .captures_iter(&text)
        {
            total += captures[1].parse::<u64>().unwrap_or(0);
            matched = true;
        }

        if matched && total > 0 {
            Some(total)
        } else {
            None
        }
    }

    fn extract_rate_limit_message(html: &str) -> Option<String> {
        let text = Self::strip_html(html);
        let lower = text.to_lowercase();
        if lower.contains("limite")
            || lower.contains("aguarde")
            || lower.contains("download simult")
            || lower.contains("outro download")
            || lower.contains("ip")
        {
            Some(text)
        } else {
            None
        }
    }

    async fn fetch_file_metadata(
        client: &reqwest::Client,
        url: &str,
        fallback_name: &str,
    ) -> Result<(String, u64)> {
        let html = client.get(url).send().await?.error_for_status()?.text().await?;
        let filename = Self::extract_filename(&html).unwrap_or_else(|| fallback_name.to_string());
        let size = Self::extract_size(&html);
        Ok((filename, size))
    }

    async fn resolve_folder_entries(
        client: &reqwest::Client,
        html: &str,
    ) -> Result<Vec<BrfilesFolderEntry>> {
        let entries = Self::extract_folder_entries(html);
        if entries.is_empty() {
            return Ok(Vec::new());
        }

        let enriched = stream::iter(entries.into_iter().map(|entry| {
            let client = client.clone();
            async move {
                let mut next = entry;
                if let Ok((filename, size)) = Self::fetch_file_metadata(&client, &next.source_url, &next.filename).await {
                    next.filename = <Self as ProviderDefaults>::safe_filename(&filename, &next.filename);
                    next.path = next.filename.clone();
                    next.size = size;
                }
                next
            }
        }))
        .buffer_unordered(4)
        .collect::<Vec<_>>()
        .await;

        Ok(enriched)
    }

    async fn download_single_file(
        client: &reqwest::Client,
        url: &str,
        dest_path: &str,
        expected_size: u64,
        speed_limit_bps: Option<u64>,
        progress_tx: tokio::sync::mpsc::Sender<ProgressUpdate>,
    ) -> Result<u64> {
        let initial_response = client.get(url).send().await?.error_for_status()?;
        let mut cookies = Self::collect_cookie_header(initial_response.headers());
        let initial_html = initial_response.text().await?;

        let expected_size = expected_size.max(Self::extract_size(&initial_html));
        let mut current_url = Self::extract_pt_url(&initial_html)
            .ok_or_else(|| anyhow!("BRFiles não expôs o link gratuito desta página"))?;
        let mut wait_secs = Self::extract_wait_seconds(&initial_html)
            .or_else(|| Self::extract_rate_limit_seconds(&initial_html))
            .unwrap_or(45)
            .min(6 * 3600);

        for _attempt in 0..6 {
            if wait_secs > 0 {
                tokio::time::sleep(tokio::time::Duration::from_secs(wait_secs.saturating_add(1))).await;
            }

            let mut request = client.get(&current_url).header("Referer", url);
            if let Some(cookie) = cookies.clone() {
                request = request.header(reqwest::header::COOKIE, cookie);
            }

            let response = request.send().await?;
            cookies = Self::merge_cookie_headers(cookies, Self::collect_cookie_header(response.headers()));

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

            if let Some(next_url) = Self::extract_free_button_url(&html) {
                current_url = next_url;
            }

            if let Some(next_wait) = Self::extract_wait_seconds(&html) {
                wait_secs = next_wait.min(6 * 3600);
                continue;
            }

            if let Some(secs) = Self::extract_rate_limit_seconds(&html) {
                let message = Self::extract_rate_limit_message(&html)
                    .unwrap_or_else(|| "BRFiles limitou temporariamente este IP.".to_string());
                return Err(rate_limit_error(secs, message));
            }
        }

        Err(rate_limit_error(
            wait_secs.max(60),
            "BRFiles ainda não liberou o arquivo gratuito. O host pode estar impondo espera por IP ou bloqueio por download simultâneo; o app vai revalidar automaticamente no próximo ciclo.",
        ))
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

impl ProviderDefaults for BrfilesProvider {}

impl Provider for BrfilesProvider {
    fn name(&self) -> &str { "BRFiles" }

    fn get_file_info<'a>(
        &'a self,
        url: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FileInfo>> + Send + 'a>> {
        Box::pin(async move {
            let client = <Self as ProviderDefaults>::http_client()?;
            let html = client.get(url).send().await?.error_for_status()?.text().await?;

            if Self::is_folder_url(url) {
                let folder_name = Self::extract_folder_name(&html)
                    .unwrap_or_else(|| "pasta_brfiles".to_string());
                let children = Self::resolve_folder_entries(&client, &html).await?;
                let total_size = children.iter().map(|child| child.size).sum();

                return Ok(FileInfo {
                    filename: <Self as ProviderDefaults>::safe_filename(&folder_name, "pasta_brfiles"),
                    size: total_size,
                    mime_type: None,
                    is_folder: true,
                    children: Some(
                        children
                            .into_iter()
                            .map(|child| FileChildInfo {
                                filename: child.filename,
                                size: child.size,
                                mime_type: None,
                                is_folder: false,
                                path: Some(child.path),
                                source_url: Some(child.source_url),
                                bytes_downloaded: None,
                                speed_bps: None,
                                eta_secs: None,
                                status: None,
                            })
                            .collect(),
                    ),
                    ..Default::default()
                });
            }

            let filename = Self::extract_filename(&html)
                .unwrap_or_else(|| "arquivo_brfiles".to_string());
            let size = Self::extract_size(&html);

            Ok(FileInfo {
                filename: <Self as ProviderDefaults>::safe_filename(&filename, "arquivo_brfiles"),
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
        selected_children: Option<Vec<String>>,
        progress_tx: tokio::sync::mpsc::Sender<ProgressUpdate>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u64>> + Send + 'a>> {
        Box::pin(async move {
            let client = <Self as ProviderDefaults>::http_client()?;

            if Self::is_folder_url(url) {
                let html = client.get(url).send().await?.error_for_status()?.text().await?;
                let mut children = Self::resolve_folder_entries(&client, &html).await?;

                if let Some(selected) = selected_children {
                    let selected_set = selected.into_iter().collect::<std::collections::HashSet<_>>();
                    children.retain(|child| selected_set.contains(&child.source_url));
                }

                if children.is_empty() {
                    return Err(anyhow!("Pasta do BRFiles vazia ou sem arquivos acessíveis"));
                }

                tokio::fs::create_dir_all(dest_path).await?;

                let total_size: u64 = children.iter().map(|child| child.size).sum();
                let mut downloaded_total = 0u64;

                for child in &children {
                    let output_path = format!("{}/{}", dest_path.trim_end_matches('/'), child.path);
                    if let Some(parent_dir) = std::path::Path::new(&output_path).parent() {
                        tokio::fs::create_dir_all(parent_dir).await?;
                    }

                    if let Ok(metadata) = tokio::fs::metadata(&output_path).await {
                        let existing_len = metadata.len();
                        if child.size > 0 && existing_len >= child.size {
                            downloaded_total = downloaded_total.saturating_add(child.size);
                            let _ = progress_tx
                                .send(ProgressUpdate {
                                    bytes_downloaded: downloaded_total,
                                    total_bytes: total_size,
                                    child_path: Some(child.path.clone()),
                                    child_filename: Some(child.filename.clone()),
                                    child_bytes_downloaded: Some(child.size),
                                    child_total_bytes: Some(child.size),
                                    child_speed_bps: Some(0),
                                    child_eta_secs: Some(0),
                                })
                                .await;
                            continue;
                        }

                        if existing_len > 0 {
                            let _ = tokio::fs::remove_file(&output_path).await;
                        }
                    }

                    let child_filename = child.filename.clone();
                    let child_path = child.path.clone();
                    let child_size = child.size;
                    let base_downloaded = downloaded_total;
                    let child_started_at = tokio::time::Instant::now();
                    let (child_tx, mut child_rx) = tokio::sync::mpsc::channel::<ProgressUpdate>(64);
                    let child_url = child.source_url.clone();
                    let child_dest_path = output_path.clone();
                    let child_client = client.clone();
                    let child_speed_limit = speed_limit_bps;

                    let child_task = tokio::spawn(async move {
                        Self::download_single_file(
                            &child_client,
                            &child_url,
                            &child_dest_path,
                            child_size,
                            child_speed_limit,
                            child_tx,
                        )
                        .await
                    });

                    while let Some(update) = child_rx.recv().await {
                        let child_downloaded = update.bytes_downloaded;
                        let total_downloaded = base_downloaded + child_downloaded;
                        let child_elapsed = child_started_at.elapsed().as_secs_f64();
                        let child_speed = if child_elapsed > 0.0 {
                            (child_downloaded as f64 / child_elapsed) as u64
                        } else {
                            0
                        };
                        let child_eta = if child_speed > 0 && child_size > child_downloaded {
                            (child_size - child_downloaded) / child_speed
                        } else {
                            0
                        };

                        let _ = progress_tx
                            .send(ProgressUpdate {
                                bytes_downloaded: total_downloaded,
                                total_bytes: total_size,
                                child_path: Some(child_path.clone()),
                                child_filename: Some(child_filename.clone()),
                                child_bytes_downloaded: Some(child_downloaded),
                                child_total_bytes: Some(child_size),
                                child_speed_bps: Some(child_speed),
                                child_eta_secs: Some(child_eta),
                            })
                            .await;
                    }

                    let child_bytes = child_task
                        .await
                        .map_err(|error| anyhow!("Falha interna ao baixar item do BRFiles: {error}"))??;
                    downloaded_total = base_downloaded + child_bytes;
                }

                return Ok(downloaded_total);
            }

            Self::download_single_file(
                &client,
                url,
                dest_path,
                0,
                speed_limit_bps,
                progress_tx,
            )
            .await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::BrfilesProvider;

    #[test]
    fn matches_file_and_folder_pages() {
        assert!(BrfilesProvider::matches(
            "https://brfiles.com/f/MoQFnG5r/Hannibal.S02e01.1080P.WEB-DL.DUAL.DUBLASERIES.TV.mkv"
        ));
        assert!(BrfilesProvider::matches("https://brfiles.com/d/kAZST1Am/"));
        assert!(!BrfilesProvider::matches("https://brfiles.com/login"));
    }

    #[test]
    fn extracts_file_metadata_and_wait_link() {
        let html = r#"
            <title>Hannibal.S02e01.1080P.WEB-DL.DUAL.DUBLASERIES.TV.mkv - BRFiles</title>
            <p class="tamanho-arquivo">995 MB</p>
            <script>
              var seconds = 30;
              html += "<a class='btn btn-free' href='https://brfiles.com/f/MoQFnG5r/Hannibal.S02e01.1080P.WEB-DL.DUAL.DUBLASERIES.TV.mkv?pt=4yo2KoxY7BfXNm7e7Ld5nA%3D%3D'>Clique para baixar</a>";
            </script>
        "#;

        assert_eq!(
            BrfilesProvider::extract_filename(html).as_deref(),
            Some("Hannibal.S02e01.1080P.WEB-DL.DUAL.DUBLASERIES.TV.mkv")
        );
        assert_eq!(BrfilesProvider::extract_size(html), 995 * 1024 * 1024);
        assert_eq!(BrfilesProvider::extract_wait_seconds(html), Some(30));
        assert_eq!(
            BrfilesProvider::extract_pt_url(html).as_deref(),
            Some("https://brfiles.com/f/MoQFnG5r/Hannibal.S02e01.1080P.WEB-DL.DUAL.DUBLASERIES.TV.mkv?pt=4yo2KoxY7BfXNm7e7Ld5nA%3D%3D")
        );
    }

    #[test]
    fn extracts_folder_entries() {
        let html = include_str!("../../tests/fixtures/providers/brfiles_folder.html");

        let entries = BrfilesProvider::extract_folder_entries(html);
        assert_eq!(BrfilesProvider::extract_folder_name(html).as_deref(), Some("Hannibal S01"));
        assert_eq!(entries.len(), 2);
        assert_eq!(
            entries[0].source_url,
            "https://brfiles.com/f/HniTPKsv/Hannibal.S01e01.1080P.WEB-DL.DUAL.DUBLASERIES.TV.mkv"
        );
        assert_eq!(
            entries[0].filename,
            "Hannibal.S01e01.1080P.WEB-DL.DUAL.DUBLASERIES.TV.mkv"
        );
        assert_eq!(
            entries[1].filename,
            "Hannibal.S01e02.1080P.WEB-DL.DUAL.DUBLASERIES.TV.mkv"
        );
    }

    #[test]
    fn extracts_rate_limit_seconds_from_message() {
        let html = "<div class='warning'>Seu IP já possui outro download ativo. Aguarde 2 horas 15 minutos e 9 segundos.</div>";
        assert_eq!(BrfilesProvider::extract_rate_limit_seconds(html), Some(8109));
    }

    #[test]
    fn falls_back_to_longer_cooldown_when_free_slot_is_not_ready() {
        let html = "<div class='warning'>Seu IP já possui outro download ativo. Aguarde 3 horas.</div>";
        assert_eq!(BrfilesProvider::extract_rate_limit_seconds(html), Some(10800));
    }
}
