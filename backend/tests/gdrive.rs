// Testes do provider Google Drive

use gdownloader_backend::providers::gdrive::GDriveProvider;
use gdownloader_backend::providers::detect_provider;

// --- matches ---

#[test]
fn matches_standard_drive_url() {
    assert!(GDriveProvider::matches(
        "https://drive.google.com/file/d/1ABC/view"
    ));
}

#[test]
fn does_not_match_non_drive_url() {
    assert!(!GDriveProvider::matches("https://mega.nz/file/abc"));
}

// --- detect_provider ---

#[test]
fn detect_provider_recognizes_gdrive_url() {
    let provider = detect_provider("https://drive.google.com/file/d/1ABC/view");
    assert!(provider.is_some());
    assert_eq!(provider.unwrap().name(), "Google Drive");
}
