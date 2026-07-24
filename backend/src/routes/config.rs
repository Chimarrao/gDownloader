use axum::{http::StatusCode, Json};
use serde::Deserialize;

use crate::{
    models::{ApiError, LegacyConfigMigration, PublicSettings, SecureSettings},
    ws::AppState,
};

use super::downloads::{enforce_active_limit, rebalance_speed_limits, schedule_pending_downloads};

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
    // Reduziu o limite ao vivo? Devolve os excedentes à fila antes de reescalonar.
    enforce_active_limit(&state).await;
    schedule_pending_downloads(state).await;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct TorRuntimeRequest {
    /// Porta SOCKS do Tor; `None`/ausente desliga o roteamento isolado.
    pub socks_port: Option<u16>,
}

/// Informa ao backend a porta SOCKS do daemon Tor gerenciado pelo main, para
/// habilitar roteamento isolado por-download sem alterar o `proxy_mode` global.
pub async fn update_tor_runtime(
    axum::extract::State(state): axum::extract::State<AppState>,
    Json(req): Json<TorRuntimeRequest>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    *state.isolated_tor_port.lock().await = req.socks_port.filter(|port| *port > 0);
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

    // Aplica AO VIVO aos downloads já existentes (rodando ou na fila):
    // 1) limite TOTAL de banda compartilhada;
    state.global_speed_limit_bps.store(
        req.speed_limit_kib.saturating_mul(1024),
        std::sync::atomic::Ordering::Relaxed,
    );
    // 2) número de tentativas (o loop de retry relê `max_retries` do registro).
    //    `max_retries_per_download` é o TOTAL de tentativas; o orçamento de RETRIES
    //    é total-1. Infinito ignora o teto (usa u32::MAX como sentinela).
    let effective_max_retries = if req.infinite_retries {
        u32::MAX
    } else {
        req.max_retries_per_download.saturating_sub(1)
    };
    {
        let mut map = state.downloads.lock().await;
        for download in map.values_mut() {
            download.max_retries = effective_max_retries;
        }
    }

    // Reduziu o limite de simultâneos ao vivo? Devolve os excedentes à fila.
    enforce_active_limit(&state).await;
    // Redistribui a banda entre os ativos com o novo total.
    rebalance_speed_limits(&state).await;
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
