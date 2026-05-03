use gdownloader_backend::{create_cnl_router, create_router_with_state, create_state};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Primeiro argumento = caminho do banco SQLite (passado pelo Electron)
    let args: Vec<String> = std::env::args().collect();
    let db_path = args.get(1).cloned().unwrap_or_else(|| {
        let cwd = std::env::current_dir().unwrap_or_else(|_| ".".into());
        cwd.join("database")
            .join("gdownloader.db")
            .to_string_lossy()
            .into_owned()
    });

    // Garante que o diretório pai existe
    let db_parent = std::path::Path::new(&db_path)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    std::fs::create_dir_all(&db_parent).ok();

    // Diretório de logs: mesmo nível do banco de dados → backend/logs/
    let log_dir = if db_parent
        .file_name()
        .and_then(|value| value.to_str())
        .map(|value| value.eq_ignore_ascii_case("database"))
        .unwrap_or(false)
    {
        db_parent.parent().unwrap_or(&db_parent).join("logs")
    } else {
        db_parent.join("logs")
    };
    std::fs::create_dir_all(&log_dir).ok();

    // Writer para app.log (rotação diária)
    let app_appender = tracing_appender::rolling::daily(&log_dir, "app.log");
    let (app_writer, _app_guard) = tracing_appender::non_blocking(app_appender);

    // Writer para mirrors.log (rotação diária)
    let mirrors_appender = tracing_appender::rolling::daily(&log_dir, "mirrors.log");
    let (mirrors_writer, _mirrors_guard) = tracing_appender::non_blocking(mirrors_appender);

    // Layer de app.log — captura tudo (info+)
    let app_layer = fmt::layer()
        .with_writer(app_writer)
        .with_ansi(false)
        .with_target(true);

    // Layer de mirrors.log — só logs com target "mirrors"
    let mirrors_layer = fmt::layer()
        .with_writer(mirrors_writer)
        .with_ansi(false)
        .with_target(true)
        .with_filter(tracing_subscriber::filter::filter_fn(|meta| {
            meta.target().starts_with("mirrors")
        }));

    // Layer de stderr — saída colorida no terminal/Electron (comportamento anterior)
    let stderr_layer = fmt::layer()
        .with_writer(std::io::stderr)
        .with_ansi(true);

    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,gdownloader_backend=debug,mirrors=debug")),
        )
        .with(stderr_layer)
        .with(app_layer)
        .with(mirrors_layer)
        .init();

    let state = create_state(&db_path)?;
    let cnl_state = state.clone();
    tokio::spawn(async move {
        match tokio::net::TcpListener::bind("127.0.0.1:9666").await {
            Ok(listener) => {
                tracing::info!("Click'n'Load local rodando em 127.0.0.1:9666");
                if let Err(error) = axum::serve(listener, create_cnl_router(cnl_state)).await {
                    tracing::error!("Servidor Click'n'Load encerrou com erro: {error}");
                }
            }
            Err(error) => {
                tracing::warn!("Não foi possível abrir Click'n'Load em 127.0.0.1:9666: {error}");
            }
        }
    });

    let app = create_router_with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();

    println!(
        "READY:{}",
        serde_json::json!({
            "port": port,
            "dbPath": db_path,
        })
    );
    println!("PORT:{port}");
    tracing::info!("Backend rodando em 127.0.0.1:{port}");

    axum::serve(listener, app).await?;
    Ok(())
}
