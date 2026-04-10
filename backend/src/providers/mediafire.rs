use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use scraper::{Html, Selector};
use tokio::io::AsyncWriteExt;

use crate::models::FileInfo;
use super::{Provider, ProgressUpdate};

pub struct MediaFireProvider;

impl MediaFireProvider {
    pub fn matches(url: &str) -> bool {
        url.contains("mediafire.com")
    }

    // Extrai o link direto de download do HTML da página do MediaFire
    // O MediaFire renderiza uma página HTML com um botão que aponta para a URL real
    // É como web scraping — lemos o HTML e procuramos o link correto
    pub fn extract_direct_link(html: &str) -> Option<String> {
        let document = Html::parse_document(html);

        // Estratégia 1: elemento com id="downloadButton"
        // Selector::parse funciona como CSS selectors no JS (document.querySelector)
        if let Ok(selector) = Selector::parse("#downloadButton") {
            if let Some(el) = document.select(&selector).next() {
                if let Some(href) = el.value().attr("href") {
                    if href.starts_with("http") {
                        return Some(href.to_string());
                    }
                }
            }
        }

        // Estratégia 2: qualquer <a> apontando para download*.mediafire.com/get/ ou /download/
        if let Ok(selector) = Selector::parse("a[href]") {
            for el in document.select(&selector) {
                if let Some(href) = el.value().attr("href") {
                    if href.starts_with("http")
                        && href.contains(".mediafire.com")
                        && (href.contains("/get/") || href.contains("/download/"))
                    {
                        return Some(href.to_string());
                    }
                }
            }
        }

        None
    }

    // Cria cliente HTTP com User-Agent de browser — o MediaFire bloqueia bots sem UA
    fn http_client() -> Result<reqwest::Client> {
        Ok(reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()?)
    }
}

impl Provider for MediaFireProvider {
    fn name(&self) -> &str { "MediaFire" }

    fn get_file_info<'a>(&'a self, url: &'a str)
        -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FileInfo>> + Send + 'a>>
    {
        Box::pin(async move {
            let client = Self::http_client()?;

            // Passo 1: busca a página HTML do arquivo
            let html = client.get(url).send().await?.text().await?;

            // Passo 2: extrai o link direto do HTML
            let direct_url = Self::extract_direct_link(&html)
                .ok_or_else(|| anyhow!("Link de download não encontrado na página do MediaFire.\nVerifique se o link é público e está correto."))?;

            // Passo 3: HEAD request para obter metadados sem baixar o arquivo
            let resp = client.head(&direct_url).send().await?;

            // Tenta extrair o nome do arquivo do Content-Disposition
            // ou da última parte da URL como fallback
            let filename = resp
                .headers()
                .get("content-disposition")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| {
                    if let Some(pos) = s.find("filename=\"") {
                        let start = pos + 10;
                        let end = s[start..].find('"')? + start;
                        Some(s[start..end].to_string())
                    } else {
                        None
                    }
                })
                .or_else(|| {
                    // Fallback: usa o último segmento da URL original
                    url.split('/').last().map(String::from)
                })
                .unwrap_or_else(|| "arquivo_mediafire".to_string());

            Ok(FileInfo {
                filename,
                size: resp.content_length().unwrap_or(0),
                mime_type: resp
                    .headers()
                    .get("content-type")
                    .and_then(|v| v.to_str().ok())
                    .map(String::from),
            })
        })
    }

    fn download<'a>(
        &'a self,
        url: &'a str,
        dest_path: &'a str,
        progress_tx: tokio::sync::mpsc::Sender<ProgressUpdate>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u64>> + Send + 'a>>
    {
        Box::pin(async move {
            let client = Self::http_client()?;

            // Passo 1: busca a página HTML para extrair o link direto
            let html = client.get(url).send().await?.text().await?;
            let direct_url = Self::extract_direct_link(&html)
                .ok_or_else(|| anyhow!("Link de download não encontrado na página do MediaFire"))?;

            // Passo 2: baixa o arquivo da URL direta
            let resp = client.get(&direct_url).send().await?.error_for_status()?;

            let total = resp.content_length().unwrap_or(0);
            let mut file = tokio::fs::File::create(dest_path).await?;
            let mut stream = resp.bytes_stream();
            let mut downloaded: u64 = 0;

            while let Some(chunk) = stream.next().await {
                let chunk = chunk?;
                file.write_all(&chunk).await?;
                downloaded += chunk.len() as u64;
                let _ = progress_tx
                    .send(ProgressUpdate { bytes_downloaded: downloaded, total_bytes: total })
                    .await;
            }

            file.flush().await?;
            Ok(downloaded)
        })
    }
}

// --- Testes unitários ---
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matches_mediafire_url() {
        assert!(MediaFireProvider::matches(
            "https://www.mediafire.com/file/abc123/file.zip/file"
        ));
        assert!(!MediaFireProvider::matches("https://mega.nz/file/abc"));
    }

    #[test]
    fn test_extract_link_download_button() {
        let html = r#"<a id="downloadButton" href="https://download2390.mediafire.com/get/abc/file.zip">Download</a>"#;
        let link = MediaFireProvider::extract_direct_link(html);
        assert_eq!(
            link,
            Some("https://download2390.mediafire.com/get/abc/file.zip".to_string())
        );
    }

    #[test]
    fn test_extract_link_fallback_pattern() {
        let html = r#"<a href="https://download123.mediafire.com/get/xyz/photo.jpg">Baixar</a>"#;
        let link = MediaFireProvider::extract_direct_link(html);
        assert!(link.is_some());
        assert!(link.unwrap().contains("mediafire.com"));
    }

    #[test]
    fn test_extract_link_not_found() {
        let html = r#"<html><body><p>Nenhum link aqui</p></body></html>"#;
        let link = MediaFireProvider::extract_direct_link(html);
        assert!(link.is_none());
    }
}
