// Testes do provider PixelDrain

use gdownloader_backend::providers::pixeldrain::PixelDrainProvider;
use gdownloader_backend::providers::detect_provider;

// --- matches ---

#[test]
fn matches_standard_pixeldrain_url() {
    assert!(PixelDrainProvider::matches("https://pixeldrain.com/u/AbCdEfGh"));
}

#[test]
fn matches_pixeldrain_list_url() {
    assert!(PixelDrainProvider::matches("https://pixeldrain.com/l/B5LnKF1N#item=0"));
}

#[test]
fn does_not_match_non_pixeldrain_url() {
    assert!(!PixelDrainProvider::matches("https://mega.nz/file/abc"));
}

// --- detect_provider ---

#[test]
fn detect_provider_recognizes_pixeldrain_url() {
    let provider = detect_provider("https://pixeldrain.com/u/AbCdEfGh");
    assert!(provider.is_some());
    assert_eq!(provider.unwrap().name(), "PixelDrain");
}

#[test]
fn detect_provider_recognizes_pixeldrain_list_url() {
    let provider = detect_provider("https://pixeldrain.com/l/B5LnKF1N#item=0");
    assert!(provider.is_some());
    assert_eq!(provider.unwrap().name(), "PixelDrain");
}

#[test]
fn detect_provider_returns_none_for_unknown_url() {
    assert!(detect_provider("ftp://example.com/file.zip").is_none());
}
