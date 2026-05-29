use axum::{http::StatusCode, Json};
use serde::Deserialize;

use crate::{
    models::{ApiError, LegacyConfigMigration, PublicSettings, SecureSettings},
    ws::AppState,
};

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

#[derive(serde::Serialize)]
pub struct TestProxyResponse {
    pub ip: String,
    #[serde(rename = "isTor")]
    pub is_tor: bool,
}

pub async fn test_proxy(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> Result<Json<TestProxyResponse>, (StatusCode, Json<ApiError>)> {
    let settings = state
        .db
        .lock()
        .ok()
        .and_then(|db| crate::db::load_public_settings(&db).ok())
        .unwrap_or_default();
    let client = <crate::providers::direct_http::DirectHttpProvider as crate::providers::ProviderDefaults>::http_client_with_proxy(
        &settings.proxy_mode,
        &settings.proxy_host,
        settings.proxy_port,
        settings.proxy_username.as_deref(),
        settings.proxy_password.as_deref(),
    )
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new(format!("Failed to create client: {}", e))),
        )
    })?;
    let response = client
        .get("https://check.torproject.org/api/ip")
        .send()
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::new(format!("Request failed: {}", e))),
            )
        })?;
    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::new(format!("Parse failed: {}", e))),
            )
        })?;
    let ip = json["IP"]
        .as_str()
        .or_else(|| json["origin"].as_str())
        .unwrap_or("unknown")
        .to_string();
    let is_tor = json["IsTor"].as_bool().unwrap_or(false);
    Ok(Json(TestProxyResponse { ip, is_tor }))
}

pub async fn get_secure_settings(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> Json<SecureSettings> {
    let settings = state
        .db
        .lock()
        .ok()
        .and_then(|db| crate::db::load_secure_settings(&db).ok())
        .unwrap_or_default();
    Json(settings)
}

pub async fn get_public_settings(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> Json<PublicSettings> {
    let settings = state
        .db
        .lock()
        .ok()
        .and_then(|db| crate::db::load_public_settings(&db).ok())
        .unwrap_or_default();
    Json(settings)
}

pub async fn get_legacy_config_migrations(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> Json<Vec<LegacyConfigMigration>> {
    let items = state
        .db
        .lock()
        .ok()
        .and_then(|db| crate::db::load_legacy_config_migrations(&db).ok())
        .unwrap_or_default();
    Json(items)
}

pub async fn update_public_settings(
    axum::extract::State(state): axum::extract::State<AppState>,
    Json(req): Json<PublicSettings>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    {
        let Ok(db) = state.db.lock() else {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::new("Falha ao abrir o banco local")),
            ));
        };

        crate::db::save_public_settings(&db, &req).map_err(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::new(format!(
                    "Falha ao salvar as configurações locais: {error}"
                ))),
            )
        })?;
    }

    crate::providers::update_global_proxy(
        req.proxy_mode.clone(),
        req.proxy_host.clone(),
        req.proxy_port,
        req.proxy_username.clone(),
        req.proxy_password.clone(),
    );

    *state.max_concurrent_downloads.lock().await = req.max_concurrent_downloads.max(1);
    schedule_pending_downloads(state).await;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn update_secure_settings(
    axum::extract::State(state): axum::extract::State<AppState>,
    Json(req): Json<SecureSettings>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let Ok(db) = state.db.lock() else {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("Falha ao abrir o banco local")),
        ));
    };

    crate::db::save_secure_settings(&db, &req).map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new(format!(
                "Falha ao salvar as credenciais locais: {error}"
            ))),
        )
    })?;

    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct LegacyMigrationRequest {
    pub version: i64,
    pub name: String,
}

pub async fn record_legacy_config_migration(
    axum::extract::State(state): axum::extract::State<AppState>,
    Json(req): Json<LegacyMigrationRequest>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let Ok(db) = state.db.lock() else {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("Falha ao abrir o banco local")),
        ));
    };

    crate::db::mark_legacy_config_migration(&db, req.version, &req.name).map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new(format!(
                "Falha ao registrar migração legada: {error}"
            ))),
        )
    })?;

    Ok(StatusCode::NO_CONTENT)
}
