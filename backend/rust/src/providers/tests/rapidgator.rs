use super::RapidgatorProvider;

#[test]
fn matches_standard_file_pages() {
    assert!(RapidgatorProvider::matches(
        "https://rapidgator.net/file/c2bd37a929081047efeff6ab088479dd/file.mp4.html"
    ));
    assert!(!RapidgatorProvider::matches("https://rapidgator.net/article/abc"));
}

#[test]
fn detects_removed_page_from_fixture() {
    let html = include_str!("../../../tests/fixtures/providers/rapidgator_removed.html");
    assert!(RapidgatorProvider::is_removed_page(html));
}

#[test]
fn detects_free_limit_block_for_large_files() {
    let html = r#"
        <html>
          <body>
            <div class="premium">Download files up to 1 GB in free mode</div>
          </body>
        </html>
    "#;

    assert!(RapidgatorProvider::is_free_limit_block(html, 1_500_000_000));
}
