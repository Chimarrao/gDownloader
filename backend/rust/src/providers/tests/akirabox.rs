use serde_json::json;

use super::AkiraboxProvider;

#[test]
fn matches_file_pages() {
    assert!(AkiraboxProvider::matches("https://akirabox.to/qPez4W5ga3OY/file"));
    assert!(!AkiraboxProvider::matches("https://akirabox.to/login"));
}

#[test]
fn helper_job_status_deserializes_expected_shape() {
    let payload = json!({
        "status": "downloading",
        "bytesDownloaded": 128,
        "totalBytes": 1024,
        "filename": "arquivo.mkv"
    });

    let parsed = serde_json::from_value::<super::HelperJobStatus>(payload).expect("json válido");
    assert_eq!(parsed.status, "downloading");
    assert_eq!(parsed.bytes_downloaded, 128);
    assert_eq!(parsed.total_bytes, 1024);
    assert_eq!(parsed.filename.as_deref(), Some("arquivo.mkv"));
}
