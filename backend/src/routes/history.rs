use axum::{extract::State, http::StatusCode, Json};

use crate::{
    models::{ApiError, HistoryItem},
    ws::AppState,
};

pub async fn list_history(
    State(state): State<AppState>,
) -> Result<Json<Vec<HistoryItem>>, (StatusCode, Json<ApiError>)> {
    let Ok(db) = state.db.lock() else {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("Falha ao abrir o banco local")),
        ));
    };

    let items = crate::db::load_history(&db).map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new(format!("Falha ao carregar o histórico local: {error}"))),
        )
    })?;

    Ok(Json(items))
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
