use std::collections::{HashMap, HashSet};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

use aes::cipher::generic_array::GenericArray;
use anyhow::{anyhow, Context, Result};
use base64::Engine;
use cbc::Decryptor;
use cipher::{
    block_padding::NoPadding, BlockDecrypt, BlockDecryptMut, KeyInit, KeyIvInit, StreamCipher,
    StreamCipherSeek,
};
use futures_util::StreamExt;
use serde_json::Value;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;

use crate::models::{FileChildInfo, FileInfo};
use super::{apply_speed_limit, host_matches, ProgressUpdate, Provider, ProviderDefaults};

type Aes128Ctr = ctr::Ctr128BE<aes::Aes128>;
type Aes128CbcDec = Decryptor<aes::Aes128>;

pub struct MegaProvider;

#[derive(Debug, Clone)]
struct MegaPublicFileMeta {
    name: String,
    size: u64,
    download_url: String,
}

#[derive(Debug, Clone)]
struct MegaFolderFileEntry {
    handle: String,
    name: String,
    path: String,
    size: u64,
    key_bytes: Vec<u8>,
    source_url: String,
}

#[derive(Debug, Clone)]
struct MegaFolderListing {
    name: String,
    files: Vec<MegaFolderFileEntry>,
}

#[derive(Debug, Clone)]
struct MegaFolderNodeMeta {
    parent: Option<String>,
    name: String,
}

impl MegaProvider {
    pub fn matches(url: &str) -> bool {
        host_matches(url, &["mega.nz", "www.mega.nz", "mega.co.nz", "www.mega.co.nz"])
    }

    pub fn mega_base64_decode(input: &str) -> Vec<u8> {
        let mut normalized = input.trim().replace('-', "+").replace('_', "/");
        while normalized.len() % 4 != 0 {
            normalized.push('=');
        }

        base64::engine::general_purpose::STANDARD
            .decode(normalized)
            .unwrap_or_default()
    }

    pub fn mega_base64_encode(bytes: &[u8]) -> String {
        base64::engine::general_purpose::STANDARD
            .encode(bytes)
            .trim_end_matches('=')
            .replace('+', "-")
            .replace('/', "_")
    }

    pub fn parse_url(url: &str) -> Option<(String, Vec<u8>)> {
        if url.contains("/folder/") || url.contains("#F!") {
            return None;
        }

        if let Some(pos) = url.find("/file/") {
            let after = &url[pos + 6..];
            let parts: Vec<&str> = after.splitn(2, '#').collect();
            if parts.len() == 2 {
                let handle = parts[0]
                    .split('/')
                    .next()?
                    .split('?')
                    .next()?
                    .trim();
                let key = parts[1].split('&').next()?.trim();
                let key_bytes = Self::mega_base64_decode(key);
                if !handle.is_empty() && key_bytes.len() >= 16 {
                    return Some((handle.to_string(), key_bytes));
                }
            }
        }

        if let Some(pos) = url.find("#!") {
            let after = &url[pos + 2..];
            let parts: Vec<&str> = after.splitn(2, '!').collect();
            if parts.len() == 2 {
                let handle = parts[0].trim();
                let key_bytes = Self::mega_base64_decode(parts[1].trim());
                if !handle.is_empty() && key_bytes.len() >= 16 {
                    return Some((handle.to_string(), key_bytes));
                }
            }
        }

        None
    }

    pub fn parse_folder_url(url: &str) -> Option<(String, Vec<u8>)> {
        if let Some(pos) = url.find("/folder/") {
            let after = &url[pos + 8..];
            let parts: Vec<&str> = after.splitn(2, '#').collect();
            if parts.len() == 2 {
                let handle = parts[0]
                    .split('/')
                    .next()?
                    .split('?')
                    .next()?
                    .trim();
                let key = parts[1].split('&').next()?.trim();
                let key_bytes = Self::mega_base64_decode(key);
                if !handle.is_empty() && key_bytes.len() >= 16 {
                    return Some((handle.to_string(), key_bytes));
                }
            }
        }

        if let Some(pos) = url.find("#F!") {
            let after = &url[pos + 3..];
            let parts: Vec<&str> = after.splitn(2, '!').collect();
            if parts.len() == 2 {
                let handle = parts[0].trim();
                let key_bytes = Self::mega_base64_decode(parts[1].trim());
                if !handle.is_empty() && key_bytes.len() >= 16 {
                    return Some((handle.to_string(), key_bytes));
                }
            }
        }

        None
    }

    pub fn derive_key_and_iv(key_bytes: &[u8]) -> ([u8; 16], [u8; 16]) {
        if key_bytes.len() < 32 {
            let mut key = [0u8; 16];
            let len = key_bytes.len().min(16);
            key[..len].copy_from_slice(&key_bytes[..len]);
            return (key, [0u8; 16]);
        }

        let mut aes_key = [0u8; 16];
        for i in 0..16 {
            aes_key[i] = key_bytes[i] ^ key_bytes[i + 16];
        }

        let mut iv = [0u8; 16];
        iv[..8].copy_from_slice(&key_bytes[16..24]);

        (aes_key, iv)
    }

    pub fn decrypt_folder_node_key(shared_key: &[u8], encrypted_node_key: &[u8]) -> Option<Vec<u8>> {
        if shared_key.len() < 16
            || encrypted_node_key.is_empty()
            || encrypted_node_key.len() % 16 != 0
        {
            return None;
        }

        let cipher = aes::Aes128::new(GenericArray::from_slice(&shared_key[..16]));
        let mut decrypted = encrypted_node_key.to_vec();

        for chunk in decrypted.chunks_mut(16) {
            let block = GenericArray::from_mut_slice(chunk);
            cipher.decrypt_block(block);
        }

        Some(decrypted)
    }

    pub fn derive_attr_key_from_node_key(node_key: &[u8], is_file: bool) -> Option<[u8; 16]> {
        if is_file && node_key.len() >= 32 {
            let mut key = [0u8; 16];
            for i in 0..16 {
                key[i] = node_key[i] ^ node_key[i + 16];
            }
            return Some(key);
        }

        if node_key.len() >= 16 {
            let mut key = [0u8; 16];
            key.copy_from_slice(&node_key[..16]);
            return Some(key);
        }

        None
    }

    pub fn decrypt_attributes_name(attr_b64: &str, attr_key: &[u8]) -> Option<String> {
        if attr_key.len() < 16 || attr_b64.trim().is_empty() {
            return None;
        }

        let mut encrypted = Self::mega_base64_decode(attr_b64);
        if encrypted.is_empty() || encrypted.len() % 16 != 0 {
            return None;
        }

        let decrypted = Aes128CbcDec::new_from_slices(&attr_key[..16], &[0u8; 16])
            .ok()?
            .decrypt_padded_mut::<NoPadding>(&mut encrypted)
            .ok()?;

        let trimmed = decrypted
            .iter()
            .copied()
            .take_while(|byte| *byte != 0)
            .collect::<Vec<_>>();
        let decoded = String::from_utf8(trimmed).ok()?;
        let payload = decoded.strip_prefix("MEGA").unwrap_or(&decoded);
        let json_start = payload.find('{')?;
        let attrs: Value = serde_json::from_str(&payload[json_start..]).ok()?;
        attrs["n"].as_str().map(|value| value.to_string())
    }

    async fn get_public_file_meta(
        client: &reqwest::Client,
        handle: &str,
        key_bytes: &[u8],
    ) -> Result<MegaPublicFileMeta> {
        let api_url = "https://g.api.mega.co.nz/cs?id=0";
        let body = serde_json::json!([{ "a": "g", "p": handle, "g": 1, "ssl": 2 }]);

        let resp: Value = client
            .post(api_url)
            .json(&body)
            .send()
            .await
            .context("Falha ao consultar a API pública do Mega")?
            .json()
            .await
            .context("Falha ao parsear resposta pública do Mega")?;

        let result = &resp[0];
        if let Some(err_code) = result.as_i64() {
            return Err(anyhow!(
                "Mega retornou erro {err_code}. O arquivo pode não existir ou ser privado."
            ));
        }

        let (attr_key, _) = Self::derive_key_and_iv(key_bytes);
        let name = result["at"]
            .as_str()
            .and_then(|attr| Self::decrypt_attributes_name(attr, &attr_key))
            .unwrap_or_else(|| format!("mega_{handle}"));

        let download_url = result["g"]
            .as_str()
            .ok_or_else(|| anyhow!("API do Mega não retornou URL de download"))?
            .to_string();

        Ok(MegaPublicFileMeta {
            name: <Self as ProviderDefaults>::safe_filename(&name, &format!("mega_{handle}")),
            size: result["s"].as_u64().unwrap_or(0),
            download_url,
        })
    }

    async fn get_public_folder_listing(
        client: &reqwest::Client,
        folder_handle: &str,
        shared_key: &[u8],
    ) -> Result<MegaFolderListing> {
        let api_url = format!("https://g.api.mega.co.nz/cs?id=0&n={folder_handle}");
        let body = serde_json::json!([{ "a": "f", "c": 1, "r": 1 }]);

        let resp: Value = client
            .post(&api_url)
            .json(&body)
            .send()
            .await
            .context("Falha ao consultar a API de pasta do Mega")?
            .json()
            .await
            .context("Falha ao parsear resposta da pasta do Mega")?;

        let result = &resp[0];
        if let Some(err_code) = result.as_i64() {
            return Err(anyhow!(
                "Mega retornou erro {err_code} ao listar a pasta pública"
            ));
        }

        let nodes = result["f"]
            .as_array()
            .ok_or_else(|| anyhow!("Resposta da pasta do Mega sem o campo de nós"))?;

        let root_node = nodes
            .iter()
            .find(|node| node["h"].as_str() == Some(folder_handle))
            .or_else(|| {
                nodes.iter().find(|node| {
                    node["t"].as_u64() == Some(1)
                        && node["p"].as_str().is_none()
                })
            })
            .or_else(|| nodes.iter().find(|node| node["t"].as_u64() == Some(1)));

        let root_handle = root_node
            .and_then(|node| node["h"].as_str())
            .unwrap_or(folder_handle);

        let folder_name = root_node
            .and_then(|node| {
                let raw_key_field = node["k"].as_str()?;
                let key_part = raw_key_field.rsplit(':').next()?;
                let encrypted_node_key = Self::mega_base64_decode(key_part);
                let node_key = Self::decrypt_folder_node_key(shared_key, &encrypted_node_key)?;
                let attr_key = Self::derive_attr_key_from_node_key(&node_key, false)?;
                let attr = node["a"].as_str()?;
                Self::decrypt_attributes_name(attr, &attr_key)
            })
            .unwrap_or_else(|| format!("mega_{folder_handle}"));

        let mut files = Self::collect_folder_files(nodes, Some(root_handle), shared_key);
        if files.is_empty() {
            files = Self::collect_folder_files(nodes, None, shared_key);
        }

        Ok(MegaFolderListing {
            name: <Self as ProviderDefaults>::safe_filename(&folder_name, &format!("mega_{folder_handle}")),
            files,
        })
    }

    fn build_folder_map(
        nodes: &[Value],
        shared_key: &[u8],
    ) -> HashMap<String, MegaFolderNodeMeta> {
        let mut folders = HashMap::new();

        for node in nodes {
            if node["t"].as_u64() != Some(1) {
                continue;
            }

            let Some(handle) = node["h"].as_str().map(str::to_string) else {
                continue;
            };

            let name = node["k"]
                .as_str()
                .and_then(|raw_key_field| raw_key_field.rsplit(':').next())
                .map(Self::mega_base64_decode)
                .and_then(|encrypted_node_key| Self::decrypt_folder_node_key(shared_key, &encrypted_node_key))
                .and_then(|node_key| Self::derive_attr_key_from_node_key(&node_key, false))
                .zip(node["a"].as_str())
                .and_then(|(attr_key, attr)| Self::decrypt_attributes_name(attr, &attr_key))
                .unwrap_or_else(|| format!("mega_{handle}"));

            folders.insert(handle, MegaFolderNodeMeta {
                parent: node["p"].as_str().map(str::to_string),
                name: <Self as ProviderDefaults>::safe_filename(&name, "pasta_mega"),
            });
        }

        folders
    }

    fn is_descendant_of(
        parent_handle: Option<&str>,
        root_handle: &str,
        folder_map: &HashMap<String, MegaFolderNodeMeta>,
    ) -> bool {
        let mut current = parent_handle.map(str::to_string);
        while let Some(handle) = current {
            if handle == root_handle {
                return true;
            }
            current = folder_map.get(&handle).and_then(|meta| meta.parent.clone());
        }
        false
    }

    fn relative_segments(
        parent_handle: Option<&str>,
        root_handle: Option<&str>,
        folder_map: &HashMap<String, MegaFolderNodeMeta>,
    ) -> Vec<String> {
        let mut segments = Vec::new();
        let mut current = parent_handle.map(str::to_string);

        while let Some(handle) = current {
            if root_handle.is_some() && Some(handle.as_str()) == root_handle {
                break;
            }

            let Some(meta) = folder_map.get(&handle) else {
                break;
            };
            segments.push(meta.name.clone());
            current = meta.parent.clone();
        }

        segments.reverse();
        segments
    }

    fn collect_folder_files(
        nodes: &[Value],
        root_handle: Option<&str>,
        shared_key: &[u8],
    ) -> Vec<MegaFolderFileEntry> {
        let mut seen = HashSet::new();
        let mut files = Vec::new();
        let folder_map = Self::build_folder_map(nodes, shared_key);

        for node in nodes {
            if node["t"].as_u64() != Some(0) {
                continue;
            }

            let parent_handle = node["p"].as_str();
            if let Some(root) = root_handle {
                if !Self::is_descendant_of(parent_handle, root, &folder_map) {
                    continue;
                }
            }

            let Some(handle) = node["h"].as_str().map(str::to_string) else {
                continue;
            };
            if !seen.insert(handle.clone()) {
                continue;
            }

            let Some(raw_key_field) = node["k"].as_str() else {
                continue;
            };
            let Some(key_part) = raw_key_field.rsplit(':').next() else {
                continue;
            };

            let encrypted_node_key = Self::mega_base64_decode(key_part);
            let Some(node_key) = Self::decrypt_folder_node_key(shared_key, &encrypted_node_key) else {
                continue;
            };
            let Some(attr_key) = Self::derive_attr_key_from_node_key(&node_key, true) else {
                continue;
            };

            let name = node["a"]
                .as_str()
                .and_then(|attr| Self::decrypt_attributes_name(attr, &attr_key))
                .unwrap_or_else(|| format!("mega_{handle}"));
            let safe_name = <Self as ProviderDefaults>::safe_filename(&name, &format!("mega_{handle}"));
            let relative_segments = Self::relative_segments(parent_handle, root_handle, &folder_map);
            let path = if relative_segments.is_empty() {
                safe_name.clone()
            } else {
                format!("{}/{}", relative_segments.join("/"), safe_name)
            };

            files.push(MegaFolderFileEntry {
                handle: handle.clone(),
                name: safe_name,
                path,
                size: node["s"].as_u64().unwrap_or(0),
                source_url: format!("https://mega.nz/file/{}#{}", handle, Self::mega_base64_encode(&node_key)),
                key_bytes: node_key,
            });
        }

        files
    }

    async fn get_folder_child_download_url(
        client: &reqwest::Client,
        folder_handle: &str,
        file_handle: &str,
    ) -> Result<(String, u64)> {
        let api_url = format!("https://g.api.mega.co.nz/cs?id=0&n={folder_handle}");
        let body = serde_json::json!([{ "a": "g", "n": file_handle, "g": 1, "ssl": 2 }]);

        let resp: Value = client
            .post(&api_url)
            .json(&body)
            .send()
            .await
            .context("Falha ao obter URL de download do arquivo da pasta Mega")?
            .json()
            .await
            .context("Falha ao parsear URL de download do arquivo da pasta Mega")?;

        let result = &resp[0];
        if let Some(err_code) = result.as_i64() {
            return Err(anyhow!(
                "Mega retornou erro {err_code} ao obter um arquivo da pasta pública"
            ));
        }

        let download_url = result["g"]
            .as_str()
            .ok_or_else(|| anyhow!("Mega não retornou a URL do arquivo da pasta"))?
            .to_string();
        let size = result["s"].as_u64().unwrap_or(0);

        Ok((download_url, size))
    }

    async fn try_parallel_download(
        client: &reqwest::Client,
        download_url: &str,
        dest_path: &str,
        total_size: u64,
        aes_key: [u8; 16],
        iv: [u8; 16],
        speed_limit_bps: Option<u64>,
        parallel_parts: usize,
        progress_tx: tokio::sync::mpsc::Sender<ProgressUpdate>,
    ) -> Result<Option<u64>> {
        if parallel_parts <= 1 || total_size == 0 {
            return Ok(None);
        }

        const MIN_PART_SIZE: u64 = 2 * 1024 * 1024;
        let max_useful_parts = (total_size / MIN_PART_SIZE).max(1) as usize;
        let part_count = parallel_parts.min(max_useful_parts).max(1);
        if part_count <= 1 {
            return Ok(None);
        }

        let part_dir = format!("{dest_path}.parts");
        let _ = tokio::fs::remove_dir_all(&part_dir).await;
        tokio::fs::create_dir_all(&part_dir).await?;

        let started_at = tokio::time::Instant::now();
        let total_downloaded = Arc::new(AtomicU64::new(0));
        let mut tasks = Vec::with_capacity(part_count);

        for part_index in 0..part_count {
            let client = client.clone();
            let download_url = download_url.to_string();
            let part_dir = part_dir.clone();
            let progress_tx = progress_tx.clone();
            let total_downloaded = Arc::clone(&total_downloaded);
            let start = (total_size * part_index as u64) / part_count as u64;
            let end = ((total_size * (part_index as u64 + 1)) / part_count as u64).saturating_sub(1);
            let aes_key = aes_key;
            let iv = iv;

            tasks.push(tokio::spawn(async move {
                let part_path = format!("{part_dir}/part-{part_index:03}");
                let resp = client
                    .get(&download_url)
                    .header("Range", format!("bytes={start}-{end}"))
                    .send()
                    .await?
                    .error_for_status()?;

                if resp.status() != reqwest::StatusCode::PARTIAL_CONTENT {
                    return Err(anyhow!("Mega não aceitou download em partes"));
                }

                let mut file = tokio::fs::File::create(&part_path).await?;
                let mut stream = resp.bytes_stream();
                let mut cipher = Aes128Ctr::new(&aes_key.into(), &iv.into());
                cipher.seek(start);

                while let Some(chunk) = stream.next().await {
                    let mut chunk = chunk?.to_vec();
                    cipher.apply_keystream(&mut chunk);
                    file.write_all(&chunk).await?;

                    let downloaded = total_downloaded.fetch_add(chunk.len() as u64, Ordering::SeqCst)
                        + chunk.len() as u64;

                    let _ = progress_tx
                        .send(ProgressUpdate {
                            bytes_downloaded: downloaded,
                            total_bytes: total_size,
                            child_path: None,
                            child_filename: None,
                            child_bytes_downloaded: None,
                            child_total_bytes: None,
                            child_speed_bps: None,
                            child_eta_secs: None,
                        })
                        .await;

                    apply_speed_limit(
                        started_at,
                        total_downloaded.load(Ordering::SeqCst),
                        speed_limit_bps,
                    )
                    .await;
                }

                file.flush().await?;
                Ok::<(), anyhow::Error>(())
            }));
        }

        for task in tasks {
            task.await??;
        }

        let _ = tokio::fs::remove_file(dest_path).await;
        let mut output = tokio::fs::File::create(dest_path).await?;
        for part_index in 0..part_count {
            let part_path = format!("{part_dir}/part-{part_index:03}");
            let mut part = OpenOptions::new().read(true).open(&part_path).await?;
            tokio::io::copy(&mut part, &mut output).await?;
        }
        output.flush().await?;
        let _ = tokio::fs::remove_dir_all(&part_dir).await;

        Ok(Some(total_size))
    }
}

impl ProviderDefaults for MegaProvider {}

impl Provider for MegaProvider {
    fn name(&self) -> &str { "Mega" }

    fn get_file_info<'a>(
        &'a self,
        url: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FileInfo>> + Send + 'a>>
    {
        Box::pin(async move {
            let client = <Self as ProviderDefaults>::http_client()?;

            if let Some((folder_handle, shared_key)) = Self::parse_folder_url(url) {
                let listing = Self::get_public_folder_listing(&client, &folder_handle, &shared_key).await?;
                let children = listing
                    .files
                    .iter()
                    .map(|file| FileChildInfo {
                        filename: file.name.clone(),
                        size: file.size,
                        mime_type: None,
                        is_folder: false,
                        path: Some(file.path.clone()),
                        source_url: Some(file.source_url.clone()),
                        bytes_downloaded: None,
                        speed_bps: None,
                        eta_secs: None,
                        status: None,
                    })
                    .collect::<Vec<_>>();
                let total_size = children.iter().map(|child| child.size).sum();

                return Ok(FileInfo {
                    filename: listing.name,
                    size: total_size,
                    mime_type: None,
                    is_folder: true,
                    children: Some(children),
                });
            }

            let (handle, key_bytes) = Self::parse_url(url)
                .ok_or_else(|| anyhow!("URL do Mega inválida: {url}"))?;
            let meta = Self::get_public_file_meta(&client, &handle, &key_bytes).await?;

            Ok(FileInfo {
                filename: meta.name,
                size: meta.size,
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
        parallel_parts: usize,
        selected_children: Option<Vec<String>>,
        progress_tx: tokio::sync::mpsc::Sender<ProgressUpdate>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u64>> + Send + 'a>>
    {
        Box::pin(async move {
            let client = <Self as ProviderDefaults>::http_client()?;
            let started_at = tokio::time::Instant::now();

            if let Some((folder_handle, shared_key)) = Self::parse_folder_url(url) {
                let listing = Self::get_public_folder_listing(&client, &folder_handle, &shared_key).await?;
                let mut files = listing.files;
                if let Some(selected) = selected_children.filter(|items| !items.is_empty()) {
                    let selected_set = selected.into_iter().collect::<HashSet<_>>();
                    files.retain(|file| selected_set.contains(&file.source_url));
                }

                if files.is_empty() {
                    return Err(anyhow!("Pasta pública do Mega vazia ou sem arquivos acessíveis"));
                }

                tokio::fs::create_dir_all(dest_path).await?;

                let total_size: u64 = files.iter().map(|file| file.size).sum();
                let mut downloaded_total = 0u64;
                let mut session_downloaded = 0u64;

                for file in &files {
                    let output_path = format!("{}/{}", dest_path.trim_end_matches('/'), file.path);
                    if let Some(parent_dir) = std::path::Path::new(&output_path).parent() {
                        tokio::fs::create_dir_all(parent_dir).await?;
                    }
                    let existing_bytes = tokio::fs::metadata(&output_path)
                        .await
                        .ok()
                        .filter(|meta| meta.is_file())
                        .map(|meta| meta.len())
                        .unwrap_or(0);

                    if file.size > 0 && existing_bytes >= file.size {
                        downloaded_total += file.size;
                        continue;
                    }

                    let (download_url, _file_size) =
                        Self::get_folder_child_download_url(&client, &folder_handle, &file.handle).await?;
                    let (aes_key, iv) = Self::derive_key_and_iv(&file.key_bytes);

                    let mut request = client.get(&download_url);
                    if existing_bytes > 0 {
                        request = request.header("Range", format!("bytes={existing_bytes}-"));
                    }
                    let resp = request.send().await?.error_for_status()?;
                    let resumed = existing_bytes > 0 && resp.status() == reqwest::StatusCode::PARTIAL_CONTENT;
                    if resumed {
                        downloaded_total += existing_bytes;
                    }

                    let mut stream = resp.bytes_stream();
                    let mut file_handle = if resumed {
                        OpenOptions::new().create(true).append(true).open(&output_path).await?
                    } else {
                        tokio::fs::File::create(&output_path).await?
                    };
                    let mut cipher = Aes128Ctr::new(&aes_key.into(), &iv.into());
                    if resumed {
                        cipher.seek(existing_bytes);
                    }
                    let mut child_session_downloaded = 0u64;
                    let child_started_at = tokio::time::Instant::now();

                    while let Some(chunk) = stream.next().await {
                        let mut chunk = chunk?.to_vec();
                        cipher.apply_keystream(&mut chunk);
                        file_handle.write_all(&chunk).await?;
                        let chunk_len = chunk.len() as u64;
                        downloaded_total += chunk_len;
                        session_downloaded += chunk_len;
                        child_session_downloaded += chunk_len;

                        let child_elapsed = child_started_at.elapsed().as_secs_f64();
                        let child_speed = if child_elapsed > 0.0 {
                            (child_session_downloaded as f64 / child_elapsed) as u64
                        } else {
                            0
                        };
                        let child_downloaded = if resumed { existing_bytes } else { 0 } + child_session_downloaded;
                        let child_eta = if child_speed > 0 && file.size > child_downloaded {
                            (file.size - child_downloaded) / child_speed
                        } else {
                            0
                        };

                        let _ = progress_tx
                            .send(ProgressUpdate {
                                bytes_downloaded: downloaded_total,
                                total_bytes: total_size,
                                child_path: Some(file.path.clone()),
                                child_filename: Some(file.name.clone()),
                                child_bytes_downloaded: Some(child_downloaded),
                                child_total_bytes: Some(file.size),
                                child_speed_bps: Some(child_speed),
                                child_eta_secs: Some(child_eta),
                            })
                            .await;
                        apply_speed_limit(started_at, session_downloaded, speed_limit_bps).await;
                    }

                    file_handle.flush().await?;
                }

                return Ok(downloaded_total);
            }

            let (handle, key_bytes) = Self::parse_url(url)
                .ok_or_else(|| anyhow!("URL do Mega inválida: {url}"))?;
            let meta = Self::get_public_file_meta(&client, &handle, &key_bytes).await?;
            let (aes_key, iv) = Self::derive_key_and_iv(&key_bytes);
            let existing_bytes = tokio::fs::metadata(dest_path)
                .await
                .ok()
                .filter(|meta| meta.is_file())
                .map(|meta| meta.len())
                .unwrap_or(0);

            if existing_bytes == 0 {
                if let Some(downloaded) = Self::try_parallel_download(
                    &client,
                    &meta.download_url,
                    dest_path,
                    meta.size,
                    aes_key,
                    iv,
                    speed_limit_bps,
                    parallel_parts,
                    progress_tx.clone(),
                )
                .await?
                {
                    return Ok(downloaded);
                }
            }

            let (resp, resumed) =
                <Self as ProviderDefaults>::send_with_resume_fallback(&client, &meta.download_url, dest_path, existing_bytes).await?;
            let total = if resumed {
                <Self as ProviderDefaults>::response_total_bytes(&resp, existing_bytes)
            } else {
                meta.size
            };
            let mut file = if resumed {
                OpenOptions::new().create(true).append(true).open(dest_path).await?
            } else {
                tokio::fs::File::create(dest_path).await?
            };
            let mut stream = resp.bytes_stream();
            let mut downloaded = if resumed { existing_bytes } else { 0 };
            let mut session_downloaded = 0u64;
            let mut cipher = Aes128Ctr::new(&aes_key.into(), &iv.into());
            if resumed {
                cipher.seek(existing_bytes);
            }

            while let Some(chunk) = stream.next().await {
                let mut chunk = chunk?.to_vec();
                cipher.apply_keystream(&mut chunk);
                file.write_all(&chunk).await?;
                let chunk_len = chunk.len() as u64;
                downloaded += chunk_len;
                session_downloaded += chunk_len;

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
        })
    }
}
