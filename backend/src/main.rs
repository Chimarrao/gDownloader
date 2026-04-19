use gdownloader_backend::create_router;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    // Primeiro argumento = caminho do banco SQLite (passado pelo Electron)
    let args: Vec<String> = std::env::args().collect();
    let db_path = args.get(1).cloned().unwrap_or_else(|| {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        format!("{}/.config/gDownloader/downloads.db", home)
    });

    // Garante que o diretório pai existe
    if let Some(parent) = std::path::Path::new(&db_path).parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let app = create_router(&db_path);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();

    println!("PORT:{port}");
    tracing::info!("Backend rodando em 127.0.0.1:{port}");

    axum::serve(listener, app).await?;
    Ok(())
}
