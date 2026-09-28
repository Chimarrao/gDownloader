use super::{ShareEntry, TeraboxProvider};

#[test]
fn extracts_surl() {
    let url = "https://www.terabox.app/portuguese/sharing/link?surl=7ztIK8tA1cPr03ELh563Rg";
    assert!(TeraboxProvider::matches(url));
    assert_eq!(
        TeraboxProvider::extract_surl(url).as_deref(),
        Some("7ztIK8tA1cPr03ELh563Rg")
    );
}

#[test]
fn extracts_video_dir_and_fsid() {
    let url = "https://www.1024tera.com/sharing/videoPlay?surl=abc&dir=/Dragon+Ball+Z/Medio&fsid=123&fileName=x";
    assert_eq!(TeraboxProvider::extract_dir(url).as_deref(), Some("/Dragon Ball Z/Medio"));
    assert_eq!(TeraboxProvider::extract_fsid(url).as_deref(), Some("123"));
}

#[test]
fn parses_folder_entries_from_fixture_json() {
    let json = serde_json::from_str::<serde_json::Value>(include_str!(
        "../../../tests/fixtures/providers/terabox_share_list.json"
    ))
    .expect("fixture json válido");

    let entries = TeraboxProvider::parse_share_entries(&json).expect("deve parsear");
    assert_eq!(entries.len(), 2);
    assert!(entries[0].is_dir);
    assert_eq!(entries[1].size, 2147483648);
    assert_eq!(
        entries[1].filename,
        "[DarthinURMZ]Saint Seiya(Hexa)EP 01 1080p V2.mkv"
    );
}

#[test]
fn builds_child_path_for_video_entry() {
    let entry = ShareEntry {
        fs_id: "71002".to_string(),
        filename: "[DarthinURMZ]Saint Seiya(Hexa)EP 01 1080p V2.mkv".to_string(),
        path: "/CDZ/Maior/[DarthinURMZ]Saint Seiya(Hexa)EP 01 1080p V2.mkv".to_string(),
        size: 2147483648,
        is_dir: false,
        category: Some("1".to_string()),
    };
    let child = TeraboxProvider::entry_to_child(
        "https://www.terabox.com/sharing/link?surl=abcdef",
        &entry,
        "Maior/[DarthinURMZ]Saint Seiya(Hexa)EP 01 1080p V2.mkv".to_string(),
    );

    assert_eq!(child.path.as_deref(), Some("Maior/[DarthinURMZ]Saint Seiya(Hexa)EP 01 1080p V2.mkv"));
    assert!(child
        .source_url
        .as_deref()
        .unwrap_or_default()
        .contains("/sharing/videoPlay?surl=abcdef"));
}
