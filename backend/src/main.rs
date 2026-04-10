// #[tokio::main] transforma a função main em assíncrona
// Sem isso, não podemos usar .await dentro de main
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Inicializa o sistema de logs — como configurar winston/pino no Node.js
    // "info" significa: mostra logs de nível INFO, WARN e ERROR
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    // Cria o router com todas as rotas registradas
    let app = create_router();

    // Porta 0 = o sistema operacional escolhe uma porta livre automaticamente
    // Isso evita conflitos com outras aplicações rodando na máquina
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();

    // Imprime a porta no stdout — o Electron vai ler isso para saber onde conectar
    // Formato específico "PORT:XXXX" que o Electron procura com regex
    println!("PORT:{port}");

    tracing::info!("Backend rodando em 127.0.0.1:{port}");

    // Inicia o servidor e fica em loop aguardando requisições
    // Isso bloqueia o programa aqui (é o comportamento desejado — servidor deve rodar forever)
    axum::serve(listener, app).await?;

    Ok(())
}

// Função separada para criar o router — facilita testes unitários
// Em testes, chamamos create_router() diretamente sem precisar de um servidor real
pub fn create_router() -> axum::Router {
    use axum::routing::get;
    use tower_http::cors::{Any, CorsLayer};

    // CORS = Cross-Origin Resource Sharing
    // Permite que o Vue (rodando em outra porta durante dev) acesse a API
    // Any = aceita qualquer origem, método e header
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    axum::Router::new()
        .route("/health", get(routes::health::health))
        // .layer() adiciona middleware — como app.use() no Express
        .layer(cors)
}

// Declaração dos módulos usados pelo main
// Em Rust, todo módulo precisa ser declarado explicitamente no arquivo pai
mod models;
mod routes;
