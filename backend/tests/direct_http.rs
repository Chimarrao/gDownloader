mod common;

use std::sync::Arc;

use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderMap, Method, Response, StatusCode},
    routing::get,
    Router,
};
use gdownloader_backend::{
    db,
    providers::{
        detect_provider,
        direct_http::DirectHttpProvider,
        DownloadContext,
        Provider,
    },
};
use tokio::sync::mpsc;

use common::temp_test_path;

const ETAG_VALUE: &str = "\"direct-http-test-v1\"";
const LAST_MODIFIED_VALUE: &str = "Wed, 01 May 2024 00:00:00 GMT";

#[derive(Clone)]
struct Fixture {
    data: Arc<Vec<u8>>,
}

async fn ranged_file_handler(
    State(fixture): State<Fixture>,
    method: Method,
    headers: HeaderMap,
) -> Response<Body> {
    let total = fixture.data.len() as u64;
    let mut status = StatusCode::OK;
    let mut start = 0u64;
    let mut end = total.saturating_sub(1);

    if let Some(range) = headers
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("bytes="))
    {
        let mut parts = range.splitn(2, '-');
        start = parts.next().and_then(|value| value.parse::<u64>().ok()).unwrap_or(0);
        end = parts
            .next()
            .filter(|value| !value.is_empty())
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or_else(|| total.saturating_sub(1));
        status = StatusCode::PARTIAL_CONTENT;
    }

    start = start.min(total.saturating_sub(1));
    end = end.min(total.saturating_sub(1)).max(start);
    let body_len = end.saturating_sub(start).saturating_add(1);
    let body = if method == Method::HEAD {
        Body::empty()
    } else {
        Body::from(fixture.data[start as usize..=end as usize].to_vec())
    };

    let mut response = Response::builder()
        .status(status)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::ETAG, ETAG_VALUE)
        .header(header::LAST_MODIFIED, LAST_MODIFIED_VALUE)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_DISPOSITION, "attachment; filename=\"sample.bin\"");

    if status == StatusCode::PARTIAL_CONTENT {
        response = response
            .header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{total}"))
            .header(header::CONTENT_LENGTH, body_len.to_string());
    } else {
        response = response.header(header::CONTENT_LENGTH, total.to_string());
    }

    response.body(body).unwrap()
}

async fn spawn_fixture_server(data: Vec<u8>) -> String {
    let app = Router::new()
        .route("/sample.bin", get(ranged_file_handler).head(ranged_file_handler))
        .with_state(Fixture {
            data: Arc::new(data),
        });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}/sample.bin")
}

fn test_data(size: usize) -> Vec<u8> {
    (0..size).map(|index| (index % 251) as u8).collect()
}

#[test]
fn detect_provider_uses_direct_http_as_fallback() {
    let provider = detect_provider("https://example.com/archive.zip").expect("direct fallback");
    assert_eq!(provider.name(), "Direct HTTP");

    let provider = detect_provider("https://mega.nz/file/AbCdEfGh#key123").expect("mega");
    assert_eq!(provider.name(), "Mega");
}

#[tokio::test]
async fn direct_http_file_info_uses_head_metadata() {
    let data = test_data(1024);
    let url = spawn_fixture_server(data).await;

    let info = DirectHttpProvider.get_file_info(&url).await.unwrap();

    assert_eq!(info.filename, "sample.bin");
    assert_eq!(info.size, 1024);
    assert_eq!(info.mime_type.as_deref(), Some("application/octet-stream"));
    assert!(!info.is_folder);
}

#[tokio::test]
async fn direct_http_segmented_download_merges_part_files() {
    let data = test_data(5 * 1024 * 1024);
    let url = spawn_fixture_server(data.clone()).await;
    let dest = temp_test_path("gdownloader-direct-http").join("sample.bin");
    tokio::fs::create_dir_all(dest.parent().unwrap()).await.unwrap();
    let dest_string = dest.to_string_lossy().to_string();
    let (tx, mut rx) = mpsc::channel(1024);

    let bytes = DirectHttpProvider
        .download(&url, &dest_string, ::std::sync::Arc::new(::std::sync::atomic::AtomicU64::new(0)), 4, None, tx)
        .await
        .unwrap();

    while rx.try_recv().is_ok() {}

    assert_eq!(bytes, data.len() as u64);
    assert_eq!(tokio::fs::read(&dest).await.unwrap(), data);
    assert!(!dest.with_extension("bin.part0").exists());
    assert!(!std::path::PathBuf::from(format!("{dest_string}.part0")).exists());

    let _ = tokio::fs::remove_file(&dest).await;
}

#[tokio::test]
async fn direct_http_resumes_single_stream_from_sqlite_offset() {
    let data = test_data(256 * 1024);
    let url = spawn_fixture_server(data.clone()).await;
    let dest = temp_test_path("gdownloader-direct-http-resume").join("sample.bin");
    tokio::fs::create_dir_all(dest.parent().unwrap()).await.unwrap();
    tokio::fs::write(&dest, &data[..8192]).await.unwrap();

    let db_path = temp_test_path("gdownloader-direct-http-resume-db").with_extension("sqlite");
    let conn = db::init(db_path.to_str().unwrap()).unwrap();
    let dest_string = dest.to_string_lossy().to_string();
    let download_key = format!("{url}\n{dest_string}");
    db::save_direct_http_part(
        &conn,
        &download_key,
        0,
        &url,
        Some(ETAG_VALUE),
        Some(LAST_MODIFIED_VALUE),
        0,
        data.len() as u64 - 1,
        8192,
    )
    .unwrap();
    drop(conn);

    let (tx, _rx) = mpsc::channel(64);
    let bytes = DirectHttpProvider
        .download_with_context(
            &url,
            &dest_string,
            ::std::sync::Arc::new(::std::sync::atomic::AtomicU64::new(0)),
            1,
            None,
            tx,
            DownloadContext {
                db_path: Some(db_path.to_string_lossy().to_string()),
                ..DownloadContext::default()
            },
        )
        .await
        .unwrap();

    assert_eq!(bytes, data.len() as u64);
    assert_eq!(tokio::fs::read(&dest).await.unwrap(), data);

    let _ = tokio::fs::remove_file(&dest).await;
    let _ = tokio::fs::remove_file(&db_path).await;
}
