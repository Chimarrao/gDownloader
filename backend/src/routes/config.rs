use axum::{http::StatusCode, Json};
use serde::Deserialize;

use crate::{models::ApiError, ws::AppState};

use super::downloads::schedule_pending_downloads;

#[derive(Debug, Deserialize)]
pub struct DownloadConfigRequest {
    pub max_concurrent_downloads: Option<usize>,
}

pub async fn update_download_config(
    axum::extract::State(state): axum::extract::State<AppState>,
    Json(req): Json<DownloadConfigRequest>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let next_limit = req.max_concurrent_downloads.unwrap_or(1).max(1);
    *state.max_concurrent_downloads.lock().await = next_limit;
    schedule_pending_downloads(state).await;
    Ok(StatusCode::NO_CONTENT)
}
