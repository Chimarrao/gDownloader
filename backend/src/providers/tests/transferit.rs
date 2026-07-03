use super::TransferItProvider;

#[test]
fn matches_transfer_links() {
    assert!(TransferItProvider::matches("https://transfer.it/t/JsbpUa9X0RIC"));
    assert!(TransferItProvider::matches("https://www.transfer.it/t/JsbpUa9X0RIC"));
    assert!(!TransferItProvider::matches("https://transfer.it/features"));
}

#[test]
fn decodes_transfer_title() {
    assert_eq!(
        TransferItProvider::decode_transfer_text(
            "KEFuaW1lc1RvdGFpcykgTWFuLm9mLlN0ZWVsLjIwMTMuMjE2MHAuTUFYLldFQi1ETC5ERFA1LjEuQXRtb3MuRFYuSERSLkgyNjUuRHVhbC5ta3Y",
        )
        .as_deref(),
        Some("(AnimesTotais) Man.of.Steel.2013.2160p.MAX.WEB-DL.DDP5.1.Atmos.DV.HDR.H265.Dual.mkv"),
    );
}

#[test]
fn decrypts_sample_file_name() {
    let key = super::MegaProvider::mega_base64_decode("Ez8TzSbwxwufPKhq9chaL4zZMBQgXNAvO9iZw7AkTbc");
    let attr_key = super::MegaProvider::derive_attr_key_from_node_key(&key, true).unwrap();
    assert_eq!(
        super::MegaProvider::decrypt_attributes_name(
            "4VNNrySp6tV8L7dxwtdmGneXr83vHFq-G5mTcGtRJirU2S8IVf84ku--AtePkMuCxljbkPc7eRw_mcI2kYTVevv6jWEmp5wWHlJfqO3zIcpu-LT6yA-Ke0GGBdVbfxbj96uhE0FFpGd0RJ5wJxFVeoOTshdT-Jt-pEdxXwAA6R8Lmjr26Te9C1pnGvnYQd6_",
            &attr_key,
        )
        .as_deref(),
        Some("(AnimesTotais) Man.of.Steel.2013.2160p.MAX.WEB-DL.DDP5.1.Atmos.DV.HDR.H265.Dual.mkv"),
    );
}
