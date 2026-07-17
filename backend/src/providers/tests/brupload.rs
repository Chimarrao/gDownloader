use super::BruploadProvider;

#[test]
fn matches_file_page() {
    assert!(BruploadProvider::matches("https://www.brupload.net/1ppok7ga1hfm"));
    assert!(BruploadProvider::matches("https://www.brupload.net/d/1ppok7ga1hfm"));
    assert!(!BruploadProvider::matches("https://www.brupload.net/login.html"));
}

#[test]
fn matches_and_detects_folder_page() {
    let folder = "https://www.brupload.net/users/Victxor/91479";
    assert!(BruploadProvider::matches(folder));
    assert!(BruploadProvider::is_folder_url(folder));
    // Página de arquivo não é pasta.
    assert!(!BruploadProvider::is_folder_url("https://www.brupload.net/1ppok7ga1hfm"));
}

#[test]
fn extracts_folder_entries_by_file_code() {
    let html = r#"
        <a href="https://brupload.net/1ppok7ga1hfm">Arquivo 1</a>
        <a href="/d/abcd1234efgh">Arquivo 2</a>
        <a href="https://brupload.net/users/Victxor/91479">voltar à pasta</a>
        <a href="/login.html">login</a>
        <a href="https://brupload.net/1ppok7ga1hfm">duplicado</a>
    "#;
    let entries = BruploadProvider::extract_folder_entries(html);
    let urls: Vec<_> = entries.iter().map(|e| e.source_url.as_str()).collect();
    assert!(urls.contains(&"https://brupload.net/1ppok7ga1hfm"));
    assert!(urls.contains(&"https://brupload.net/abcd1234efgh"));
    // Sem duplicados, sem a pasta, sem login (curto/não-código).
    assert_eq!(entries.len(), 2);
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
