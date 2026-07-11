use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;

use crate::models::{FileChildInfo, FileInfo};
use super::{
    apply_speed_limit, host_matches, path_segments, rate_limit_error, Provider, ProgressUpdate,
    ProviderDefaults,
};

pub struct GofileProvider;

// O Gofile valida um "website token" recalculado no servidor a partir do User-Agent
// enviado — logo o UA usado no hash PRECISA ser o mesmo do header das requisições.
const GOFILE_UA: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36";
// Sal embutido no wt.obf.js do Gofile. Pode rotacionar; se o Gofile passar a responder
// "error-notPremium" para contas guest, atualizar este valor (ver gallery-dl/yt-dlp).
const GOFILE_WT_SALT: &str = "5d4f7g8sd45fsd";

struct GofileFile {
    id: String,
    name: String,
    size: u64,
    link: String,
    mime_type: Option<String>,
}

impl GofileProvider {
    pub fn matches(url: &str) -> bool {
        host_matches(url, &["gofile.io", "www.gofile.io"])
    }

    /// Extrai o código do conteúdo de `https://gofile.io/d/{code}`.
    fn content_id(url: &str) -> Option<String> {
        let segments = path_segments(url);
        if let [first, code, ..] = segments.as_slice() {
            if first == "d" && !code.is_empty() {
                return Some(code.clone());
            }
        }
        None
    }

    fn client() -> Result<reqwest::Client> {
        Ok(reqwest::Client::builder().user_agent(GOFILE_UA).build()?)
    }

    async fn create_token(client: &reqwest::Client) -> Result<String> {
        let response: Value = client
            .post("https://api.gofile.io/accounts")
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        response["data"]["token"]
            .as_str()
            .map(String::from)
            .filter(|token| !token.is_empty())
            .ok_or_else(|| anyhow!("Gofile: falha ao criar a sessão de convidado"))
    }

    fn website_token(account_token: &str) -> String {
        let time_window = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() / 14_400)
            .unwrap_or(0);
        let data = format!("{GOFILE_UA}::en-US::{account_token}::{time_window}::{GOFILE_WT_SALT}");
        let mut hasher = Sha256::new();
        hasher.update(data.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    async fn fetch_contents(
        client: &reqwest::Client,
        token: &str,
        content_id: &str,
    ) -> Result<Value> {
        let website_token = Self::website_token(token);
        let url = format!(
            "https://api.gofile.io/contents/{content_id}?contentFilter=&page=1&pageSize=1000&sortField=name&sortDirection=1"
        );
        let response: Value = client
            .get(&url)
            .header("Authorization", format!("Bearer {token}"))
            .header("X-Website-Token", website_token)
            .header("X-BL", "en-US")
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        match response["status"].as_str() {
            Some("ok") => Ok(response["data"].clone()),
            Some("error-rateLimit") => Err(rate_limit_error(
                120,
                "Gofile limitou as requisições. Retry automático agendado.",
            )),
            Some("error-notPremium") => Err(anyhow!(
                "Gofile rejeitou a sessão de convidado (token do site expirado). Tente novamente mais tarde."
            )),
            Some(other) => Err(anyhow!("Gofile respondeu: {other}")),
            None => Err(anyhow!("Gofile: resposta inesperada da API")),
        }
    }

    fn collect_files(data: &Value) -> Vec<GofileFile> {
        let mut files = Vec::new();
        let push_file = |files: &mut Vec<GofileFile>, node: &Value| {
            if node["type"].as_str() != Some("file") {
                return;
            }
            let (Some(id), Some(name), Some(link)) = (
                node["id"].as_str(),
                node["name"].as_str(),
                node["link"].as_str(),
            ) else {
                return;
            };
            files.push(GofileFile {
                id: id.to_string(),
                name: name.to_string(),
                size: node["size"].as_u64().unwrap_or(0),
                link: link.to_string(),
                mime_type: node["mimetype"].as_str().map(String::from),
            });
        };

        if let Some(children) = data["children"].as_object() {
            for child in children.values() {
                push_file(&mut files, child);
            }
        } else if data["type"].as_str() == Some("file") {
            push_file(&mut files, data);
        }
        files
    }

    /// Identidade estável de um filho (não muda entre sessões, ao contrário do link).
    fn child_source_url(content_id: &str, file_id: &str) -> String {
        format!("https://gofile.io/d/{content_id}?file={file_id}")
    }
}

impl ProviderDefaults for GofileProvider {}

impl Provider for GofileProvider {
    fn name(&self) -> &str {
        "Gofile"
    }

    fn get_file_info<'a>(
        &'a self,
        url: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FileInfo>> + Send + 'a>> {
        Box::pin(async move {
            let content_id = Self::content_id(url)
                .ok_or_else(|| anyhow!("URL do Gofile inválida: {url}"))?;
            let client = Self::client()?;
            let token = Self::create_token(&client).await?;
            let data = Self::fetch_contents(&client, &token, &content_id).await?;
            let files = Self::collect_files(&data);

            if files.is_empty() {
                return Err(anyhow!("Gofile: nenhum arquivo encontrado neste link"));
            }

            if files.len() == 1 {
                let file = &files[0];
                return Ok(FileInfo {
                    filename: file.name.clone(),
                    size: file.size,
                    mime_type: file.mime_type.clone(),
                    is_folder: false,
                    children: None,
                    ..Default::default()
                });
            }

            let children = files
                .iter()
                .map(|file| FileChildInfo {
                    filename: file.name.clone(),
                    size: file.size,
                    mime_type: file.mime_type.clone(),
                    is_folder: false,
                    path: None,
                    source_url: Some(Self::child_source_url(&content_id, &file.id)),
                    bytes_downloaded: None,
                    speed_bps: None,
                    eta_secs: None,
                    status: None,
                })
                .collect::<Vec<_>>();
            let total_size = children.iter().map(|child| child.size).sum();
            let folder_name = data["name"].as_str().unwrap_or(&content_id);

            Ok(FileInfo {
                filename: <Self as ProviderDefaults>::safe_filename(folder_name, "gofile"),
                size: total_size,
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
            let content_id = Self::content_id(url)
                .ok_or_else(|| anyhow!("URL do Gofile inválida: {url}"))?;
            let client = Self::client()?;
            let token = Self::create_token(&client).await?;
            let data = Self::fetch_contents(&client, &token, &content_id).await?;
            let mut files = Self::collect_files(&data);

            if files.is_empty() {
                return Err(anyhow!("Gofile: nenhum arquivo encontrado neste link"));
            }

            // Filtra pelos filhos selecionados (quando é pasta).
            if let Some(selected) = selected_children.as_ref() {
                let selected: std::collections::HashSet<&String> = selected.iter().collect();
                files.retain(|file| {
                    selected.contains(&Self::child_source_url(&content_id, &file.id))
                });
            }
            if files.is_empty() {
                return Err(anyhow!("Gofile: nenhum arquivo selecionado disponível"));
            }

            let started_at = tokio::time::Instant::now();
            let total_size: u64 = files.iter().map(|file| file.size).sum();
            let mut session_downloaded = 0u64;

            // Um único arquivo → grava direto no dest_path.
            if files.len() == 1 && selected_children.is_none() {
                let file = &files[0];
                return stream_gofile_file(
                    &client,
                    &token,
                    file,
                    dest_path,
                    &speed_limit_bps,
                    started_at,
                    0,
                    total_size.max(file.size),
                    &mut session_downloaded,
                    None,
                    &progress_tx,
                )
                .await;
            }

            // Pasta → grava cada arquivo dentro de dest_path.
            tokio::fs::create_dir_all(dest_path).await?;
            let mut downloaded_total = 0u64;
            for file in &files {
                let file_path = format!("{}/{}", dest_path.trim_end_matches('/'), file.name);
                let file_bytes = stream_gofile_file(
                    &client,
                    &token,
                    file,
                    &file_path,
                    &speed_limit_bps,
                    started_at,
                    downloaded_total,
                    total_size,
                    &mut session_downloaded,
                    Some(file.name.clone()),
                    &progress_tx,
                )
                .await?;
                downloaded_total += file_bytes;
            }
            Ok(downloaded_total)
        })
    }
}

// Streama um arquivo do Gofile para o disco, com resume via Range e progresso.
#[allow(clippy::too_many_arguments)]
async fn stream_gofile_file(
    client: &reqwest::Client,
    token: &str,
    file: &GofileFile,
    dest_path: &str,
    speed_limit_bps: &super::SpeedLimitBps,
    started_at: tokio::time::Instant,
    base_downloaded: u64,
    total_size: u64,
    session_downloaded: &mut u64,
    child_filename: Option<String>,
    progress_tx: &tokio::sync::mpsc::Sender<ProgressUpdate>,
) -> Result<u64> {
    let existing_bytes = tokio::fs::metadata(dest_path)
        .await
        .ok()
        .filter(|meta| meta.is_file())
        .map(|meta| meta.len())
        .unwrap_or(0);

    if file.size > 0 && existing_bytes >= file.size {
        let _ = progress_tx
            .send(ProgressUpdate {
                bytes_downloaded: base_downloaded + file.size,
                total_bytes: total_size,
                child_path: None,
                child_filename: child_filename.clone(),
                child_bytes_downloaded: Some(file.size),
                child_total_bytes: Some(file.size),
                child_speed_bps: Some(0),
                child_eta_secs: Some(0),
            })
            .await;
        return Ok(file.size);
    }

    let mut request = client
        .get(&file.link)
        .header("Cookie", format!("accountToken={token}"));
    if existing_bytes > 0 {
        request = request.header("Range", format!("bytes={existing_bytes}-"));
    }
    let response = request.send().await?.error_for_status()?;
    let resumed = existing_bytes > 0 && response.status() == reqwest::StatusCode::PARTIAL_CONTENT;

    let mut out = if resumed {
        OpenOptions::new().create(true).append(true).open(dest_path).await?
    } else {
        tokio::fs::File::create(dest_path).await?
    };
    let mut stream = response.bytes_stream();
    let mut file_downloaded = if resumed { existing_bytes } else { 0 };
    let child_started_at = tokio::time::Instant::now();
    let mut child_session = 0u64;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        out.write_all(&chunk).await?;
        let chunk_len = chunk.len() as u64;
        file_downloaded += chunk_len;
        *session_downloaded += chunk_len;
        child_session += chunk_len;

        let child_elapsed = child_started_at.elapsed().as_secs_f64();
        let child_speed = if child_elapsed > 0.0 {
            (child_session as f64 / child_elapsed) as u64
        } else {
            0
        };
        let child_eta = if child_speed > 0 && file.size > file_downloaded {
            (file.size - file_downloaded) / child_speed
        } else {
            0
        };

        let _ = progress_tx
            .send(ProgressUpdate {
                bytes_downloaded: base_downloaded + file_downloaded,
                total_bytes: total_size,
                child_path: None,
                child_filename: child_filename.clone(),
                child_bytes_downloaded: Some(file_downloaded),
                child_total_bytes: Some(file.size),
                child_speed_bps: Some(child_speed),
                child_eta_secs: Some(child_eta),
            })
            .await;
        apply_speed_limit(started_at, *session_downloaded, speed_limit_bps).await;
    }

    out.flush().await?;
    Ok(file_downloaded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_and_extracts_content_id() {
        assert!(GofileProvider::matches("https://gofile.io/d/XkVXyk"));
        assert!(GofileProvider::matches("https://www.gofile.io/d/AbCdef"));
        assert!(!GofileProvider::matches("https://pixeldrain.com/u/abc"));
        assert_eq!(
            GofileProvider::content_id("https://gofile.io/d/XkVXyk").as_deref(),
            Some("XkVXyk")
        );
        assert_eq!(GofileProvider::content_id("https://gofile.io/").as_deref(), None);
    }

    #[test]
    fn website_token_is_deterministic_hex() {
        let a = GofileProvider::website_token("tok123");
        let b = GofileProvider::website_token("tok123");
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn collect_files_reads_children_and_skips_folders() {
        // Estrutura real do /contents: data.children é um objeto id->nó.
        let data = serde_json::json!({
            "type": "folder",
            "name": "pasta",
            "children": {
                "id1": {
                    "type": "file",
                    "id": "id1",
                    "name": "a.bin",
                    "size": 100,
                    "mimetype": "application/octet-stream",
                    "link": "https://store1.gofile.io/download/web/id1/a.bin"
                },
                "id2": {
                    "type": "folder",
                    "id": "id2",
                    "name": "subpasta"
                },
                "id3": {
                    "type": "file",
                    "id": "id3",
                    "name": "b.mp4",
                    "size": 250,
                    "link": "https://store2.gofile.io/download/web/id3/b.mp4"
                }
            }
        });

        let mut files = GofileProvider::collect_files(&data);
        files.sort_by(|l, r| l.name.cmp(&r.name));
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].name, "a.bin");
        assert_eq!(files[0].size, 100);
        assert!(files[0].link.contains("/download/web/id1/"));
        assert_eq!(files[1].name, "b.mp4");
        assert_eq!(files[1].size, 250);
    }

    #[test]
    fn collect_files_handles_single_file_node() {
        let data = serde_json::json!({
            "type": "file",
            "id": "only",
            "name": "single.zip",
            "size": 42,
            "link": "https://store3.gofile.io/download/web/only/single.zip"
        });
        let files = GofileProvider::collect_files(&data);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].id, "only");
        assert_eq!(files[0].size, 42);
    }
}
