mod common;

use gdownloader_backend::providers::brfiles::BrfilesProvider;
use gdownloader_backend::providers::brupload::BruploadProvider;
use gdownloader_backend::providers::katfile::KatfileProvider;
use gdownloader_backend::providers::rapidgator::RapidgatorProvider;
use gdownloader_backend::providers::terabox::TeraboxProvider;
use gdownloader_backend::providers::akirabox::AkiraboxProvider;
use gdownloader_backend::providers::Provider;

use common::{required_test_env, skip_if_missing, test_env};

#[tokio::test]
async fn real_brupload_file_info_returns_name_when_configured() {
    let url = required_test_env("TEST_BRUPLOAD_FILE_URL");
    if skip_if_missing(&url) {
        return;
    }

    let info = BruploadProvider.get_file_info(&url).await.unwrap();
    assert!(!info.is_folder);
    assert!(!info.filename.trim().is_empty());
}

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
