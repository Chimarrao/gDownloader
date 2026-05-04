use axum::{
    extract::State,
    http::StatusCode,
    Json,
};
use serde::Deserialize;

use crate::{
    models::{ApiError, ArchivePassword},
    ws::AppState,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PasswordRequest {
    pub password: String,
    pub source: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPasswordsRequest {
    pub passwords: Vec<String>,
    pub source: Option<String>,
}

pub async fn list_archive_passwords(
    State(state): State<AppState>,
) -> Result<Json<Vec<ArchivePassword>>, (StatusCode, Json<ApiError>)> {
    let db = state.db.lock().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("Falha ao abrir o banco local")),
        )
    })?;
    let items = crate::db::list_archive_passwords(&db, 500).map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new(format!("Falha ao listar senhas: {error}"))),
        )
    })?;
    Ok(Json(items))
}

pub async fn add_archive_password(
    State(state): State<AppState>,
    Json(req): Json<PasswordRequest>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let db = state.db.lock().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("Falha ao abrir o banco local")),
        )
    })?;
    crate::db::add_archive_password(&db, &req.password, req.source.as_deref().unwrap_or("manual"))
        .map_err(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::new(format!("Falha ao salvar senha: {error}"))),
            )
        })?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn import_archive_passwords(
    State(state): State<AppState>,
    Json(req): Json<ImportPasswordsRequest>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let db = state.db.lock().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("Falha ao abrir o banco local")),
        )
    })?;
    for password in req.passwords {
        crate::db::add_archive_password(&db, &password, req.source.as_deref().unwrap_or("manual"))
            .map_err(|error| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiError::new(format!("Falha ao importar senha: {error}"))),
                )
            })?;
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn record_archive_password_success(
    State(state): State<AppState>,
    Json(req): Json<PasswordRequest>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let db = state.db.lock().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("Falha ao abrir o banco local")),
        )
    })?;
    crate::db::record_archive_password_success(&db, &req.password, req.source.as_deref().unwrap_or("auto"))
        .map_err(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::new(format!("Falha ao registrar senha: {error}"))),
            )
        })?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_archive_password(
    State(state): State<AppState>,
    Json(req): Json<PasswordRequest>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let db = state.db.lock().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("Falha ao abrir o banco local")),
        )
    })?;
    crate::db::delete_archive_password(&db, &req.password).map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new(format!("Falha ao esquecer senha: {error}"))),
        )
    })?;
    Ok(StatusCode::NO_CONTENT)
}
