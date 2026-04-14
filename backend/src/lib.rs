// lib.rs — exporta módulos públicos para testes e para que outras crates usem a biblioteca

pub mod models;
pub mod providers;
pub mod routes;
pub mod ws;

// Exporta a função create_router do main
pub use crate::providers::detect_provider;

pub fn create_router(state: ws::AppState) -> axum::Router {
    use axum::routing::{delete, get, post};
    use tower_http::cors::{Any, CorsLayer};

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    axum::Router::new()
        .route("/health", get(routes::health::health))
        .route("/ws", get(ws::ws_handler))
        .route("/downloads", post(routes::downloads::add_download))
        .route("/downloads", get(routes::downloads::list_downloads))
        .route("/downloads/:id", delete(routes::downloads::cancel_download))
        .with_state(state)
        .layer(cors)
}
