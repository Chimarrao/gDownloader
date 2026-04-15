// Ponto de entrada do binário — equivalente ao index.php ou ao bootstrap do Laravel
// Importa do lib.rs para reutilizar a criação do router e o estado WebSocket
use gdownloader_backend::{create_router, ws};

// #[tokio::main] transforma main() em assíncrona
// Sem isso, não dá para usar .await — obrigatório para qualquer I/O async
// É como declarar o loop de eventos do Node.js ou configurar o Swoole no PHP
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Inicializa o sistema de logs estruturado
    // Similar a configurar um logger (Monolog, winston, pino)
    // "info" = nível mínimo de log que será exibido
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    // Cria o estado global compartilhado entre todas as rotas e WebSockets
    // É como um container de dependências (DI) passado para cada handler
    let state = ws::AppState::new();

    // Monta o router com todas as rotas HTTP — ver lib.rs para a lista completa
    let app = create_router(state);

    // Porta 0 = o sistema operacional escolhe uma porta livre automaticamente
    // Evita conflitos com outras aplicações rodando na máquina
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();

    // Imprime a porta no stdout para o Electron capturar e saber onde conectar
    // O Electron lê a saída padrão deste processo e extrai o número da porta
    println!("PORT:{port}");
    tracing::info!("Backend rodando em 127.0.0.1:{port}");

    // Inicia o servidor HTTP — bloqueia até o processo ser encerrado
    // Equivalente a $app->run() no Slim ou ao app.listen() no Express
    axum::serve(listener, app).await?;
    Ok(())
}
