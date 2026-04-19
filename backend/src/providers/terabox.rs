use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::{json, Value};
use std::fs;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;

use crate::models::{FileChildInfo, FileInfo};
use super::{apply_speed_limit, ProgressUpdate, Provider, ProviderDefaults};

const APP_ID: &str = "250528";
const CHANNEL: &str = "dubox";

pub struct TeraboxProvider;

#[derive(Clone)]
struct ShareContext {
    js_token: String,
    surl: String,
    shareid: i64,
    uk: i64,
    sign: String,
    timestamp: i64,
    host: &'static str,  // "https://www.terabox.com" ou "https://www.1024tera.com"
}

#[derive(Clone)]
struct ShareEntry {
    fs_id: String,
    filename: String,
    path: String,
    size: u64,
    is_dir: bool,
    category: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RootSettings {
    accounts: Option<SettingsAccounts>,
}

#[derive(Debug, Deserialize)]
struct SettingsAccounts {
    terabox: Option<TeraboxAccountSettings>,
}

#[derive(Debug, Deserialize)]
struct TeraboxAccountSettings {
    cookies: Option<Vec<String>>,
}

impl TeraboxProvider {
    pub fn matches(url: &str) -> bool {
        (url.contains("1024tera.com/") || url.contains("terabox.com/"))
            && (url.contains("/sharing/link") || url.contains("/sharing/videoPlay"))
    }

    fn extract_query_value(url: &str, key: &str) -> Option<String> {
        let query = url.split('?').nth(1)?;
        for pair in query.split('&') {
            let mut parts = pair.splitn(2, '=');
            let k = parts.next()?;
            let v = parts.next().unwrap_or("");
            if k == key {
                return Some(v.replace('+', " "));
            }
        }
        None
    }

    fn decode_url_component(value: &str) -> String {
        let mut out = String::with_capacity(value.len());
        let bytes = value.as_bytes();
        let mut i = 0usize;
        while i < bytes.len() {
            if bytes[i] == b'%' && i + 2 < bytes.len() {
                let hex = &value[i + 1..i + 3];
                if let Ok(decoded) = u8::from_str_radix(hex, 16) {
                    out.push(decoded as char);
                    i += 3;
                    continue;
                }
            }
            out.push(bytes[i] as char);
            i += 1;
        }
        out
    }

    fn extract_surl(url: &str) -> Option<String> {
        Self::extract_query_value(url, "surl").map(|v| Self::decode_url_component(&v))
    }

    fn extract_dir(url: &str) -> Option<String> {
        // Suporta tanto ?dir= quanto ?path= (links públicos do terabox.com usam path=)
        Self::extract_query_value(url, "dir")
            .or_else(|| Self::extract_query_value(url, "path"))
            .map(|v| Self::decode_url_component(&v))
            .filter(|v| !v.is_empty())
    }

    fn extract_fsid(url: &str) -> Option<String> {
        Self::extract_query_value(url, "fsid").map(|v| Self::decode_url_component(&v))
    }

    fn detect_lang(url: &str) -> &'static str {
        if url.contains("/portuguese/") {
            "pt"
        } else {
            "en"
        }
    }

    fn extract_js_token(html: &str) -> Option<String> {
        let marker = "fn%28%22";
        let start = html.find(marker)? + marker.len();
        let rest = &html[start..];
        let end = rest.find('"')?;
        let candidate = &rest[..end];
        if candidate.len() >= 32 {
            Some(candidate.to_string())
        } else {
            None
        }
    }

    fn make_logid() -> String {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        format!("{nanos}")
    }

    fn saved_cookie_header() -> Option<String> {
        let settings_path = std::env::current_dir().ok()?.join("settings.json");
        let content = fs::read_to_string(settings_path).ok()?;
        let parsed: RootSettings = serde_json::from_str(&content).ok()?;
        let cookies = parsed.accounts?.terabox?.cookies?;
        let header = cookies
            .into_iter()
            .filter(|cookie| !cookie.trim().is_empty())
            .collect::<Vec<_>>()
            .join("; ");
        if header.is_empty() {
            None
        } else {
            Some(header)
        }
    }

    fn with_saved_cookies(req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let Some(cookie_header) = Self::saved_cookie_header() {
            req.header("Cookie", cookie_header)
        } else {
            req
        }
    }

    async fn build_context(client: &reqwest::Client, url: &str) -> Result<ShareContext> {
        let html = Self::with_saved_cookies(
            client
                .get(url)
                .header("Referer", url)
        )
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;

        let js_token = Self::extract_js_token(&html)
            .ok_or_else(|| anyhow!("Não foi possível extrair o jsToken do TeraBox"))?;
        let surl = Self::extract_surl(url)
            .ok_or_else(|| anyhow!("URL do TeraBox sem parâmetro surl"))?;

        let info = Self::fetch_shorturl_info(client, &js_token, &surl, url).await?;
        Ok(ShareContext {
            js_token,
            surl,
            shareid: info["shareid"].as_i64().ok_or_else(|| anyhow!("Resposta do TeraBox sem shareid"))?,
            uk: info["uk"].as_i64().ok_or_else(|| anyhow!("Resposta do TeraBox sem uk"))?,
            sign: info["sign"].as_str().unwrap_or_default().to_string(),
            timestamp: info["timestamp"].as_i64().ok_or_else(|| anyhow!("Resposta do TeraBox sem timestamp"))?,
            host: Self::api_host(url),
        })
    }

    fn api_host(url: &str) -> &'static str {
        if url.contains("terabox.com") {
            "https://www.terabox.com"
        } else {
            "https://www.1024tera.com"
        }
    }

    async fn fetch_shorturl_info(
        client: &reqwest::Client,
        js_token: &str,
        surl: &str,
        origin_url: &str,
    ) -> Result<Value> {
        let logid = Self::make_logid();
        let host = Self::api_host(origin_url);
        let json: Value = Self::with_saved_cookies(
            client
                .get(format!("{host}/api/shorturlinfo"))
                .query(&[
                    ("app_id", APP_ID),
                    ("web", "1"),
                    ("channel", CHANNEL),
                    ("clienttype", "0"),
                    ("jsToken", js_token),
                    ("dp-logid", &logid),
                    ("shorturl", &format!("1{surl}")),
                    ("root", "1"),
                    ("scene", ""),
                ])
        )
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let errno = json["errno"].as_i64().unwrap_or(-1);
        let errmsg = json["errmsg"].as_str().unwrap_or("");
        if errno == 400210 || errmsg.contains("verify_v2") {
            return Err(anyhow!(
                "O compartilhamento público do TeraBox exigiu verificação adicional do host antes de liberar a leitura."
            ));
        }

        if errno != 0 {
            return Err(anyhow!(
                "TeraBox falhou ao ler o compartilhamento (errno {errno}{}{})",
                if errmsg.is_empty() { "" } else { ": " },
                errmsg
            ));
        }

        Ok(json)
    }

    async fn fetch_share_list(
        client: &reqwest::Client,
        ctx: &ShareContext,
        dir: Option<&str>,
        fid: Option<&str>,
        page: usize,
    ) -> Result<Vec<ShareEntry>> {
        let logid = Self::make_logid();
        let mut req = client
            .get(format!("{}/share/list", ctx.host))
            .query(&[
                ("app_id", APP_ID),
                ("web", "1"),
                ("channel", CHANNEL),
                ("clienttype", "0"),
                ("jsToken", ctx.js_token.as_str()),
                ("dp-logid", logid.as_str()),
                ("page", &page.to_string()),
                ("num", "100"),
                ("by", "name"),
                ("order", "asc"),
                ("site_referer", ""),
                ("shorturl", ctx.surl.as_str()),
            ]);

        if let Some(fid) = fid {
            req = req.query(&[("fid", fid)]);
        }
        if let Some(dir) = dir {
            req = req.query(&[("dir", dir)]);
        } else {
            req = req.query(&[("root", "1")]);
        }

        let json: Value = Self::with_saved_cookies(req).send().await?.error_for_status()?.json().await?;
        let errno = json["errno"]
            .as_i64()
            .or_else(|| json["code"].as_i64())
            .unwrap_or(-1);
        let errmsg = json["errmsg"].as_str().unwrap_or("");
        if errno != 0 {
            if errno == 460020 || errmsg.contains("need verify") {
                return Err(anyhow!(
                    "O TeraBox exigiu verificação pública ou código de extração para abrir esta pasta."
                ));
            }
            return Err(anyhow!(
                "TeraBox falhou ao listar a pasta (errno {errno}{}{})",
                if errmsg.is_empty() { "" } else { ": " },
                errmsg
            ));
        }

        let mut out = Vec::new();
        for item in json["list"].as_array().cloned().unwrap_or_default() {
            out.push(ShareEntry {
                fs_id: item["fs_id"].as_str().unwrap_or_default().to_string(),
                filename: item["server_filename"]
                    .as_str()
                    .unwrap_or("arquivo_terabox")
                    .to_string(),
                path: item["path"].as_str().unwrap_or_default().to_string(),
                size: item["size"]
                    .as_str()
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(0),
                is_dir: item["isdir"].as_str().unwrap_or("0") == "1",
                category: item["category"].as_str().map(str::to_string),
            });
        }

        Ok(out)
    }

    async fn collect_folder_files(
        client: &reqwest::Client,
        ctx: &ShareContext,
        root_dir: String,
        root_fid: String,
    ) -> Result<Vec<ShareEntry>> {
        let mut files = Vec::new();
        let mut pending = vec![(root_dir, root_fid)];

        while let Some((dir, fid)) = pending.pop() {
            let mut page = 1usize;
            loop {
                let list = Self::fetch_share_list(client, ctx, Some(&dir), Some(&fid), page).await?;
                let count = list.len();
                if count == 0 {
                    break;
                }

                for item in list {
                    if item.is_dir {
                        pending.push((item.path.clone(), item.fs_id.clone()));
                    } else {
                        files.push(item);
                    }
                }

                if count < 100 {
                    break;
                }
                page += 1;
            }
        }

        Ok(files)
    }

    fn entry_to_child(url: &str, entry: &ShareEntry, relative_path: String) -> FileChildInfo {
        FileChildInfo {
            filename: entry.filename.clone(),
            size: entry.size,
            mime_type: None,
            is_folder: false,
            path: Some(relative_path),
            source_url: Some(if entry.category.as_deref() == Some("1") {
                format!(
                    "https://www.1024tera.com/sharing/videoPlay?surl={}&dir={}&fsid={}&fileName={}",
                    Self::extract_surl(url).unwrap_or_default(),
                    entry.path.rsplit_once('/').map(|(parent, _)| parent).unwrap_or(""),
                    entry.fs_id,
                    entry.filename.replace(' ', "+"),
                )
            } else {
                url.to_string()
            }),
            bytes_downloaded: None,
            speed_bps: None,
            eta_secs: None,
            status: None,
        }
    }

    fn split_path(path: &str) -> Vec<&str> {
        path.split('/').filter(|segment| !segment.is_empty()).collect()
    }

    fn relative_path(root_path: &str, full_path: &str) -> String {
        let root = Self::split_path(root_path);
        let full = Self::split_path(full_path);
        let relative = if full.starts_with(&root) {
            &full[root.len()..]
        } else {
            &full[..]
        };
        relative.join("/")
    }

    async fn resolve_file_entry(client: &reqwest::Client, url: &str) -> Result<(ShareContext, ShareEntry)> {
        let ctx = Self::build_context(client, url).await?;
        if let (Some(dir), Some(fsid)) = (Self::extract_dir(url), Self::extract_fsid(url)) {
            let items = Self::fetch_share_list(client, &ctx, Some(&dir), Some(&fsid), 1).await?;
            if let Some(entry) = items.into_iter().find(|item| item.fs_id == fsid) {
                return Ok((ctx, entry));
            }
        }

        let root_items = Self::fetch_share_list(client, &ctx, None, None, 1).await?;
        let first = root_items.into_iter().find(|item| !item.is_dir)
            .ok_or_else(|| anyhow!("TeraBox sem arquivo acessível neste compartilhamento"))?;
        Ok((ctx, first))
    }

    async fn request_download_json(client: &reqwest::Client, ctx: &ShareContext, fsid: &str, referer: &str) -> Result<Value> {
        let logid = Self::make_logid();
        let payload = [
            ("shareid", ctx.shareid.to_string()),
            ("uk", ctx.uk.to_string()),
            ("sign", ctx.sign.clone()),
            ("timestamp", ctx.timestamp.to_string()),
            ("fid_list", json!([fsid]).to_string()),
            ("primaryid", ctx.shareid.to_string()),
        ];

        Ok(Self::with_saved_cookies(
            client
                .post(format!("{}/share/download", ctx.host))
                .header("Origin", ctx.host)
                .header("Referer", referer)
                .query(&[
                    ("app_id", APP_ID),
                    ("web", "1"),
                    ("channel", CHANNEL),
                    ("clienttype", "0"),
                    ("jsToken", ctx.js_token.as_str()),
                    ("dp-logid", logid.as_str()),
                    ("product", "share"),
                    ("nozip", "0"),
                ])
                .form(&payload)
        )
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?)
    }

    fn terabox_download_error(json: &Value) -> anyhow::Error {
        let errno = json["errno"].as_i64().unwrap_or(-1);
        let errmsg = json["errmsg"].as_str().unwrap_or("");
        if errno == 400310 || errmsg.contains("verify_v2") {
            if Self::saved_cookie_header().is_some() {
                anyhow!("TeraBox exigiu verify_v2 mesmo com conta salva. A sessão do host ainda não foi suficiente para liberar este arquivo.")
            } else {
                anyhow!("TeraBox exigiu verify_v2 para liberar o arquivo. A listagem pública funciona, mas este download ainda depende de conta válida ou verificação pública do host.")
            }
        } else {
            anyhow!("TeraBox falhou ao liberar o download (errno {errno}{}{})",
                if errmsg.is_empty() { "" } else { ": " },
                errmsg
            )
        }
    }

    fn extract_download_url(json: &Value) -> Option<String> {
        json["dlink"]
            .as_str()
            .map(str::to_string)
            .or_else(|| {
                json["list"]
                    .as_array()
                    .and_then(|items| items.first())
                    .and_then(|item| item["dlink"].as_str())
                    .map(str::to_string)
            })
            .or_else(|| {
                json["download_url"]
                    .as_str()
                    .map(str::to_string)
            })
    }
}

impl ProviderDefaults for TeraboxProvider {}

impl Provider for TeraboxProvider {
    fn name(&self) -> &str { "Terabox" }

    fn get_file_info<'a>(
        &'a self,
        url: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FileInfo>> + Send + 'a>> {
        Box::pin(async move {
            let client = <Self as ProviderDefaults>::http_client()?;

            if url.contains("/sharing/videoPlay") {
                let (_, entry) = Self::resolve_file_entry(&client, url).await?;
                return Ok(FileInfo {
                    filename: <Self as ProviderDefaults>::safe_filename(&entry.filename, "arquivo_terabox"),
                    size: entry.size,
                    mime_type: None,
                    is_folder: false,
                    children: None,
                });
            }

            let ctx = Self::build_context(&client, url).await?;
            let root_items = Self::fetch_share_list(&client, &ctx, None, None, 1).await?;
            let root = root_items.first().ok_or_else(|| anyhow!("Compartilhamento do TeraBox vazio"))?;

            if !root.is_dir {
                return Ok(FileInfo {
                    filename: <Self as ProviderDefaults>::safe_filename(&root.filename, "arquivo_terabox"),
                    size: root.size,
                    mime_type: None,
                    is_folder: false,
                    children: None,
                });
            }

            // Se o URL aponta para uma subpasta (path=/CDZ), navega direto para ela
            let (entry_path, entry_fsid) = if let Some(sub_path) = Self::extract_dir(url) {
                // sub_path é algo como "/CDZ" — o full path seria root.path + sub_path
                let full_path = format!("{}{}", root.path.trim_end_matches('/'), sub_path);
                // Busca a entrada no root que corresponde ao nome da subpasta
                let sub_name = sub_path.trim_matches('/').split('/').next().unwrap_or("");
                let sub_entry = root_items.iter()
                    .find(|e| e.filename.eq_ignore_ascii_case(sub_name) || e.path.ends_with(sub_name));
                if let Some(sub) = sub_entry {
                    (sub.path.clone(), sub.fs_id.clone())
                } else {
                    // Tenta direto com o full_path
                    (full_path, root.fs_id.clone())
                }
            } else {
                (root.path.clone(), root.fs_id.clone())
            };

            let files = Self::collect_folder_files(
                &client,
                &ctx,
                entry_path.clone(),
                entry_fsid,
            ).await?;

            let children = files
                .iter()
                .map(|entry| {
                    let relative_path = Self::relative_path(&entry_path, &entry.path);
                    Self::entry_to_child(url, entry, relative_path)
                })
                .collect::<Vec<_>>();
            let total_size = children.iter().map(|child| child.size).sum();

            let folder_name = entry_path
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .filter(|s| !s.is_empty())
                .unwrap_or(&root.filename);
            Ok(FileInfo {
                filename: <Self as ProviderDefaults>::safe_filename(folder_name, "pasta_terabox"),
                size: total_size,
                mime_type: None,
                is_folder: true,
                children: Some(children),
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

            if url.contains("/sharing/videoPlay") {
                let (ctx, entry) = Self::resolve_file_entry(&client, url).await?;
                let json = Self::request_download_json(&client, &ctx, &entry.fs_id, url).await?;
                if json["errno"].as_i64().unwrap_or(-1) != 0 {
                    return Err(Self::terabox_download_error(&json));
                }
                let direct_url = Self::extract_download_url(&json)
                    .ok_or_else(|| anyhow!("TeraBox não retornou dlink para o arquivo"))?;
                let existing = tokio::fs::metadata(dest_path).await.ok().map(|m| m.len()).unwrap_or(0);
                let (resp, resumed) = <Self as ProviderDefaults>::send_with_resume_fallback(&client, &direct_url, dest_path, existing).await?;
                let total = if resumed {
                    <Self as ProviderDefaults>::response_total_bytes(&resp, existing)
                } else {
                    resp.content_length().unwrap_or(entry.size)
                };
                let mut file = if resumed {
                    OpenOptions::new().create(true).append(true).open(dest_path).await?
                } else {
                    tokio::fs::File::create(dest_path).await?
                };
                let mut stream = resp.bytes_stream();
                let mut downloaded = if resumed { existing } else { 0 };
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

            let info = self.get_file_info(url).await?;
            Err(anyhow!(
                "TeraBox já lista esta pasta corretamente, mas o download em lote ainda esbarra na verificação pública verify_v2 do host: {} ({} arquivo(s))",
                info.filename,
                info.children.as_ref().map(|c| c.len()).unwrap_or(0)
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::TeraboxProvider;

    #[test]
    fn extracts_surl() {
        let url = "https://www.1024tera.com/portuguese/sharing/link?surl=7ztIK8tA1cPr03ELh563Rg";
        assert_eq!(
            TeraboxProvider::extract_surl(url).as_deref(),
            Some("7ztIK8tA1cPr03ELh563Rg")
        );
    }

    #[test]
    fn extracts_video_dir_and_fsid() {
        let url = "https://www.1024tera.com/sharing/videoPlay?surl=abc&dir=/Dragon+Ball+Z/Medio&fsid=123&fileName=x";
        assert_eq!(TeraboxProvider::extract_dir(url).as_deref(), Some("/Dragon Ball Z/Medio"));
        assert_eq!(TeraboxProvider::extract_fsid(url).as_deref(), Some("123"));
    }
}
