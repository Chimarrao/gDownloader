use super::BruploadProvider;

#[test]
fn matches_file_page() {
    assert!(BruploadProvider::matches("https://www.brupload.net/1ppok7ga1hfm"));
    assert!(BruploadProvider::matches("https://www.brupload.net/d/1ppok7ga1hfm"));
    assert!(!BruploadProvider::matches("https://www.brupload.net/login.html"));
}

#[test]
fn extracts_filename_from_current_page() {
    let html = r#"
        <title>Download Familia Soprano S01E04 1080p Mini HMAX WEB DD2 264 DUAL Dinho mkv</title>
        <input type="hidden" name="fname" value="Familia.Soprano.S01E04.1080p.mkv">
    "#;
    assert_eq!(
        BruploadProvider::extract_filename(html).as_deref(),
        Some("Familia.Soprano.S01E04.1080p.mkv")
    );
}

#[test]
fn detects_captcha_sitekeys_and_wait_time() {
    let html = r#"
        <div class="g-recaptcha" data-sitekey="sitekey-recaptcha"></div>
        <div class="h-captcha" data-sitekey="sitekey-hcaptcha"></div>
        <script>var seconds = 15;</script>
    "#;

    assert_eq!(BruploadProvider::detect_recaptcha_sitekey(html).as_deref(), Some("sitekey-recaptcha"));
    assert_eq!(BruploadProvider::detect_hcaptcha_sitekey(html).as_deref(), Some("sitekey-hcaptcha"));
    assert_eq!(BruploadProvider::extract_wait_seconds(html), Some(15));
}
