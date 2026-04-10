// #[tokio::main] transforma main em assíncrona — obrigatório para usar async/await
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Inicializa o sistema de logs (como configurar winston/pino no Node.js)
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    // Cria o estado compartilhado com canal de broadcast e mapa de downloads
    let state = ws::AppState::new();

    let app = create_router(state);

    // Porta 0 = o SO escolhe uma porta livre automaticamente
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();

    // Imprime a porta para o Electron ler via stdout
    println!("PORT:{port}");
    tracing::info!("Backend rodando em 127.0.0.1:{port}");

    axum::serve(listener, app).await?;
    Ok(())
}

// Router separado em função para facilitar testes
pub fn create_router(state: ws::AppState) -> axum::Router {
    use axum::routing::get;
    use tower_http::cors::{Any, CorsLayer};

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    axum::Router::new()
        .route("/health", get(routes::health::health))
        .route("/ws", get(ws::ws_handler))   // WebSocket endpoint para progresso em tempo real
        .with_state(state)                    // Injeta AppState em todos os handlers que precisam
        .layer(cors)
}

// Declaração dos módulos — Rust exige declaração explícita de cada módulo
mod models;
mod providers;
mod routes;
mod ws;
