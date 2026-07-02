use axum::{
    extract::{Form, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use base64::{engine::general_purpose, Engine as _};
use regex::Regex;
use serde::Deserialize;
use tracing::{info, warn};

use crate::{
    models::AddDownloadRequest,
    routes::downloads::add_download_internal,
    ws::AppState,
};

#[derive(Debug, Deserialize, Default)]
pub struct ClickNLoadForm {
    pub urls: Option<String>,
    pub url: Option<String>,
    pub source: Option<String>,
    pub password: Option<String>,
    pub crypted: Option<String>,
    pub jk: Option<String>,
    pub source_url: Option<String>,
}

pub async fn flash_add(
    State(state): State<AppState>,
    Form(form): Form<ClickNLoadForm>,
) -> Response {
    handle_clicknload("flash/add", state, form).await
}

pub async fn flash_addcrypted(
    State(state): State<AppState>,
    Form(form): Form<ClickNLoadForm>,
) -> Response {
    handle_clicknload("flash/addcrypted", state, form).await
}

pub async fn flash_addcrypted2(
    State(state): State<AppState>,
    Form(form): Form<ClickNLoadForm>,
) -> Response {
    handle_clicknload("flash/addcrypted2", state, form).await
}

pub async fn jdcheck() -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/javascript; charset=utf-8"));
    headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
    (
        headers,
        "jdownloader=true;\nvar jdownloader=true;\nvar jd=true;\n",
    )
        .into_response()
}

async fn handle_clicknload(kind: &'static str, state: AppState, form: ClickNLoadForm) -> Response {
    let urls = extract_urls_from_form(&form);
    if urls.is_empty() {
        warn!(
            target: "gdownloader_backend::clicknload",
            "CnL request sem URLs kind={} source={:?} has_crypted={} has_jk={}",
            kind,
            form.source.as_ref().or(form.source_url.as_ref()),
            form.crypted.is_some(),
            form.jk.is_some()
        );
        return text_response(
            StatusCode::BAD_REQUEST,
            "Nenhuma URL reconhecida no payload Click'n'Load.",
        );
    }

    let defaults = default_download_options(&state);
    let mut added = 0usize;
    let mut failed = Vec::new();

    for url in urls {
        let req = AddDownloadRequest {
            url: url.clone(),
            dest_dir: defaults.dest_dir.clone(),
            max_retries: Some(defaults.max_retries),
            speed_limit_kib: Some(defaults.speed_limit_kib),
            parallel_parts: Some(defaults.parallel_parts),
            selected_children: None,
            expected_hash: None,
            priority: None,
            duplicate_action: None,
            request_headers: None,
            auto_tor_on_limit: None,
        };

        match add_download_internal(state.clone(), req).await {
            Ok(download) => {
                added += 1;
                info!(
                    target: "gdownloader_backend::clicknload",
                    "CnL adicionou download kind={} id={} provider={} url={}",
                    kind,
                    download.id,
                    download.provider,
                    url
                );
            }
            Err((_status, body)) => {
                failed.push(format!("{} ({})", url, body.error));
            }
        }
    }

    if added > 0 {
        let mut body = format!("success\nadded={added}");
        if !failed.is_empty() {
            body.push_str(&format!("\nfailed={}", failed.len()));
        }
        return text_response(StatusCode::OK, &body);
    }

    text_response(
        StatusCode::BAD_REQUEST,
        &format!("Nenhuma URL pôde ser adicionada.\n{}", failed.join("\n")),
    )
}

struct CnlDefaults {
    dest_dir: String,
    max_retries: u32,
    speed_limit_kib: u64,
    parallel_parts: u32,
}

fn default_download_options(state: &AppState) -> CnlDefaults {
    let settings = state
        .db
        .lock()
        .ok()
        .and_then(|db| crate::db::load_public_settings(&db).ok())
        .unwrap_or_default();

    CnlDefaults {
        dest_dir: settings.output_dir,
        max_retries: settings.max_retries_per_download.saturating_sub(1),
        speed_limit_kib: settings.speed_limit_kib,
        parallel_parts: settings.parallel_parts_per_download.max(1),
    }
}

fn extract_urls_from_form(form: &ClickNLoadForm) -> Vec<String> {
    let mut sources = Vec::new();
    for value in [
        form.urls.as_deref(),
        form.url.as_deref(),
        form.crypted.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        sources.push(value.to_string());
    }

    if let Some(decoded) = form
        .crypted
        .as_deref()
        .and_then(|value| general_purpose::STANDARD.decode(value.trim()).ok())
        .and_then(|bytes| String::from_utf8(bytes).ok())
    {
        sources.push(decoded);
    }

    let mut seen = std::collections::HashSet::new();
    sources
        .iter()
        .flat_map(|value| extract_urls(value))
        .filter(|url| seen.insert(url.clone()))
        .collect()
}

fn extract_urls(value: &str) -> Vec<String> {
    let Some(re) = Regex::new(r#"https?://[^\s"'<>\\]+"#).ok() else {
        return Vec::new();
    };
    re.find_iter(value)
        .map(|match_| {
            match_
                .as_str()
                .trim_end_matches(|ch: char| matches!(ch, ')' | ']' | ',' | ';' | '.'))
                .to_string()
        })
        .collect()
}

fn text_response(status: StatusCode, body: &str) -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/plain; charset=utf-8"));
    headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
    (status, headers, body.to_string()).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_urls_from_plain_and_base64_payloads() {
        let encoded = general_purpose::STANDARD.encode("https://example.com/a.bin\nhttps://host.test/b.rar");
        let form = ClickNLoadForm {
            urls: Some("https://example.org/c.mkv".to_string()),
            crypted: Some(encoded),
            ..ClickNLoadForm::default()
        };

        let urls = extract_urls_from_form(&form);
        assert_eq!(urls.len(), 3);
        assert!(urls.contains(&"https://example.com/a.bin".to_string()));
        assert!(urls.contains(&"https://host.test/b.rar".to_string()));
        assert!(urls.contains(&"https://example.org/c.mkv".to_string()));
    }
}
