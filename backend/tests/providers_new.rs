use gdownloader_backend::providers::{
    akirabox::AkiraboxProvider,
    brfiles::BrfilesProvider,
    brupload::BruploadProvider,
    detect_provider,
    katfile::KatfileProvider,
    rapidgator::RapidgatorProvider,
    terabox::TeraboxProvider,
};

#[test]
fn detect_provider_recognizes_newer_hosters() {
    let cases = [
        ("https://akirabox.to/qPez4W5ga3OY/file", "AkiraBox"),
        ("https://brfiles.com/f/MoQFnG5r/Hannibal.S02e01.1080P.WEB-DL.DUAL.DUBLASERIES.TV.mkv", "BRFiles"),
        ("https://www.brupload.net/1ppok7ga1hfm", "BRupload"),
        ("https://katfile.com/u1ifmhkgsyjx", "Katfile"),
        ("https://rapidgator.net/file/c2bd37a929081047efeff6ab088479dd/file.mp4.html", "Rapidgator"),
        ("https://www.terabox.com/sharing/link?surl=fDbunOb9Y25CRnZdn1l1GQ", "Terabox"),
    ];

    for (url, expected) in cases {
        let detected = detect_provider(url).expect("provider reconhecido");
        assert_eq!(detected.name(), expected, "url: {url}");
    }
}

#[test]
fn direct_matches_cover_supported_aliases() {
    assert!(AkiraboxProvider::matches("https://akirabox.to/qPez4W5ga3OY/file"));
    assert!(BrfilesProvider::matches("https://brfiles.com/d/kAZST1Am/"));
    assert!(BruploadProvider::matches("https://www.brupload.net/d/1ppok7ga1hfm"));
    assert!(KatfileProvider::matches("https://katfile.ws/u1ifmhkgsyjx"));
    assert!(RapidgatorProvider::matches("https://rapidgator.net/file/c2bd37a929081047efeff6ab088479dd/file.mp4.html"));
    assert!(TeraboxProvider::matches("https://www.1024tera.com/sharing/link?surl=abc"));
}
