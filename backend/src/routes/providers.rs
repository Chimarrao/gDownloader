use axum::{
    extract::Query,
    http::StatusCode,
    Json,
};
use serde::Deserialize;

use crate::{
    models::{ApiError, FileInfo},
    providers,
};

#[derive(Deserialize)]
pub struct UrlQuery {
    pub url: String,
}

// GET /detect?url=...
// Retorna qual provider suporta a URL e suas informações de display
pub async fn detect_provider(
    Query(params): Query<UrlQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    match providers::detect_provider(&params.url) {
        Some(provider) => {
            let (id, color) = provider_meta(provider.name());
            Ok(Json(serde_json::json!({
                "id": id,
                "name": provider.name(),
                "icon": id,
                "color": color,
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
    Query(params): Query<UrlQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let provider = providers::detect_provider(&params.url).ok_or_else(|| {
        (StatusCode::BAD_REQUEST, Json(ApiError::new("URL não reconhecida")))
    })?;

    let info: FileInfo = provider.get_file_info(&params.url).await.map_err(|e| {
        (StatusCode::BAD_REQUEST, Json(ApiError::new(e.to_string())))
    })?;

    Ok(Json(serde_json::json!({
        "name": info.filename,
        "size": info.size,
        "mimeType": info.mime_type,
    })))
}

// GET /providers
// Lista todos os providers suportados
pub async fn list_providers() -> Json<serde_json::Value> {
    Json(serde_json::json!([
        { "id": "mega",       "name": "Mega",         "icon": "mega",       "color": "#e84d3d" },
        { "id": "mediafire",  "name": "MediaFire",    "icon": "mediafire",  "color": "#0062C7" },
        { "id": "gdrive",     "name": "Google Drive", "icon": "gdrive",     "color": "#4285F4" },
        { "id": "pixeldrain", "name": "PixelDrain",   "icon": "pixeldrain", "color": "#ff6600" },
    ]))
}

fn provider_meta(name: &str) -> (&'static str, &'static str) {
    match name {
        "Mega"         => ("mega",       "#e84d3d"),
        "MediaFire"    => ("mediafire",  "#0062C7"),
        "Google Drive" => ("gdrive",     "#4285F4"),
        "PixelDrain"   => ("pixeldrain", "#ff6600"),
        _              => ("unknown",    "#64748b"),
    }
}
