// Testes do provider Mega
// Cada função testa um comportamento específico e isolado

use gdownloader_backend::providers::mega::MegaProvider;
use gdownloader_backend::providers::detect_provider;

// --- parse_url ---

#[test]
fn parse_url_file_format_returns_handle_and_key() {
    let url = "https://mega.nz/file/AbCdEfGh#AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    let result = MegaProvider::parse_url(url);
    assert!(result.is_some());
    let (handle, _) = result.unwrap();
    assert_eq!(handle, "AbCdEfGh");
}

#[test]
fn parse_url_folder_format_returns_none() {
    let url = "https://mega.nz/folder/AbCdEfGh#key";
    assert!(MegaProvider::parse_url(url).is_none());
}

#[test]
fn parse_url_old_format_returns_handle() {
    let url = "https://mega.nz/#!AbCdEfGh!AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    let result = MegaProvider::parse_url(url);
    assert!(result.is_some());
}

#[test]
fn parse_url_invalid_returns_none() {
    assert!(MegaProvider::parse_url("https://mega.nz/invalid").is_none());
}

// --- mega_base64_decode ---

#[test]
fn base64_decode_zeros_returns_32_bytes() {
    let decoded = MegaProvider::mega_base64_decode(
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
    );
    assert_eq!(decoded.len(), 32);
    assert!(decoded.iter().all(|&b| b == 0));
}

// --- derive_key_and_iv ---

#[test]
fn derive_key_all_zeros_returns_zero_key_and_iv() {
    let key_bytes = vec![0u8; 32];
    let (aes_key, iv) = MegaProvider::derive_key_and_iv(&key_bytes);
    assert_eq!(aes_key, [0u8; 16]);
    assert_eq!(iv, [0u8; 16]);
}

#[test]
fn derive_key_xor_applied_correctly() {
    let mut key_bytes = vec![0u8; 32];
    key_bytes[0] = 0xFF;
    key_bytes[16] = 0x0F;
    let (aes_key, _) = MegaProvider::derive_key_and_iv(&key_bytes);
    assert_eq!(aes_key[0], 0xFF ^ 0x0F);
}

// --- detect_provider ---

#[test]
fn detect_provider_recognizes_mega_url() {
    let provider = detect_provider("https://mega.nz/file/AbCdEfGh#key123");
    assert!(provider.is_some());
    assert_eq!(provider.unwrap().name(), "Mega");
}
