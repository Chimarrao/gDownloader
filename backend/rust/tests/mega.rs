mod common;

use gdownloader_backend::providers::detect_provider;
use gdownloader_backend::providers::mega::MegaProvider;
use gdownloader_backend::providers::Provider;
use std::time::Duration;

use common::{assert_download_starts_and_can_abort, required_test_env, skip_if_missing, temp_test_path};

async fn fetch_folder_info_with_retry(provider: &MegaProvider, url: &str) -> gdownloader_backend::models::FileInfo {
    match provider.get_file_info(url).await {
        Ok(info) => info,
        Err(first_err) => {
            tokio::time::sleep(Duration::from_secs(2)).await;
            provider.get_file_info(url).await.unwrap_or_else(|second_err| {
                panic!(
                    "Falha ao ler pasta pública do Mega após retry. Primeiro erro: {first_err}. Segundo erro: {second_err}"
                )
            })
        }
    }
}

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
fn parse_folder_url_returns_handle_and_key() {
    let url = "https://mega.nz/folder/u01BkSJK#RijytzrQB9yQHDgdJZfNWw";
    let result = MegaProvider::parse_folder_url(url);
    assert!(result.is_some());
    let (handle, key) = result.unwrap();
    assert_eq!(handle, "u01BkSJK");
    assert_eq!(key.len(), 16);
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

#[test]
fn decrypts_root_folder_name_from_real_sample() {
    let (_, shared_key) = MegaProvider::parse_folder_url(
        "https://mega.nz/folder/u01BkSJK#RijytzrQB9yQHDgdJZfNWw"
    ).unwrap();

    let encrypted_node_key = MegaProvider::mega_base64_decode("BOT1Jt3QEMAXNInwbq-LpA");
    let node_key = MegaProvider::decrypt_folder_node_key(&shared_key, &encrypted_node_key).unwrap();
    let attr_key = MegaProvider::derive_attr_key_from_node_key(&node_key, false).unwrap();
    let name = MegaProvider::decrypt_attributes_name(
        "B1lOzMxCg_5MYgQhsUrQPv3wQPWlg5_VmxGewiVM4LxNMT4hiT_IVqVT9nM3eAYe",
        &attr_key,
    ).unwrap();

    assert_eq!(name, "Prints Warface - Avulsos");
}

#[test]
fn decrypts_child_file_name_from_real_folder_sample() {
    let (_, shared_key) = MegaProvider::parse_folder_url(
        "https://mega.nz/folder/u01BkSJK#RijytzrQB9yQHDgdJZfNWw"
    ).unwrap();

    let encrypted_node_key =
        MegaProvider::mega_base64_decode("ja7knJrgHRIXgM99jR3p9x0BQeKevUrtdpiOS0Ms4AI");
    let node_key = MegaProvider::decrypt_folder_node_key(&shared_key, &encrypted_node_key).unwrap();
    let attr_key = MegaProvider::derive_attr_key_from_node_key(&node_key, true).unwrap();
    let name = MegaProvider::decrypt_attributes_name(
        "aGZ0VdeJ-hFAFap07jrCVBjvrrZvCGzRZjur3GhFM8PwWdzhYJB3fnHA49OExGiAzCUK4Me7F0aMyZt-aIt86hl_9eaG15NzjQPAtf1czPI",
        &attr_key,
    ).unwrap();

    assert_eq!(name, "ScreenShot0012.jpg");
}

// --- detect_provider ---

#[test]
fn detect_provider_recognizes_mega_url() {
    let provider = detect_provider("https://mega.nz/file/AbCdEfGh#key123");
    assert!(provider.is_some());
    assert_eq!(provider.unwrap().name(), "Mega");
}

#[tokio::test]
async fn real_mega_file_info_returns_real_name() {
    let url = required_test_env("TEST_MEGA_FILE_URL");
    if skip_if_missing(&url) {
        return;
    }

    let provider = MegaProvider;
    let info = provider.get_file_info(&url).await.unwrap();

    assert!(!info.is_folder);
    assert!(info.size > 0);
    assert!(!info.filename.trim().is_empty());
    assert!(!info.filename.starts_with("mega_"));
}

#[tokio::test]
async fn real_mega_folder_info_returns_children() {
    let url = required_test_env("TEST_MEGA_FOLDER_URL");
    if skip_if_missing(&url) {
        return;
    }

    let provider = MegaProvider;
    let info = fetch_folder_info_with_retry(&provider, &url).await;

    assert!(info.is_folder);
    assert!(info.size > 0);
    assert!(!info.filename.trim().is_empty());
    assert!(!info.filename.starts_with("mega_"));
    assert!(info.children.as_ref().is_some_and(|children| !children.is_empty()));
    assert!(info.children.as_ref().unwrap().iter().all(|child| !child.filename.starts_with("mega_")));
}

#[tokio::test]
async fn real_mega_folder_download_starts_and_can_be_aborted() {
    let url = required_test_env("TEST_MEGA_FOLDER_URL");
    if skip_if_missing(&url) {
        return;
    }

    let provider = MegaProvider;
    let info = fetch_folder_info_with_retry(&provider, &url).await;
    let dest = temp_test_path("gdownloader-mega-folder").join(info.filename);

    assert_download_starts_and_can_abort(provider, url, dest, true).await;
}
