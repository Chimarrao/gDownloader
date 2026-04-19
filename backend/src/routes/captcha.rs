use axum::{
    extract::{Query, State},
    response::Html,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::models::DownloadStatus;
use crate::ws::AppState;

#[derive(Deserialize)]
pub struct CaptchaQuery {
    pub r#type: String,   // "recaptcha2" | "hcaptcha"
    pub sitekey: String,
    pub pageurl: String,
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
    function onSolved(token) {{
      window.parent.postMessage({{ type: 'captcha-token', token }}, '*');
    }}
  </script>
  <script src="{script_url}" async defer></script>
</body>
</html>"#
    ))
}

#[derive(Deserialize)]
pub struct SubmitCaptchaBody {
    pub download_id: String,
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
    let mut downloads = state.downloads.lock().await;
    if let Some(d) = downloads.get_mut(&body.download_id) {
        d.captcha_token = Some(body.token);
        d.status = DownloadStatus::Pending;
        d.captcha_type = None;
        d.captcha_sitekey = None;
        d.captcha_page_url = None;
    }
    Json(SubmitCaptchaResponse { ok: true })
}
