// lib.rs — raiz da biblioteca (crate)
// Declara os módulos públicos que ficam disponíveis para os testes de integração
// e para o main.rs usar. É como o autoload do Composer: registra o que existe.

pub mod db;        // Persistência SQLite
pub mod mirrors;   // Busca de mirrors (SSE streaming)
pub mod models;    // Structs e enums de dados (Download, FileInfo, WsEvent, etc.)
pub mod providers; // Lógica de cada provedor de download (Mega, MediaFire, etc.)
pub mod routes;    // Handlers HTTP organizados por domínio
pub mod ws;        // Estado global + handler do WebSocket

// Re-exporta detect_provider diretamente no topo da crate
// Atalho: permite `gdownloader_backend::detect_provider(url)` nos testes,
// sem precisar escrever `gdownloader_backend::providers::detect_provider(url)`
pub use crate::providers::detect_provider;

// Monta e retorna o router do Axum com todas as rotas configuradas
// Equivalente ao arquivo de rotas do Laravel (routes/api.php) ou do Express (app.use(...))
// Recebe o AppState (estado compartilhado) que será injetado em cada handler
pub fn create_router(db_path: &str) -> axum::Router {
    let db = db::init(db_path).expect("Falha ao abrir banco SQLite");
    let max_concurrent_downloads = db::load_public_settings(&db)
        .map(|settings| settings.max_concurrent_downloads.max(1))
        .unwrap_or(3);
    let state = ws::AppState::new_with_max(db, max_concurrent_downloads);
    create_router_with_state(state)
}

pub fn create_router_with_state(state: ws::AppState) -> axum::Router {
    // Recupera downloads interrompidos do SQLite em background
    {
        let state_clone = state.clone();
        tokio::spawn(async move {
            routes::downloads::recover_downloads_from_db(state_clone).await;
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
        .route("/config/legacy-migrations", get(routes::config::get_legacy_config_migrations))
        .route("/config/legacy-migrations", post(routes::config::record_legacy_config_migration))
        .route("/config/downloads", post(routes::config::update_download_config))
        .route("/config/secure", get(routes::config::get_secure_settings))
        .route("/config/secure", post(routes::config::update_secure_settings))
        .route("/history",       get(routes::history::list_history))
        .route("/history",       post(routes::history::save_history))
        .route("/history",       delete(routes::history::clear_history))
        .route("/downloads",     post(routes::downloads::add_download))
        .route("/downloads",     get(routes::downloads::list_downloads))
        .route("/downloads/finished", delete(routes::downloads::clear_finished_downloads))
        .route("/downloads/:id/pause", post(routes::downloads::pause_download))
        .route("/downloads/:id/resume", post(routes::downloads::resume_download))
        .route("/downloads/:id/retry", post(routes::downloads::retry_download))
        .route("/downloads/:id/restart", post(routes::downloads::restart_download))
        .route("/downloads/:id/force", post(routes::downloads::force_download))
        .route("/downloads/:id/remove", delete(routes::downloads::remove_download))
        .route("/downloads/:id/remove-with-files", delete(routes::downloads::remove_download_with_files))
        .route("/downloads/:id", delete(routes::downloads::cancel_download))
        .route("/captcha", get(routes::captcha::captcha_page))
        .route("/captcha/submit", post(routes::captcha::submit_captcha))
        .route("/mirrors/search", get(routes::mirrors::search_mirrors))
        .with_state(state)
        .layer(cors)
}
