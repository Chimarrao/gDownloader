use super::FichierProvider;

#[test]
fn parses_wait_seconds() {
    let html = r#"
        <script>
            var ct = 60;
        </script>
    "#;
    assert_eq!(FichierProvider::extract_wait_seconds(html), Some(60));
}

#[test]
fn parses_filename_and_size() {
    let html = r#"
        <span style="font-weight:bold">Outcome.2026.mp4</span>
        <span style="font-size:0.9em;font-style:italic">1.66 GB</span>
    "#;

    let (filename, size) = FichierProvider::extract_filename_and_size(html, "fallback.bin");
    assert_eq!(filename, "Outcome.2026.mp4");
    assert!(size > 1_700_000_000 && size < 1_900_000_000);
}

#[test]
fn detects_free_slot_error() {
    let html = "All free guest slots are currently in use.";
    assert!(FichierProvider::has_free_slot_error(html));
}

#[test]
fn detects_restricted_access_block() {
    // Texto real da página servida pelo 1fichier a IPs de Tor/VPN/servidor.
    let html = "Accès restreint – professional infrastructure detected. This IP \
                address has been identified as belonging to a server, proxy, VPN, \
                relay network, or associated with abusive activity.";
    assert!(FichierProvider::has_restricted_access_error(html));
    // Página normal não dispara o bloqueio.
    assert!(!FichierProvider::has_restricted_access_error(
        "<span style=\"font-weight:bold\">arquivo.rar</span>"
    ));
}

#[test]
fn direct_link_ignores_favicon_and_assets() {
    // Reproduz a página real do 1fichier: o favicon aparece como o primeiro
    // href; o link verdadeiro está no botão verde de download.
    let html = r#"
        <link rel="icon" href="https://img.1fichier.com/favicon.ico" />
        <a href="https://img.1fichier.com/css/style.css">css</a>
        <a href="https://1fichier.com/tarifs.html">Preços</a>
        <a href="/login.pl">Entrar</a>
        <div class="ct_warn">
          <a href="https://a-7.1fichier.com/c123456789?file=token" class="ok btn-general btn-orange">Click here to download the file</a>
        </div>
        <a target="_new" href="https://facebook.com/1fichiercom">fb</a>
    "#;

    assert_eq!(
        FichierProvider::extract_direct_link(html).as_deref(),
        Some("https://a-7.1fichier.com/c123456789?file=token")
    );
}

#[test]
fn direct_link_falls_back_without_button_class() {
    // Mesmo sem a classe do botão, o filtro deve pular favicon/assets/páginas
    // e cair no nó de download real.
    let html = r#"
        <a href="https://img.1fichier.com/favicon.ico">icon</a>
        <a href="https://1fichier.com/cgu.html">cgu</a>
        <a href="https://a-3.1fichier.com/d/abcXYZ">Download</a>
    "#;
    assert_eq!(
        FichierProvider::extract_direct_link(html).as_deref(),
        Some("https://a-3.1fichier.com/d/abcXYZ")
    );
}

#[test]
fn direct_link_rejects_bare_domain() {
    assert!(!FichierProvider::is_plausible_direct_link("https://1fichier.com"));
    assert!(!FichierProvider::is_plausible_direct_link("https://1fichier.com/"));
    assert!(!FichierProvider::is_plausible_direct_link(
        "https://img.1fichier.com/favicon.ico"
    ));
    assert!(FichierProvider::is_plausible_direct_link(
        "https://a-9.1fichier.com/c987"
    ));
}

#[test]
fn extracts_folder_children_with_sizes() {
    let html = r#"
        <tr>
          <td class="normal alg file-obj"><a href="https://1fichier.com/?abc123">Primeiro.rar</a></td>
          <td class="normal">17.06 GB</td>
        </tr>
        <tr>
          <td class="normal alg file-obj"><a href="https://1fichier.com/?def456">Segundo.rar</a></td>
          <td class="normal">6.73 GB</td>
        </tr>
    "#;

    let children = FichierProvider::extract_folder_children(html);
    assert_eq!(children.len(), 2);
    assert_eq!(children[0].filename, "Primeiro.rar");
    assert_eq!(children[0].source_url.as_deref(), Some("https://1fichier.com/?abc123"));
    assert!(children[0].size > 18_000_000_000 - 2_000_000_000);
    assert!(children[1].size > 7_000_000_000 - 500_000_000);
}
