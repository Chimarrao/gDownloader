use super::SharePointProvider;

#[test]
fn matches_sharepoint_urls() {
    assert!(SharePointProvider::matches("https://tenant-my.sharepoint.com/:u:/r/personal/foo/Documents/file.rar"));
    assert!(SharePointProvider::matches("https://onedrive.live.com/?id=abc"));
    assert!(SharePointProvider::matches("https://1drv.ms/u/s!abc"));
    assert!(!SharePointProvider::matches("https://mega.nz/file/test"));
}

#[test]
fn decodes_percent_encoded_filename() {
    let decoded = SharePointProvider::decode_path_segment("Rambo%204%20-%202008.rar");
    assert_eq!(decoded, "Rambo 4 - 2008.rar");
}
