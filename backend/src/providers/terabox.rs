use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::Path as FsPath;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio::time::{sleep, Duration};

use crate::models::{FileChildInfo, FileInfo};
use super::{apply_speed_limit, host_matches, path_segments, ProgressUpdate, Provider, ProviderDefaults};

/// Porta do proxy local Electron para chamadas Terabox com cookies de sessão browser.
fn electron_proxy_port() -> Option<u16> {
    std::env::var("TERABOX_PROXY_PORT")
        .ok()
        .and_then(|s| s.parse::<u16>().ok())
        .filter(|&p| p > 0)
}

/// Faz GET via proxy Electron (usa sessão persist:terabox com cookies reais).
/// Se o proxy não estiver disponível, retorna None e o caller usa reqwest diretamente.
async fn proxy_get(url: &str, referer: &str) -> Option<Value> {
    let port = electron_proxy_port()?;
    let proxy_url = format!("http://127.0.0.1:{port}/");
    let body = serde_json::json!({
        "url": url,
        "method": "GET",
        "headers": { "Referer": referer }
    });
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(20)).build().ok()?;
    let resp = client.post(&proxy_url).json(&body).send().await.ok()?;
    resp.json::<Value>().await.ok()
}

async fn proxy_action(payload: Value) -> Result<Value> {
    let port = electron_proxy_port().ok_or_else(|| anyhow!("Proxy local do TeraBox não disponível"))?;
    let proxy_url = format!("http://127.0.0.1:{port}/");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(180))
        .build()?;
    let resp = client.post(&proxy_url).json(&payload).send().await?;
    Ok(resp.json::<Value>().await?)
}

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
    sekey: String,       // randsk from shorturlinfo — used as sekey in share/list
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
struct HelperJobStatus {
    status: String,
    #[serde(default, rename = "bytesDownloaded")]
    bytes_downloaded: u64,
    #[serde(default, rename = "totalBytes")]
    total_bytes: u64,
    #[serde(default, rename = "speedBps")]
    speed_bps: u64,
    #[serde(default, rename = "etaSecs")]
    eta_secs: u64,
    error: Option<String>,
}

impl TeraboxProvider {
    pub fn matches(url: &str) -> bool {
        host_matches(
            url,
            &[
                "1024tera.com",
                "www.1024tera.com",
                "terabox.com",
                "www.terabox.com",
                "terabox.app",
                "www.terabox.app",
            ],
        ) && {
            let segments = path_segments(url);
            segments
                .windows(2)
                .any(|parts| parts[0] == "sharing" && (parts[1] == "link" || parts[1] == "videoPlay"))
        }
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
        let db_path = std::env::var("GDOWNLOADER_DB_PATH").ok()?;
        let settings = crate::db::load_secure_settings_from_path(&db_path).ok()?;
        let cookies = settings.terabox_account?.cookies;
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
        // randsk (URL-encoded) from shorturlinfo is used as sekey in share/list
        let sekey = info["randsk"].as_str()
            .map(|s| urlencoding::decode(s).unwrap_or(std::borrow::Cow::Borrowed(s)).into_owned())
            .unwrap_or_default();
        Ok(ShareContext {
            js_token,
            surl,
            shareid: info["shareid"].as_i64().ok_or_else(|| anyhow!("Resposta do TeraBox sem shareid"))?,
            uk: info["uk"].as_i64().ok_or_else(|| anyhow!("Resposta do TeraBox sem uk"))?,
            sign: info["sign"].as_str().unwrap_or_default().to_string(),
            timestamp: info["timestamp"].as_i64().ok_or_else(|| anyhow!("Resposta do TeraBox sem timestamp"))?,
            sekey,
            host: Self::api_host(url),
        })
    }

    fn api_host(url: &str) -> &'static str {
        if host_matches(url, &["terabox.com", "www.terabox.com", "terabox.app", "www.terabox.app"]) {
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
        let host = Self::api_host(origin_url);
        let shorturl_candidates = if surl.starts_with('1') {
            vec![surl.to_string(), surl.trim_start_matches('1').to_string()]
        } else {
            vec![format!("1{surl}"), surl.to_string()]
        };

        let mut last_error = None;
        for shorturl in shorturl_candidates {
            if shorturl.is_empty() {
                continue;
            }
            let logid = Self::make_logid();
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
                        ("shorturl", shorturl.as_str()),
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
            if errno == 0 {
                return Ok(json);
            }
            if errno == 400210 || errmsg.contains("verify_v2") {
                return Err(anyhow!(
                    "O compartilhamento público do TeraBox exigiu verificação adicional do host antes de liberar a leitura."
                ));
            }
            last_error = Some(format!(
                "TeraBox falhou ao ler o compartilhamento (errno {errno}{}{})",
                if errmsg.is_empty() { "" } else { ": " },
                errmsg
            ));
        }

        Err(anyhow!(last_error.unwrap_or_else(|| "TeraBox não aceitou o código do compartilhamento".to_string())))
    }

    fn build_share_list_url(ctx: &ShareContext, dir: Option<&str>, fid: Option<&str>, page: usize, sekey: Option<&str>) -> String {
        let mut params = format!(
            "app_id={APP_ID}&web=1&channel={CHANNEL}&clienttype=0&jsToken={}&dp-logid={}&page={page}&num=100&by=name&order=asc&site_referer=&shorturl={}",
            ctx.js_token, Self::make_logid(), ctx.surl
        );
        if let Some(d) = dir { params.push_str(&format!("&dir={}", urlencoding::encode(d))); }
        if let Some(f) = fid { params.push_str(&format!("&fid={f}")); }
        if dir.is_none() && fid.is_none() { params.push_str("&root=1"); }
        if let Some(sk) = sekey { params.push_str(&format!("&sekey={}", urlencoding::encode(sk))); }
        format!("{}/share/list?{}", ctx.host, params)
    }

    async fn fetch_share_list(
        client: &reqwest::Client,
        ctx: &ShareContext,
        dir: Option<&str>,
        fid: Option<&str>,
        page: usize,
    ) -> Result<Vec<ShareEntry>> {
        let referer = format!("{}/sharing/link?surl={}", ctx.host, ctx.surl);
        let url = Self::build_share_list_url(ctx, dir, fid, page, Some(&ctx.sekey));

        // 1st try: via Electron proxy (browser session with real cookies)
        let json = if let Some(proxied) = proxy_get(&url, &referer).await {
            proxied
        } else {
            // Fallback: direct reqwest
            let mut req = client
                .get(format!("{}/share/list", ctx.host))
                .query(&[
                    ("app_id", APP_ID), ("web", "1"), ("channel", CHANNEL), ("clienttype", "0"),
                    ("jsToken", ctx.js_token.as_str()), ("dp-logid", &Self::make_logid()),
                    ("page", &page.to_string()), ("num", "100"), ("by", "name"),
                    ("order", "asc"), ("site_referer", ""), ("shorturl", ctx.surl.as_str()),
                    ("sekey", ctx.sekey.as_str()),
                ]);
            if let Some(f) = fid { req = req.query(&[("fid", f)]); }
            if let Some(d) = dir { req = req.query(&[("dir", d)]); } else { req = req.query(&[("root", "1")]); }
            Self::with_saved_cookies(req).send().await?.error_for_status()?.json().await?
        };

        let errno = json["errno"]
            .as_i64()
            .or_else(|| json["code"].as_i64())
            .unwrap_or(-1);
        let errmsg = json["errmsg"].as_str().unwrap_or("");
        if errno != 0 {
            if errno == 460020 || errno == 400141 || errmsg.contains("need verify") {
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

        Self::parse_share_entries(&json)
    }

    fn parse_share_entries(json: &Value) -> Result<Vec<ShareEntry>> {
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

    async fn helper_start_download(source_url: &str, dest_path: &str) -> Result<String> {
        let json = proxy_action(json!({
            "action": "terabox_download_file",
            "url": source_url,
            "destPath": dest_path,
        })).await?;

        json["jobId"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| anyhow!("Helper local do TeraBox não retornou um jobId"))
    }

    async fn helper_job_status(job_id: &str) -> Result<HelperJobStatus> {
        let json = proxy_action(json!({
            "action": "terabox_job_status",
            "jobId": job_id,
        })).await?;

        Ok(serde_json::from_value::<HelperJobStatus>(json)?)
    }

    async fn helper_file_info(url: &str) -> Result<FileInfo> {
        let json = proxy_action(json!({
            "action": "terabox_file_info",
            "url": url,
        })).await?;

        Ok(serde_json::from_value::<FileInfo>(json)?)
    }

    fn child_output_path(dest_path: &str, child: &FileChildInfo) -> String {
        let relative = child.path.clone().unwrap_or_else(|| child.filename.clone());
        format!("{}/{}", dest_path.trim_end_matches('/'), relative)
    }

    async fn download_via_helper(
        source_url: &str,
        dest_path: &str,
        expected_size: u64,
        progress_tx: tokio::sync::mpsc::Sender<ProgressUpdate>,
        total_bytes: u64,
        completed_bytes: u64,
        child: Option<&FileChildInfo>,
    ) -> Result<u64> {
        if let Some(parent) = FsPath::new(dest_path).parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let existing_bytes = tokio::fs::metadata(dest_path)
            .await
            .ok()
            .filter(|meta| meta.is_file())
            .map(|meta| meta.len())
            .unwrap_or(0);

        if expected_size > 0 && existing_bytes >= expected_size {
            let child_path = child.and_then(|item| item.path.clone());
            let child_filename = child.map(|item| item.filename.clone());
            let _ = progress_tx
                .send(ProgressUpdate {
                    bytes_downloaded: completed_bytes + expected_size,
                    total_bytes,
                    child_path,
                    child_filename,
                    child_bytes_downloaded: child.map(|_| expected_size),
                    child_total_bytes: child.map(|_| expected_size),
                    child_speed_bps: child.map(|_| 0),
                    child_eta_secs: child.map(|_| 0),
                })
                .await;
            return Ok(expected_size);
        }

        let job_id = Self::helper_start_download(source_url, dest_path).await?;

        loop {
            sleep(Duration::from_millis(500)).await;
            let status = Self::helper_job_status(&job_id).await?;
            let child_total = if expected_size > 0 {
                expected_size
            } else {
                status.total_bytes
            };
            let child_downloaded = status.bytes_downloaded.min(child_total.max(status.bytes_downloaded));
            let bytes_downloaded = completed_bytes + child_downloaded;
            let child_path = child.and_then(|item| item.path.clone());
            let child_filename = child.map(|item| item.filename.clone());
            let child_speed = child.map(|_| status.speed_bps);
            let child_eta = child.map(|_| status.eta_secs);

            let _ = progress_tx
                .send(ProgressUpdate {
                    bytes_downloaded,
                    total_bytes,
                    child_path,
                    child_filename,
                    child_bytes_downloaded: child.map(|_| child_downloaded),
                    child_total_bytes: child.map(|_| child_total),
                    child_speed_bps: child_speed,
                    child_eta_secs: child_eta,
                })
                .await;

            match status.status.as_str() {
                "pending" | "downloading" => continue,
                "complete" => {
                    return Ok(child_total.max(status.bytes_downloaded));
                }
                "cancelled" => {
                    return Err(anyhow!("O navegador do TeraBox cancelou este download."));
                }
                "error" => {
                    return Err(anyhow!(
                        "{}",
                        status
                            .error
                            .unwrap_or_else(|| "O helper local do TeraBox falhou.".to_string())
                    ));
                }
                other => {
                    return Err(anyhow!("Status inesperado do helper do TeraBox: {other}"));
                }
            }
        }
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
            if Self::matches(url) && path_segments(url).windows(2).any(|parts| parts[0] == "sharing" && parts[1] == "link") && electron_proxy_port().is_some() {
                let mut info = Self::helper_file_info(url).await?;
                let fallback = if info.is_folder {
                    "pasta_terabox"
                } else {
                    "arquivo_terabox"
                };
                info.filename = <Self as ProviderDefaults>::safe_filename(&info.filename, fallback);
                return Ok(info);
            }

            let client = <Self as ProviderDefaults>::http_client()?;

            if url.contains("/sharing/videoPlay") {
                let (_, entry) = Self::resolve_file_entry(&client, url).await?;
                return Ok(FileInfo {
                    filename: <Self as ProviderDefaults>::safe_filename(&entry.filename, "arquivo_terabox"),
                    size: entry.size,
                    mime_type: None,
                    is_folder: false,
                    children: None,
                    ..Default::default()
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
                    ..Default::default()
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

            if url.contains("/sharing/videoPlay") {
                let (ctx, entry) = Self::resolve_file_entry(&client, url).await?;
                if entry.category.as_deref() == Some("1") {
                    return Self::download_via_helper(
                        url,
                        dest_path,
                        entry.size,
                        progress_tx,
                        entry.size,
                        0,
                        None,
                    ).await;
                }

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
            if !info.is_folder {
                return Err(anyhow!("O TeraBox não retornou uma pasta válida para este link."));
            }

            let mut children = info.children.unwrap_or_default();
            if let Some(selected) = selected_children {
                let selected_set = selected.into_iter().collect::<std::collections::HashSet<_>>();
                children.retain(|child| {
                    child
                        .source_url
                        .as_ref()
                        .map(|source_url| selected_set.contains(source_url))
                        .unwrap_or(false)
                });
            }

            if children.is_empty() {
                return Err(anyhow!("Pasta do TeraBox vazia ou sem arquivos selecionáveis."));
            }

            tokio::fs::create_dir_all(dest_path).await?;
            let total_size: u64 = children.iter().map(|child| child.size).sum();
            let mut downloaded_total = 0u64;

            for child in &children {
                let source_url = child
                    .source_url
                    .as_ref()
                    .ok_or_else(|| anyhow!("Arquivo do TeraBox sem URL individual utilizável"))?;
                let output_path = Self::child_output_path(dest_path, child);
                let downloaded = Self::download_via_helper(
                    source_url,
                    &output_path,
                    child.size,
                    progress_tx.clone(),
                    total_size,
                    downloaded_total,
                    Some(child),
                ).await?;
                downloaded_total += downloaded.max(child.size);
            }

            Ok(downloaded_total)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{ShareEntry, TeraboxProvider};

    #[test]
    fn extracts_surl() {
        let url = "https://www.terabox.app/portuguese/sharing/link?surl=7ztIK8tA1cPr03ELh563Rg";
        assert!(TeraboxProvider::matches(url));
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

    #[test]
    fn parses_folder_entries_from_fixture_json() {
        let json = serde_json::from_str::<serde_json::Value>(include_str!(
            "../../tests/fixtures/providers/terabox_share_list.json"
        ))
        .expect("fixture json válido");

        let entries = TeraboxProvider::parse_share_entries(&json).expect("deve parsear");
        assert_eq!(entries.len(), 2);
        assert!(entries[0].is_dir);
        assert_eq!(entries[1].size, 2147483648);
        assert_eq!(
            entries[1].filename,
            "[DarthinURMZ]Saint Seiya(Hexa)EP 01 1080p V2.mkv"
        );
    }

    #[test]
    fn builds_child_path_for_video_entry() {
        let entry = ShareEntry {
            fs_id: "71002".to_string(),
            filename: "[DarthinURMZ]Saint Seiya(Hexa)EP 01 1080p V2.mkv".to_string(),
            path: "/CDZ/Maior/[DarthinURMZ]Saint Seiya(Hexa)EP 01 1080p V2.mkv".to_string(),
            size: 2147483648,
            is_dir: false,
            category: Some("1".to_string()),
        };
        let child = TeraboxProvider::entry_to_child(
            "https://www.terabox.com/sharing/link?surl=abcdef",
            &entry,
            "Maior/[DarthinURMZ]Saint Seiya(Hexa)EP 01 1080p V2.mkv".to_string(),
        );

        assert_eq!(child.path.as_deref(), Some("Maior/[DarthinURMZ]Saint Seiya(Hexa)EP 01 1080p V2.mkv"));
        assert!(child
            .source_url
            .as_deref()
            .unwrap_or_default()
            .contains("/sharing/videoPlay?surl=abcdef"));
    }
}
