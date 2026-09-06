mod common;

use gdownloader_backend::providers::brfiles::BrfilesProvider;
use gdownloader_backend::providers::fichier::FichierProvider;
use gdownloader_backend::providers::katfile::KatfileProvider;
use gdownloader_backend::providers::rapidgator::RapidgatorProvider;
use gdownloader_backend::providers::terabox::TeraboxProvider;
use gdownloader_backend::providers::akirabox::AkiraboxProvider;
use gdownloader_backend::providers::Provider;

use common::{required_test_env, skip_if_missing, test_env};

#[tokio::test]
async fn real_brfiles_file_info_returns_name_when_configured() {
    let url = required_test_env("TEST_BRFILES_FILE_URL");
    if skip_if_missing(&url) {
        return;
    }

    let info = BrfilesProvider.get_file_info(&url).await.unwrap();
    assert!(!info.is_folder);
    assert!(!info.filename.trim().is_empty());
}

#[tokio::test]
async fn real_rapidgator_file_info_returns_name_when_configured() {
    let url = required_test_env("TEST_RAPIDGATOR_FILE_URL");
    if skip_if_missing(&url) {
        return;
    }

    let info = RapidgatorProvider.get_file_info(&url).await.unwrap();
    assert!(!info.is_folder);
    assert!(!info.filename.trim().is_empty());
}

#[tokio::test]
async fn real_terabox_file_info_returns_name_when_helper_is_available() {
    let url = required_test_env("TEST_TERABOX_SHARE_URL");
    if skip_if_missing(&url) || skip_if_missing(&test_env("TERABOX_PROXY_PORT").unwrap_or_default()) {
        return;
    }

    let info = TeraboxProvider.get_file_info(&url).await.unwrap();
    assert!(!info.filename.trim().is_empty());
}

#[tokio::test]
async fn real_katfile_file_info_returns_name_when_helper_is_available() {
    let url = required_test_env("TEST_KATFILE_FILE_URL");
    if skip_if_missing(&url) || skip_if_missing(&test_env("KATFILE_PROXY_PORT").unwrap_or_default()) {
        return;
    }

    let info = KatfileProvider.get_file_info(&url).await.unwrap();
    assert!(!info.filename.trim().is_empty());
}

#[tokio::test]
async fn real_akirabox_file_info_returns_name_when_helper_is_available() {
    let url = required_test_env("TEST_AKIRABOX_FILE_URL");
    if skip_if_missing(&url) || skip_if_missing(&test_env("AKIRABOX_PROXY_PORT").unwrap_or_default()) {
        return;
    }

    let info = AkiraboxProvider.get_file_info(&url).await.unwrap();
    assert!(!info.filename.trim().is_empty());
}

/// Mantém a verificação contra a página real opcional para não tornar a suite
/// dependente de rede. O link de teste é informado somente na execução manual.
#[tokio::test]
async fn real_1fichier_folder_info_returns_children_when_configured() {
    let url = required_test_env("TEST_1FICHIER_FOLDER_URL");
    if skip_if_missing(&url) {
        return;
    }

    let info = FichierProvider.get_file_info(&url).await.unwrap();
    assert!(info.is_folder);
    assert!(info.children.as_ref().is_some_and(|children| !children.is_empty()));
    assert!(info.size > 0);
}
