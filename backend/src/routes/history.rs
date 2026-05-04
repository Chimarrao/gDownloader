use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;

use crate::{
    models::{ApiError, HistoryItem},
    ws::AppState,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryQuery {
    pub q: Option<String>,
    pub host: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub page: Option<usize>,
    pub page_size: Option<usize>,
}

pub async fn list_history(
    State(state): State<AppState>,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<Vec<HistoryItem>>, (StatusCode, Json<ApiError>)> {
    let Ok(db) = state.db.lock() else {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("Falha ao abrir o banco local")),
        ));
    };

    let items = crate::db::search_history(
        &db,
        &crate::db::HistorySearch {
            q: query.q,
            host: query.host,
            from: query.from,
            to: query.to,
            page: query.page.unwrap_or(0),
            page_size: query.page_size.unwrap_or(80),
        },
    )
    .map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new(format!("Falha ao carregar o histórico local: {error}"))),
        )
    })?;

    Ok(Json(items))
}

pub async fn list_history_hosts(
    State(state): State<AppState>,
) -> Result<Json<Vec<String>>, (StatusCode, Json<ApiError>)> {
    let Ok(db) = state.db.lock() else {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("Falha ao abrir o banco local")),
        ));
    };

    let hosts = crate::db::list_history_hosts(&db).map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new(format!("Falha ao carregar hosts do histórico: {error}"))),
        )
    })?;

    Ok(Json(hosts))
}

pub async fn save_history(
    State(state): State<AppState>,
    Json(items): Json<Vec<HistoryItem>>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let Ok(db) = state.db.lock() else {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("Falha ao abrir o banco local")),
        ));
    };

    crate::db::replace_history(&db, &items).map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new(format!("Falha ao salvar o histórico local: {error}"))),
        )
    })?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn upsert_history_item(
    State(state): State<AppState>,
    Json(item): Json<HistoryItem>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let Ok(db) = state.db.lock() else {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("Falha ao abrir o banco local")),
        ));
    };

    crate::db::upsert_history_item(&db, &item).map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new(format!("Falha ao salvar item no histórico: {error}"))),
        )
    })?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_history_item(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let Ok(db) = state.db.lock() else {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("Falha ao abrir o banco local")),
        ));
    };

    crate::db::delete_history_item(&db, &id).map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new(format!("Falha ao remover item do histórico: {error}"))),
        )
    })?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn clear_history(
    State(state): State<AppState>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let Ok(db) = state.db.lock() else {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("Falha ao abrir o banco local")),
        ));
    };

    crate::db::clear_history(&db).map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new(format!("Falha ao limpar o histórico local: {error}"))),
        )
    })?;

    Ok(StatusCode::NO_CONTENT)
}
