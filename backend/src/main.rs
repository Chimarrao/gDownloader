use gdownloader_backend::{create_cnl_router, create_router_with_state, create_state};
use tracing_subscriber::{
    filter::LevelFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer,
};

const LOG_RETENTION_DAYS: u64 = 14;

/// Remove arquivos de log rotacionados que já não ajudam no diagnóstico. Mantemos o
/// arquivo atual e só tocamos nos logs do backend, para nunca apagar arquivos do usuário.
fn cleanup_old_logs(log_dir: &std::path::Path) {
    let now = std::time::SystemTime::now();
    let retention = std::time::Duration::from_secs(LOG_RETENTION_DAYS * 24 * 60 * 60);

    let Ok(entries) = std::fs::read_dir(log_dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        // tracing_appender usa app.log.YYYY-MM-DD e mirrors.log.YYYY-MM-DD.
        if !((name.starts_with("app.log.") || name.starts_with("mirrors.log.")) && path.is_file()) {
            continue;
        }
        let Ok(modified) = entry.metadata().and_then(|metadata| metadata.modified()) else {
            continue;
        };
        if now.duration_since(modified).is_ok_and(|age| age > retention) {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn main() -> anyhow::Result<()> {
    // Runtime tokio multi-thread explícito para poder LIMITAR o pool de threads
    // bloqueantes (spawn_blocking). Nosso uso de spawn_blocking (verificação de hash
    // e gravação de resume) já é serializado pelo semáforo de finalização e pelo
    // throttle de resume, então 32 é folgado e evita o pool crescer sem limite sob
    // rajadas. Worker threads assíncronas ficam no padrão (nº de núcleos), adequado
    // para carga majoritariamente de I/O de rede.
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .max_blocking_threads(32)
        .build()?
        .block_on(async_main())
}

async fn async_main() -> anyhow::Result<()> {
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
    cleanup_old_logs(&log_dir);

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

    // O terminal é lento para processar um grande volume de texto e o Electron já
    // persiste os logs em arquivo. Portanto, só avisos e erros chegam ao stderr,
    // inclusive em builds de desenvolvimento.
    let stderr_layer = fmt::layer()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .with_filter(LevelFilter::WARN);

    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,gdownloader_backend=info,mirrors=info")),
        )
        .with(stderr_layer)
        .with(app_layer)
        .with(mirrors_layer)
        .init();

    let state = create_state(&db_path)?;
    gdownloader_backend::proxy_intercept::spawn_intercept_proxy_manager(state.clone());
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
