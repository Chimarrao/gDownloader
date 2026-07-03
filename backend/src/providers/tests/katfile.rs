use super::KatfileProvider;

#[test]
fn matches_file_page() {
    assert!(KatfileProvider::matches("https://katfile.ws/u1ifmhkgsyjx"));
    assert!(KatfileProvider::matches("https://katfile.com/u1ifmhkgsyjx"));
    assert!(KatfileProvider::matches("https://katfile.space/u1ifmhkgsyjx"));
    assert!(!KatfileProvider::matches("https://katfile.ws/login.html"));
}

#[test]
fn extracts_metadata_from_initial_page() {
    let html = r#"
        <form id="btn_download">
          <input type="hidden" name="fname" value="Familia.Soprano.S01E01.1080p.Mini.HMAX.WEB-DL.DD2.0.H.264.DUAL-Dinho.mkv">
        </form>
        <span id="fsize">1.1 GB</span>
    "#;

    assert_eq!(
        KatfileProvider::extract_filename(html).as_deref(),
        Some("Familia.Soprano.S01E01.1080p.Mini.HMAX.WEB-DL.DD2.0.H.264.DUAL-Dinho.mkv")
    );
    assert_eq!(KatfileProvider::extract_size(html), 1181116006);
}

#[test]
fn detects_removed_page() {
    let html = r#"
        <html>
          <body>
            <img src="../images/404-remove.png" alt="File has been removed">
          </body>
        </html>
    "#;

    assert!(KatfileProvider::is_removed_page(html));
}

#[test]
fn detects_removed_page_from_fixture_like_html() {
    let html = r#"
        <html>
          <title>Katfile - File Removed</title>
          <body>
            <div class="msg">This file has been removed</div>
          </body>
        </html>
    "#;

    assert!(KatfileProvider::is_removed_page(html));
}
