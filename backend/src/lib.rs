// lib.rs — raiz da biblioteca (crate)
// Declara os módulos públicos que ficam disponíveis para os testes de integração
// e para o main.rs usar. É como o autoload do Composer: registra o que existe.

pub mod db;        // Persistência SQLite
pub mod hash_verify;
pub mod mirrors;   // Busca de mirrors (SSE streaming)
pub mod models;    // Structs e enums de dados (Download, FileInfo, WsEvent, etc.)
pub mod providers; // Lógica de cada provedor de download (Mega, MediaFire, etc.)
pub mod reconnect; // Reconexão/rotação de IP para quebrar rate-limit
pub mod routes;    // Handlers HTTP organizados por domínio
pub mod stats;     // StatsManager e ring buffer de velocidade
pub mod ws;        // Estado global + handler do WebSocket

// Re-exporta detect_provider diretamente no topo da crate
// Atalho: permite `gdownloader_backend::detect_provider(url)` nos testes,
// sem precisar escrever `gdownloader_backend::providers::detect_provider(url)`
pub use crate::providers::detect_provider;

pub fn create_state(db_path: &str) -> anyhow::Result<ws::AppState> {
    let db = db::init(db_path)?;
    let max_concurrent_downloads = db::load_public_settings(&db)
        .map(|settings| settings.max_concurrent_downloads.max(1))
        .unwrap_or(3);
    Ok(ws::AppState::new_with_max_and_db_path(
        db,
        max_concurrent_downloads,
        Some(db_path.to_string()),
    ))
}

// Monta e retorna o router do Axum com todas as rotas configuradas
// Equivalente ao arquivo de rotas do Laravel (routes/api.php) ou do Express (app.use(...))
// Recebe o AppState (estado compartilhado) que será injetado em cada handler
pub fn create_router(db_path: &str) -> axum::Router {
    let state = create_state(db_path).expect("Falha ao abrir banco SQLite");
    create_router_with_state(state)
}

pub fn create_cnl_router(state: ws::AppState) -> axum::Router {
    use axum::routing::{get, post};
    use tower_http::cors::{Any, CorsLayer};

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    axum::Router::new()
        .route("/flash/add", post(routes::clicknload::flash_add))
        .route("/flash/addcrypted", post(routes::clicknload::flash_addcrypted))
        .route("/flash/addcrypted2", post(routes::clicknload::flash_addcrypted2))
        .route("/jdcheck.js", get(routes::clicknload::jdcheck))
        .with_state(state)
        .layer(cors)
}

pub fn create_router_with_state(state: ws::AppState) -> axum::Router {
    // Recupera downloads interrompidos do SQLite em background
    {
        let state_clone = state.clone();
        tokio::spawn(async move {
            routes::downloads::recover_downloads_from_db(state_clone).await;
        });
    }

    // Initialize proxy settings from DB
    {
        let settings = state.db.lock().ok()
            .and_then(|db| crate::db::load_public_settings(&db).ok())
            .unwrap_or_default();
        crate::providers::update_global_proxy(
            settings.proxy_mode,
            settings.proxy_host,
            settings.proxy_port,
            settings.proxy_username,
            settings.proxy_password,
        );
    }

    // Start TOR if enabled
    {
        let state_clone = state.clone();
        tokio::spawn(async move {
            let settings = state_clone
                .db
                .lock()
                .ok()
                .and_then(|db| crate::db::load_public_settings(&db).ok())
                .unwrap_or_default();
            if settings.start_tor {
                match tokio::process::Command::new("tor")
                    .arg("--SocksPort")
                    .arg("9050")
                    .arg("--DataDirectory")
                    .arg("/tmp/tor-data")
                    .spawn() {
                    Ok(child) => {
                        tracing::info!("TOR process started with PID {}", child.id().unwrap_or(0));
                    }
                    Err(e) => {
                        tracing::warn!("Failed to start TOR: {}", e);
                    }
                }
            }
        });
    }

    // Stats tick: every 1s, collect speed from active downloads and broadcast
    {
        let state_clone = state.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(1));
            loop {
                interval.tick().await;
                let (total_speed, per_host) = {
                    let downloads = state_clone.downloads.lock().await;
                    let mut total = 0u64;
                    let mut host_map: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
                    for d in downloads.values() {
                        if d.status == crate::models::DownloadStatus::Downloading {
                            total += d.speed_bps;
                            *host_map.entry(d.provider.clone()).or_insert(0) += d.speed_bps;
                        }
                    }
                    (total, host_map)
                };
                state_clone.stats.record_stats(total_speed, per_host.clone()).await;
                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                state_clone.broadcast(crate::models::WsEvent::StatsTick {
                    timestamp,
                    total_speed_bps: total_speed,
                    per_host_speed: per_host,
                });
            }
        });
    }

    use axum::routing::{delete, get, post};
    use tower_http::cors::{Any, CorsLayer};

    // Configura CORS permissivo — aceita requisições de qualquer origem
    // Necessário porque o Vue roda em uma origem diferente (window do Electron ou localhost:5173)
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Define cada rota com seu método HTTP e handler correspondente
    // .with_state(state) injeta o AppState em todos os handlers que pedirem State<AppState>
    // .layer(cors) aplica o middleware de CORS em todas as rotas
    axum::Router::new()
        .route("/health",        get(routes::health::health))
        .route("/ws",            get(ws::ws_handler))
        .route("/providers",     get(routes::providers::list_providers))
        .route("/detect",        get(routes::providers::detect_provider))
        .route("/file-info",     get(routes::providers::get_file_info))
        .route("/file-info/cache", get(routes::providers::get_cached_file_info))
        .route("/config/public", get(routes::config::get_public_settings))
        .route("/config/public", post(routes::config::update_public_settings))
        .route("/config/test-proxy", get(routes::config::test_proxy))
        .route("/config/legacy-migrations", get(routes::config::get_legacy_config_migrations))
        .route("/config/legacy-migrations", post(routes::config::record_legacy_config_migration))
        .route("/config/downloads", post(routes::config::update_download_config))
        .route("/config/secure", get(routes::config::get_secure_settings))
        .route("/config/secure", post(routes::config::update_secure_settings))
        .route("/history",       get(routes::history::list_history))
        .route("/history",       post(routes::history::save_history))
        .route("/history",       delete(routes::history::clear_history))
        .route("/history/hosts", get(routes::history::list_history_hosts))
        .route("/history/item",  post(routes::history::upsert_history_item))
        .route("/history/:id",   delete(routes::history::delete_history_item))
        .route("/archive-passwords", get(routes::archive_passwords::list_archive_passwords))
        .route("/archive-passwords", post(routes::archive_passwords::add_archive_password))
        .route("/archive-passwords/import", post(routes::archive_passwords::import_archive_passwords))
        .route("/archive-passwords/success", post(routes::archive_passwords::record_archive_password_success))
        .route("/archive-passwords/delete", post(routes::archive_passwords::delete_archive_password))
        .route("/links/import-container", post(routes::links::import_container))
        .route("/downloads",     post(routes::downloads::add_download))
        .route("/downloads",     get(routes::downloads::list_downloads))
        .route("/downloads/duplicates", get(routes::downloads::list_duplicate_downloads))
        .route("/downloads/:id/events", get(routes::downloads::list_download_events))
        .route("/downloads/finished", delete(routes::downloads::clear_finished_downloads))
        .route("/downloads/:id/pause", post(routes::downloads::pause_download))
        .route("/downloads/:id/resume", post(routes::downloads::resume_download))
        .route("/downloads/:id/retry", post(routes::downloads::retry_download))
        .route("/downloads/:id/restart", post(routes::downloads::restart_download))
        .route("/downloads/:id/force", post(routes::downloads::force_download))
        .route("/downloads/:id/priority", post(routes::downloads::update_download_priority))
        .route("/downloads/:id/speed-limit", post(routes::downloads::update_download_speed_limit))
        .route("/downloads/:id/remove", delete(routes::downloads::remove_download))
        .route("/downloads/:id/remove-with-files", delete(routes::downloads::remove_download_with_files))
        .route("/downloads/:id", delete(routes::downloads::cancel_download))
        .route("/downloads/:id/pin", post(routes::downloads::toggle_pin_download))
        .route("/captcha", get(routes::captcha::captcha_page))
        .route("/captcha/submit", post(routes::captcha::submit_captcha))
        .route("/mirrors/search", get(routes::mirrors::search_mirrors))
        .route("/stats/realtime", get(routes::stats::get_realtime_stats))
        .route("/packages",      get(routes::packages::list_packages))
        .route("/packages",      post(routes::packages::create_package))
        .route("/packages/:id",  delete(routes::packages::delete_package))
        .route("/packages/:package_id/assign/:download_id", post(routes::packages::assign_download_to_package))
        .route("/packages/unassign/:download_id", delete(routes::packages::unassign_download_from_package))
        .with_state(state)
        .layer(cors)
}
