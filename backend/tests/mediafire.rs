// Testes do provider MediaFire

use gdownloader_backend::providers::mediafire::MediaFireProvider;
use gdownloader_backend::providers::detect_provider;

// --- matches ---

#[test]
fn matches_standard_mediafire_url() {
    assert!(MediaFireProvider::matches(
        "https://www.mediafire.com/file/abc123/file.zip/file"
    ));
}

#[test]
fn matches_mediafire_folder_url() {
    assert!(MediaFireProvider::matches(
        "https://www.mediafire.com/folder/sf8smp0qmm0hr/dbz"
    ));
}

#[test]
fn does_not_match_non_mediafire_url() {
    assert!(!MediaFireProvider::matches("https://mega.nz/file/abc"));
}

// --- extract_direct_link ---

#[test]
fn extract_link_from_download_button() {
    let html = r#"<a id="downloadButton" href="https://download2390.mediafire.com/get/abc/file.zip">Download</a>"#;
    let link = MediaFireProvider::extract_direct_link(html);
    assert_eq!(
        link,
        Some("https://download2390.mediafire.com/get/abc/file.zip".to_string())
    );
}

#[test]
fn extract_link_from_fallback_anchor() {
    let html = r#"<a href="https://download123.mediafire.com/get/xyz/photo.jpg">Baixar</a>"#;
    let link = MediaFireProvider::extract_direct_link(html);
    assert!(link.is_some());
}

#[test]
fn returns_none_when_no_download_link_found() {
    let html = r#"<html><body><p>Nenhum link</p></body></html>"#;
    assert!(MediaFireProvider::extract_direct_link(html).is_none());
}

#[test]
fn extracts_filename_from_standard_file_url() {
    let filename = MediaFireProvider::extract_filename_from_url(
        "https://www.mediafire.com/file/rzr1u8ba62xksi0/DBZ.161.BD1080p.MemoriadaTV.Menor.mkv/file"
    );
    assert_eq!(
        filename,
        Some("DBZ.161.BD1080p.MemoriadaTV.Menor.mkv".to_string())
    );
}

#[test]
fn extracts_folder_key_from_folder_url() {
    let folder_key = MediaFireProvider::extract_folder_key(
        "https://www.mediafire.com/folder/sf8smp0qmm0hr/dbz"
    );
    assert_eq!(folder_key, Some("sf8smp0qmm0hr".to_string()));
}

// --- detect_provider ---

#[test]
fn detect_provider_recognizes_mediafire_url() {
    let provider = detect_provider("https://www.mediafire.com/file/abc/file.zip");
    assert!(provider.is_some());
    assert_eq!(provider.unwrap().name(), "MediaFire");
}
