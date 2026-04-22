use axum::{
    extract::Query,
    http::StatusCode,
    Json,
};
use serde::Deserialize;

use crate::{
    models::{ApiError, CachedFileInfo, FileInfo, SecureSettings},
    providers,
};

fn account_state_for_provider(
    provider_name: &str,
    secure: &SecureSettings,
) -> Option<providers::ProviderAccountState> {
    match provider_name {
        "Terabox" => secure.terabox_account.as_ref().map(|account| providers::ProviderAccountState {
            connected: !account.cookies.is_empty() || account.verified_at.is_some(),
            verified_at: account.verified_at.clone(),
        }),
        "BRupload" => secure.brupload_account.as_ref().map(|account| providers::ProviderAccountState {
            connected: !account.cookies.is_empty() || account.verified_at.is_some(),
            verified_at: account.verified_at.clone(),
        }),
        _ => None,
    }
}

fn enrich_descriptor(
    mut descriptor: providers::ProviderDescriptor,
    secure: &SecureSettings,
) -> providers::ProviderDescriptor {
    descriptor.account_state = account_state_for_provider(descriptor.name, secure);
    descriptor
}

#[derive(Deserialize)]
pub struct UrlQuery {
    pub url: String,
}

// GET /detect?url=...
// Retorna qual provider suporta a URL e suas informações de display
pub async fn detect_provider(
    axum::extract::State(state): axum::extract::State<crate::ws::AppState>,
    Query(params): Query<UrlQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    match providers::detect_provider(&params.url) {
        Some(provider) => {
            let secure_settings = state
                .db
                .lock()
                .ok()
                .and_then(|db| crate::db::load_secure_settings(&db).ok())
                .unwrap_or_default();
            let descriptor = enrich_descriptor(
                providers::provider_descriptor_from_name(provider.name()),
                &secure_settings,
            );
            Ok(Json(serde_json::json!({
                "id": descriptor.id,
                "name": descriptor.name,
                "icon": descriptor.id,
                "color": descriptor.color,
                "capabilities": descriptor.capabilities,
                "accountState": descriptor.account_state,
            })))
        }
        None => Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError::new("URL não reconhecida por nenhum provider suportado")),
        )),
    }
}

// GET /file-info?url=...
// Retorna metadados do arquivo (nome, tamanho) sem baixar
pub async fn get_file_info(
    axum::extract::State(state): axum::extract::State<crate::ws::AppState>,
    Query(params): Query<UrlQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let provider = providers::detect_provider(&params.url).ok_or_else(|| {
        (StatusCode::BAD_REQUEST, Json(ApiError::new("URL não reconhecida")))
    })?;

    let info: FileInfo = provider.get_file_info(&params.url).await.map_err(|e| {
        (StatusCode::BAD_REQUEST, Json(ApiError::new(e.to_string())))
    })?;

    if let Ok(db) = state.db.lock() {
        let _ = crate::db::save_cached_file_info(
            &db,
            &params.url,
            providers::provider_id_from_name(provider.name()),
            &info.filename,
            info.size,
            info.mime_type.as_deref(),
            info.is_folder,
            &info.children,
        );
    }

    Ok(Json(serde_json::json!({
        "name": info.filename,
        "size": info.size,
        "mimeType": info.mime_type,
        "isFolder": info.is_folder,
        "children": info.children,
    })))
}

pub async fn get_cached_file_info(
    axum::extract::State(state): axum::extract::State<crate::ws::AppState>,
    Query(params): Query<UrlQuery>,
) -> Result<Json<CachedFileInfo>, (StatusCode, Json<ApiError>)> {
    let Ok(db) = state.db.lock() else {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("Falha ao abrir o banco local")),
        ));
    };

    let cached = crate::db::load_cached_file_info(&db, &params.url).map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new(format!("Falha ao consultar o cache local: {error}"))),
        )
    })?;

    let Some(cached) = cached else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ApiError::new("Nenhum cache local encontrado para este link")),
        ));
    };

    Ok(Json(cached))
}

// GET /providers
// Lista todos os providers suportados
pub async fn list_providers(
    axum::extract::State(state): axum::extract::State<crate::ws::AppState>,
) -> Json<serde_json::Value> {
    let secure_settings = state
        .db
        .lock()
        .ok()
        .and_then(|db| crate::db::load_secure_settings(&db).ok())
        .unwrap_or_default();

    let items = providers::all_provider_descriptors()
        .into_iter()
        .map(|descriptor| enrich_descriptor(descriptor, &secure_settings))
        .collect::<Vec<_>>();

    Json(serde_json::json!(items))
}
