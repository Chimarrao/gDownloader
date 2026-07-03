use super::GDriveProvider;

#[test]
fn extracts_warning_page_metadata() {
    let html = r#"
        <p class="uc-warning-subcaption"><span class="uc-name-size">
          <a href="/open?id=abc">arquivo-grande.mkv</a> (2.5G)
        </span> is too large for Google to scan for viruses.</p>
    "#;

    let (filename, size) = GDriveProvider::extract_warning_page_metadata(html, "abc");
    assert_eq!(filename, "arquivo-grande.mkv");
    assert!(size > 2_600_000_000);
}

#[test]
fn extracts_confirm_download_url() {
    let html = r#"
        <form id="download-form" action="https://drive.usercontent.google.com/download" method="get">
          <input type="hidden" name="id" value="abc">
          <input type="hidden" name="export" value="download">
          <input type="hidden" name="confirm" value="t">
          <input type="hidden" name="uuid" value="uuid-123">
        </form>
    "#;

    let url = GDriveProvider::extract_confirm_download_url(html).expect("url");
    assert!(url.contains("drive.usercontent.google.com/download"));
    assert!(url.contains("id=abc"));
    assert!(url.contains("export=download"));
    assert!(url.contains("confirm=t"));
    assert!(url.contains("uuid=uuid-123"));
}

#[test]
fn parses_folder_children_from_drive_ivd() {
    let ivd = r#"[[["file123",["folder123"],"Movie.mkv","video/x-matroska",0,null,0,0,0,0,0,null,null,12345],["file456",["folder123"],"Subtitle.srt","text/plain",0,null,0,0,0,0,0,null,null,678]],null]"#;
    let children = GDriveProvider::parse_folder_children_from_ivd(ivd).expect("children");

    assert_eq!(children.len(), 2);
    assert_eq!(children[0].filename, "Movie.mkv");
    assert_eq!(children[0].size, 12345);
    assert_eq!(
        children[0].source_url.as_deref(),
        Some("https://drive.google.com/file/d/file123/view")
    );
}

#[test]
fn marks_drive_folder_children_as_folders() {
    let ivd = r#"[[["child_folder",["root"],"Subpasta","application/vnd.google-apps.folder",0,null,0,0,0,0,0,null,null,0],["file123",["root"],"Movie.mkv","video/x-matroska",0,null,0,0,0,0,0,null,null,12345]],null]"#;
    let children =
        GDriveProvider::parse_folder_children_from_ivd_with_prefix(ivd, "Root").expect("children");

    assert_eq!(children.len(), 2);
    assert!(children[0].is_folder);
    assert_eq!(children[0].path.as_deref(), Some("Root/Subpasta"));
    assert_eq!(
        children[0].source_url.as_deref(),
        Some("https://drive.google.com/drive/folders/child_folder")
    );
    assert!(!children[1].is_folder);
    assert_eq!(children[1].path.as_deref(), Some("Root/Movie.mkv"));
}

#[test]
fn renames_duplicate_drive_child_paths() {
    let mut seen = std::collections::HashMap::new();

    assert_eq!(
        GDriveProvider::unique_child_path("THUMBS/1.jpeg", &mut seen),
        "THUMBS/1.jpeg"
    );
    assert_eq!(
        GDriveProvider::unique_child_path("THUMBS/1.jpeg", &mut seen),
        "THUMBS/1 (2).jpeg"
    );
    assert_eq!(
        GDriveProvider::unique_child_path("README", &mut seen),
        "README"
    );
    assert_eq!(
        GDriveProvider::unique_child_path("README", &mut seen),
        "README (2)"
    );
}

#[test]
fn extracts_folder_id_from_drive_folder_url() {
    assert_eq!(
        GDriveProvider::extract_folder_id(
            "https://drive.google.com/drive/folders/1P6BHkRSXsfF2pCPi4ZlJ18GBtlODhX5Y?usp=sharing"
        )
        .as_deref(),
        Some("1P6BHkRSXsfF2pCPi4ZlJ18GBtlODhX5Y")
    );
}

#[test]
fn strips_google_drive_error_html() {
    let message = GDriveProvider::strip_html(
        r#"Sorry, you can&#39;t view or download this file at this time."#,
    );
    assert_eq!(message, "Sorry, you can't view or download this file at this time.");
}
