// Testes do provider Google Drive

use gdownloader_backend::providers::gdrive::GDriveProvider;
use gdownloader_backend::providers::detect_provider;
use gdownloader_backend::providers::Provider;

// --- matches ---

#[test]
fn matches_standard_drive_url() {
    assert!(GDriveProvider::matches(
        "https://drive.google.com/file/d/1ABC/view"
    ));
}

#[test]
fn recognizes_drive_folder_url() {
    assert!(GDriveProvider::matches(
        "https://drive.google.com/drive/folders/1P6BHkRSXsfF2pCPi4ZlJ18GBtlODhX5Y"
    ));
    assert!(GDriveProvider::is_folder_url(
        "https://drive.google.com/drive/folders/1P6BHkRSXsfF2pCPi4ZlJ18GBtlODhX5Y"
    ));
    assert!(
        GDriveProvider::extract_id(
            "https://drive.google.com/drive/folders/1P6BHkRSXsfF2pCPi4ZlJ18GBtlODhX5Y"
        )
        .is_none()
    );
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

#[tokio::test]
#[ignore]
async fn live_google_drive_folder_info_returns_children() {
    let provider = GDriveProvider;
    let info = provider
        .get_file_info("https://drive.google.com/drive/folders/1P6BHkRSXsfF2pCPi4ZlJ18GBtlODhX5Y")
        .await
        .expect("folder info");

    assert!(info.is_folder);
    assert_eq!(info.filename, "Eles Vivem - 1988");
    assert_eq!(info.children.as_ref().map(Vec::len), Some(5));
    assert!(info.size > 0);
}

#[tokio::test]
#[ignore]
async fn live_google_drive_large_folder_info_returns_children() {
    let provider = GDriveProvider;
    let info = provider
        .get_file_info("https://drive.google.com/drive/folders/1BpXC0DFRgRdVUiHvrixr-X0FArFFw2Si")
        .await
        .expect("folder info");

    assert!(info.is_folder);
    let children = info.children.as_ref().expect("children");
    assert!(children.iter().any(|child| child.filename.ends_with(".mkv")));
    assert!(children.iter().any(|child| child.filename.ends_with(".txt")));
    assert!(info.size > 20 * 1024 * 1024 * 1024);
}
