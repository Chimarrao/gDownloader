#![allow(dead_code)]

use std::sync::Once;
use std::time::Duration;

use gdownloader_backend::providers::{ProgressUpdate, Provider};
use tokio::sync::mpsc;
use tokio::time::timeout;
use uuid::Uuid;

static LOAD_DOTENV: Once = Once::new();

pub fn test_env(key: &str) -> Option<String> {
    LOAD_DOTENV.call_once(|| {
        let _ = dotenvy::from_filename(".env.test.local");
    });

    std::env::var(key).ok().filter(|value| !value.trim().is_empty())
}

pub fn required_test_env(key: &str) -> String {
    match test_env(key) {
        Some(value) => value,
        None => {
            eprintln!("Ignorando teste real: variável {key} não definida em backend/.env.test.local");
            String::new()
        }
    }
}

pub fn skip_if_missing(value: &str) -> bool {
    value.trim().is_empty()
}

pub fn temp_test_path(prefix: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("{prefix}-{}", Uuid::new_v4()))
}

pub async fn assert_download_starts_and_can_abort<P: Provider + 'static>(
    provider: P,
    url: String,
    dest_path: std::path::PathBuf,
    is_folder_target: bool,
) {
    let (tx, mut rx) = mpsc::channel::<ProgressUpdate>(32);
    let dest_string = dest_path.to_string_lossy().to_string();

    if is_folder_target {
        let _ = tokio::fs::create_dir_all(&dest_path).await;
    } else if let Some(parent) = dest_path.parent() {
        let _ = tokio::fs::create_dir_all(parent).await;
    }

    let handle = tokio::spawn(async move {
        provider
            .download(&url, &dest_string, ::std::sync::Arc::new(::std::sync::atomic::AtomicU64::new(0)), 1, None, tx)
            .await
    });

    let progress = timeout(Duration::from_secs(25), rx.recv())
        .await
        .expect("timeout aguardando primeiro progresso")
        .expect("canal de progresso encerrado antes do primeiro evento");

    let bytes = progress.bytes_downloaded;
    let child_bytes = progress.child_bytes_downloaded.unwrap_or(0);
    assert!(
        bytes > 0 || child_bytes > 0,
        "o download não iniciou de fato antes do abort"
    );

    handle.abort();
    let _ = handle.await;

    if dest_path.exists() {
        if is_folder_target {
            let _ = tokio::fs::remove_dir_all(&dest_path).await;
        } else {
            let _ = tokio::fs::remove_file(&dest_path).await;
        }
    }
}
