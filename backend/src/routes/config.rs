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
