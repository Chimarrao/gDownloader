use axum::{
    body::{to_bytes, Body, Bytes},
    extract::State,
    http::{HeaderMap, HeaderName, HeaderValue, Method, Request, Response, StatusCode, Uri},
    response::{Html, IntoResponse},
    routing::{any, get, post},
    Json, Router,
};
use reqwest::redirect::Policy;
use serde::Serialize;
use std::{collections::HashMap, path::PathBuf, time::Duration};
use uuid::Uuid;

use crate::{
    models::{AddDownloadRequest, ApiError, InterceptHistoryItem, InterceptRequest, PublicSettings},
    routes::downloads::add_download_internal,
    ws::AppState,
};

const PROXY_ADDR: &str = "127.0.0.1:9667";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InterceptStatus {
    pub enabled: bool,
    pub proxy_addr: String,
    pub ca_cert_path: String,
    pub history: Vec<InterceptHistoryItem>,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/intercept", post(add_intercept))
        .route("/intercept/status", get(status))
        .route("/intercept/history", get(history))
}

pub fn spawn_intercept_proxy_manager(state: AppState) {
    tokio::spawn(async move {
        let mut started = false;
        loop {
            let enabled = load_settings(&state).intercept_mode == "proxy_only";
            if enabled && !started {
                started = true;
                let proxy_state = state.clone();
                tokio::spawn(async move {
                    ensure_ca_files(&proxy_state).ok();
                    match tokio::net::TcpListener::bind(PROXY_ADDR).await {
                        Ok(listener) => {
                            tracing::info!("Intercept proxy rodando em http://{PROXY_ADDR}");
                            let app = Router::new()
                                .fallback(any(proxy_handler))
                                .with_state(proxy_state);
                            if let Err(error) = axum::serve(listener, app).await {
                                tracing::error!("Intercept proxy encerrou com erro: {error}");
                            }
                        }
                        Err(error) => {
                            tracing::warn!("Não foi possível abrir intercept proxy em {PROXY_ADDR}: {error}");
                        }
                    }
                });
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });
}

pub async fn status(State(state): State<AppState>) -> Json<InterceptStatus> {
    let settings = load_settings(&state);
    let history = load_history(&state);
    let ca_cert_path = ensure_ca_files(&state)
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_default();
    Json(InterceptStatus {
        enabled: settings.intercept_mode == "proxy_only",
        proxy_addr: PROXY_ADDR.to_string(),
        ca_cert_path,
        history,
    })
}

pub async fn history(State(state): State<AppState>) -> Json<Vec<InterceptHistoryItem>> {
    Json(load_history(&state))
}

pub async fn add_intercept(
    State(state): State<AppState>,
    Json(req): Json<InterceptRequest>,
) -> Result<Json<InterceptHistoryItem>, (StatusCode, Json<ApiError>)> {
    enqueue_intercept(state, req).await.map(Json)
}

async fn proxy_handler(State(state): State<AppState>, req: Request<Body>) -> impl IntoResponse {
    let settings = load_settings(&state);
    if settings.intercept_mode != "proxy_only" {
        return html_response(StatusCode::SERVICE_UNAVAILABLE, "Interceptação desativada");
    }
    if req.method() == Method::CONNECT {
        return html_response(
            StatusCode::NOT_IMPLEMENTED,
            "HTTPS CONNECT não é interceptado nesta camada. Instale a CA e use HTTP/HTTPS via proxy local quando suportado pelo app.",
        );
    }

    let Some(url) = absolute_url(req.uri(), req.headers()) else {
        return html_response(StatusCode::BAD_REQUEST, "URL absoluta inválida para proxy local");
    };
    if is_blocked(&url, &settings) {
        return proxy_pass(req, url).await;
    }

    let method = req.method().clone();
    let request_headers = capture_headers(req.headers());
    let body = to_bytes(req.into_body(), 8 * 1024 * 1024).await.unwrap_or_else(|_| Bytes::new());
    let client = match reqwest::Client::builder().redirect(Policy::limited(10)).build() {
        Ok(client) => client,
        Err(error) => return html_response(StatusCode::BAD_GATEWAY, &format!("Falha no proxy: {error}")),
    };
    let mut outbound = client.request(method.clone(), &url).headers(to_reqwest_headers(&request_headers));
    if !body.is_empty() {
        outbound = outbound.body(body);
    }
    let response = match outbound.send().await {
        Ok(response) => response,
        Err(error) => return html_response(StatusCode::BAD_GATEWAY, &format!("Falha ao acessar origem: {error}")),
    };

    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_string();
    let content_length = response.content_length().unwrap_or(0);
    let filename = response
        .headers()
        .get(reqwest::header::CONTENT_DISPOSITION)
        .and_then(|value| value.to_str().ok())
        .and_then(filename_from_content_disposition)
        .unwrap_or_else(|| filename_from_url(&url));
    let should_intercept = response
        .headers()
        .contains_key(reqwest::header::CONTENT_DISPOSITION)
        || mime_allowed(&content_type, &settings);
    let large_enough = content_length >= settings.intercept_min_size_mb.saturating_mul(1_048_576);

    if should_intercept && large_enough {
        let intercept = InterceptRequest {
            url: url.clone(),
            method: method.as_str().to_string(),
            headers: request_headers,
            content_type: Some(content_type),
            content_length: Some(content_length),
            filename: Some(filename),
            source: Some("proxy".to_string()),
        };
        return match enqueue_intercept(state, intercept).await {
            Ok(item) => html_response(
                StatusCode::OK,
                &format!("Download enviado para o gDownloader:<br><strong>{}</strong>", item.filename),
            ),
            Err((status, Json(error))) => html_response(status, &error.error),
        };
    }

    let status = response.status();
    let mut builder = Response::builder().status(status);
    for (name, value) in response.headers() {
        if let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(name.as_str().as_bytes()),
            HeaderValue::from_bytes(value.as_bytes()),
        ) {
            builder = builder.header(name, value);
        }
    }
    let bytes = response.bytes().await.unwrap_or_default();
    builder.body(Body::from(bytes)).unwrap_or_else(|_| Response::new(Body::empty()))
}

async fn proxy_pass(req: Request<Body>, url: String) -> Response<Body> {
    let method = req.method().clone();
    let headers = capture_headers(req.headers());
    let body = to_bytes(req.into_body(), 8 * 1024 * 1024).await.unwrap_or_else(|_| Bytes::new());
    let client = match reqwest::Client::builder().redirect(Policy::limited(10)).build() {
        Ok(client) => client,
        Err(error) => return text_response(StatusCode::BAD_GATEWAY, &format!("Falha no proxy: {error}")),
    };
    let mut outbound = client.request(method, &url).headers(to_reqwest_headers(&headers));
    if !body.is_empty() {
        outbound = outbound.body(body);
    }
    match outbound.send().await {
        Ok(response) => {
            let status = response.status();
            let bytes = response.bytes().await.unwrap_or_default();
            Response::builder()
                .status(status)
                .body(Body::from(bytes))
                .unwrap_or_else(|_| Response::new(Body::empty()))
        }
        Err(error) => text_response(StatusCode::BAD_GATEWAY, &format!("Falha ao acessar origem: {error}")),
    }
}

async fn enqueue_intercept(
    state: AppState,
    req: InterceptRequest,
) -> Result<InterceptHistoryItem, (StatusCode, Json<ApiError>)> {
    let settings = load_settings(&state);
    if is_blocked(&req.url, &settings) {
        return Err((StatusCode::FORBIDDEN, Json(ApiError::new("Domínio bloqueado nas exceções do interceptor"))));
    }
    let id = Uuid::new_v4().to_string();
    let filename = req.filename.clone().unwrap_or_else(|| filename_from_url(&req.url));
    let item = InterceptHistoryItem {
        id,
        url: req.url.clone(),
        filename,
        mime_type: req.content_type.clone().unwrap_or_default(),
        size: req.content_length.unwrap_or(0),
        status: "queued".to_string(),
        created_at: current_unix_secs(),
    };
    let add_req = AddDownloadRequest {
        url: req.url,
        dest_dir: settings.output_dir,
        max_retries: Some(settings.max_retries_per_download.saturating_sub(1)),
        speed_limit_kib: Some(settings.speed_limit_kib),
        parallel_parts: Some(settings.parallel_parts_per_download),
        selected_children: None,
        expected_hash: None,
        priority: Some(0),
        duplicate_action: Some(settings.duplicate_action),
        request_headers: Some(req.headers),
        auto_tor_on_limit: None,
    };
    add_download_internal(state.clone(), add_req).await?;
    if let Ok(db) = state.db.lock() {
        let _ = crate::db::insert_intercept_history(&db, &item);
    }
    Ok(item)
}

fn load_settings(state: &AppState) -> PublicSettings {
    state
        .db
        .lock()
        .ok()
        .and_then(|db| crate::db::load_public_settings(&db).ok())
        .unwrap_or_default()
}

fn load_history(state: &AppState) -> Vec<InterceptHistoryItem> {
    state
        .db
        .lock()
        .ok()
        .and_then(|db| crate::db::list_intercept_history(&db, 80).ok())
        .unwrap_or_default()
}

fn ensure_ca_files(state: &AppState) -> anyhow::Result<PathBuf> {
    let base = state
        .db_path
        .as_deref()
        .and_then(|path| std::path::Path::new(path).parent().map(|parent| parent.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("proxy-ca");
    std::fs::create_dir_all(&base)?;
    let cert_path = base.join("gdownloader-local-ca.pem");
    let key_path = base.join("gdownloader-local-ca-key.pem");
    if cert_path.exists() && key_path.exists() {
        return Ok(cert_path);
    }
    let certified = rcgen::generate_simple_self_signed(vec!["gDownloader Local Intercept".to_string()])?;
    std::fs::write(&cert_path, certified.cert.pem())?;
    std::fs::write(&key_path, certified.key_pair.serialize_pem())?;
    Ok(cert_path)
}

fn absolute_url(uri: &Uri, headers: &HeaderMap) -> Option<String> {
    let raw = uri.to_string();
    if raw.starts_with("http://") || raw.starts_with("https://") {
        return Some(raw);
    }
    let host = headers.get("host")?.to_str().ok()?;
    Some(format!("http://{host}{raw}"))
}

fn capture_headers(headers: &HeaderMap) -> HashMap<String, String> {
    headers
        .iter()
        .filter_map(|(name, value)| value.to_str().ok().map(|value| (name.as_str().to_string(), value.to_string())))
        .collect()
}

fn to_reqwest_headers(headers: &HashMap<String, String>) -> reqwest::header::HeaderMap {
    let mut map = reqwest::header::HeaderMap::new();
    for (name, value) in headers {
        let lower = name.to_ascii_lowercase();
        if matches!(lower.as_str(), "host" | "connection" | "content-length") {
            continue;
        }
        if let (Ok(name), Ok(value)) = (
            reqwest::header::HeaderName::from_bytes(lower.as_bytes()),
            reqwest::header::HeaderValue::from_str(value),
        ) {
            map.insert(name, value);
        }
    }
    map
}

fn mime_allowed(content_type: &str, settings: &PublicSettings) -> bool {
    let mime = content_type.to_ascii_lowercase();
    settings
        .intercept_mime_allowlist
        .iter()
        .any(|entry| mime.starts_with(&entry.to_ascii_lowercase()))
}

fn is_blocked(url: &str, settings: &PublicSettings) -> bool {
    let lower = url.to_ascii_lowercase();
    settings
        .intercept_domain_blocklist
        .iter()
        .map(|entry| entry.trim().to_ascii_lowercase())
        .filter(|entry| !entry.is_empty())
        .any(|entry| lower.contains(&entry))
}

fn filename_from_url(url: &str) -> String {
    url.split('?')
        .next()
        .unwrap_or(url)
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("download")
        .to_string()
}

fn filename_from_content_disposition(value: &str) -> Option<String> {
    value.split(';').map(str::trim).find_map(|part| {
        let lower = part.to_ascii_lowercase();
        if lower.starts_with("filename=") {
            part.split_once('=')
                .map(|(_, filename)| filename.trim().trim_matches('"').to_string())
                .filter(|filename| !filename.is_empty())
        } else {
            None
        }
    })
}

fn html_response(status: StatusCode, message: &str) -> Response<Body> {
    let html = format!(
        "<!doctype html><meta charset=\"utf-8\"><title>gDownloader</title><body style=\"font-family:system-ui;padding:32px\"><h1>gDownloader</h1><p>{message}</p></body>"
    );
    (status, Html(html)).into_response()
}

fn text_response(status: StatusCode, message: &str) -> Response<Body> {
    Response::builder()
        .status(status)
        .body(Body::from(message.to_string()))
        .unwrap_or_else(|_| Response::new(Body::empty()))
}

fn current_unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
