use super::BrfilesProvider;

#[test]
fn matches_file_and_folder_pages() {
    assert!(BrfilesProvider::matches(
        "https://brfiles.com/f/MoQFnG5r/Serie.Ficticia.S02E01.1080p.WEB-DL.DUAL.mkv"
    ));
    assert!(BrfilesProvider::matches("https://brfiles.com/d/kAZST1Am/"));
    assert!(!BrfilesProvider::matches("https://brfiles.com/login"));
}

#[test]
fn extracts_file_metadata_and_wait_link() {
    let html = r#"
        <title>Serie.Ficticia.S02E01.1080p.WEB-DL.DUAL.mkv - BRFiles</title>
        <p class="tamanho-arquivo">995 MB</p>
        <script>
          var seconds = 30;
          html += "<a class='btn btn-free' href='https://brfiles.com/f/MoQFnG5r/Serie.Ficticia.S02E01.1080p.WEB-DL.DUAL.mkv?pt=4yo2KoxY7BfXNm7e7Ld5nA%3D%3D'>Clique para baixar</a>";
        </script>
    "#;

    assert_eq!(
        BrfilesProvider::extract_filename(html).as_deref(),
        Some("Serie.Ficticia.S02E01.1080p.WEB-DL.DUAL.mkv")
    );
    assert_eq!(BrfilesProvider::extract_size(html), 995 * 1024 * 1024);
    assert_eq!(BrfilesProvider::extract_wait_seconds(html), Some(30));
    assert_eq!(
        BrfilesProvider::extract_pt_url(html).as_deref(),
        Some("https://brfiles.com/f/MoQFnG5r/Serie.Ficticia.S02E01.1080p.WEB-DL.DUAL.mkv?pt=4yo2KoxY7BfXNm7e7Ld5nA%3D%3D")
    );
}

#[test]
fn extracts_folder_entries() {
    let html = include_str!("../../../tests/fixtures/providers/brfiles_folder.html");

    let entries = BrfilesProvider::extract_folder_entries(html);
    assert_eq!(BrfilesProvider::extract_folder_name(html).as_deref(), Some("Serie Ficticia S01"));
    assert_eq!(entries.len(), 2);
    assert_eq!(
        entries[0].source_url,
        "https://brfiles.com/f/HniTPKsv/Serie.Ficticia.S01E01.1080p.WEB-DL.DUAL.mkv"
    );
    assert_eq!(
        entries[0].filename,
        "Serie.Ficticia.S01E01.1080p.WEB-DL.DUAL.mkv"
    );
    assert_eq!(
        entries[1].filename,
        "Serie.Ficticia.S01E02.1080p.WEB-DL.DUAL.mkv"
    );
}

#[test]
fn extracts_rate_limit_seconds_from_message() {
    let html = "<div class='warning'>Seu IP já possui outro download ativo. Aguarde 2 horas 15 minutos e 9 segundos.</div>";
    assert_eq!(BrfilesProvider::extract_rate_limit_seconds(html), Some(8109));
}

#[test]
fn falls_back_to_longer_cooldown_when_free_slot_is_not_ready() {
    let html = "<div class='warning'>Seu IP já possui outro download ativo. Aguarde 3 horas.</div>";
    assert_eq!(BrfilesProvider::extract_rate_limit_seconds(html), Some(10800));
}
