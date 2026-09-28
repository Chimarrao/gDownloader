use axum::{extract::State, Json};
use crate::ws::AppState;

pub async fn get_realtime_stats(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let stats = state.stats.get_realtime_stats().await;
    Json(serde_json::json!({ "ticks": stats }))
}
