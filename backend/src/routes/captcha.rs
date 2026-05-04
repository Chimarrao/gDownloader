use axum::{
    extract::{Query, State},
    response::Html,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::models::DownloadStatus;
use crate::ws::AppState;
use super::downloads::schedule_pending_downloads;

#[derive(Deserialize)]
pub struct CaptchaQuery {
    pub r#type: String,   // "recaptcha2" | "hcaptcha"
    pub sitekey: String,
    pub pageurl: String,
    pub id: String,       // download ID — echoed back in postMessage
}

/// Serve uma página HTML mínima com apenas o widget de captcha.
/// O frontend carrega esta URL em um iframe.
pub async fn captcha_page(Query(params): Query<CaptchaQuery>) -> Html<String> {
    let (script_url, widget_html) = match params.r#type.as_str() {
        "hcaptcha" => (
            "https://js.hcaptcha.com/1/api.js".to_string(),
            format!(
                r#"<div class="h-captcha" data-sitekey="{}" data-callback="onSolved"></div>"#,
                params.sitekey
            ),
        ),
        _ => (
            // recaptcha2 default
            "https://www.google.com/recaptcha/api.js".to_string(),
            format!(
                r#"<div class="g-recaptcha" data-sitekey="{}" data-callback="onSolved"></div>"#,
                params.sitekey
            ),
        ),
    };

    let download_id = &params.id;
    Html(format!(
        r#"<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8">
  <style>
    * {{ margin: 0; padding: 0; box-sizing: border-box; }}
    body {{
      background: transparent;
      display: flex;
      justify-content: center;
      padding: 8px;
    }}
  </style>
</head>
<body>
  {widget_html}
  <script>
    var DOWNLOAD_ID = "{download_id}";
    function onSolved(token) {{
      window.parent.postMessage({{ type: 'captcha-token', id: DOWNLOAD_ID, token: token }}, '*');
    }}
  </script>
  <script src="{script_url}" async defer></script>
</body>
</html>"#
    ))
}

#[derive(Deserialize)]
pub struct SubmitCaptchaBody {
    pub id: String,
    pub token: String,
}

#[derive(Serialize)]
pub struct SubmitCaptchaResponse {
    pub ok: bool,
}

/// Recebe o token resolvido e re-enfileira o download.
pub async fn submit_captcha(
    State(state): State<AppState>,
    Json(body): Json<SubmitCaptchaBody>,
) -> Json<SubmitCaptchaResponse> {
    let snapshot = {
        let mut downloads = state.downloads.lock().await;
        if let Some(d) = downloads.get_mut(&body.id) {
            d.captcha_token = Some(body.token);
            d.status = DownloadStatus::Pending;
            d.captcha_type = None;
            d.captcha_sitekey = None;
            d.captcha_page_url = None;
            d.error = None;
            d.retry_at = None;
            d.speed_bps = 0;
            d.eta_secs = 0;
            Some(d.clone())
        } else {
            None
        }
    };

    if let Some(download) = snapshot {
        if let Ok(db) = state.db.lock() {
            let _ = crate::db::upsert(&db, &download);
        }
        schedule_pending_downloads(state).await;
    }

    Json(SubmitCaptchaResponse { ok: true })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        db,
        models::{Download, DownloadStatus},
        ws::AppState,
    };

    fn sample_download() -> Download {
        Download {
            id: "download-1".to_string(),
            url: "https://example.com/file".to_string(),
            provider: "Example".to_string(),
            identity_key: "example::https://example.com/file".to_string(),
            filename: "file.bin".to_string(),
            size: 1234,
            dest_path: "/tmp/file.bin".to_string(),
            status: DownloadStatus::WaitingCaptcha,
            bytes_downloaded: 0,
            speed_bps: 0,
            eta_secs: 0,
            is_folder: false,
            children: None,
            retry_count: 0,
            max_retries: 3,
            speed_limit_kib: 0,
            parallel_parts: 1,
            selected_children: None,
            expected_hash: None,
            retry_at: Some(999),
            captcha_type: Some("recaptcha2".to_string()),
            captcha_sitekey: Some("sitekey".to_string()),
            captcha_page_url: Some("https://example.com/captcha".to_string()),
            captcha_token: None,
            error: Some("captcha".to_string()),
            priority: 0,
            created_at: 1,
            started_at: None,
            completed_at: None,
            last_progress_at: None,
            pinned: false,
            package_id: None,
            request_headers: None,
        }
    }

    #[tokio::test]
    async fn submit_captcha_requeues_download_and_clears_metadata() {
        let db_path = std::env::temp_dir().join(format!(
            "gdownloader-captcha-test-{}.sqlite",
            uuid::Uuid::new_v4()
        ));
        let conn = db::init(db_path.to_string_lossy().as_ref()).unwrap();
        let state = AppState::new(conn);

        {
            let mut downloads = state.downloads.lock().await;
            downloads.insert("download-1".to_string(), sample_download());
        }

        let response = submit_captcha(
            axum::extract::State(state.clone()),
            Json(SubmitCaptchaBody {
                id: "download-1".to_string(),
                token: "token-123".to_string(),
            }),
        )
        .await;

        assert!(response.ok);

        let downloads = state.downloads.lock().await;
        let updated = downloads.get("download-1").unwrap();
        assert!(matches!(
            updated.status,
            DownloadStatus::Pending | DownloadStatus::Downloading
        ));
        assert_eq!(updated.captcha_token.as_deref(), Some("token-123"));
        assert!(updated.captcha_type.is_none());
        assert!(updated.captcha_sitekey.is_none());
        assert!(updated.captcha_page_url.is_none());
        assert!(updated.retry_at.is_none());
    }
}
