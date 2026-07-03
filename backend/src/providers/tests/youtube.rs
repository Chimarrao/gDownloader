use super::{YouTubeProvider, SYNTHETIC_PROGRESS_TOTAL};

#[test]
fn matches_youtube_short_and_watch_urls() {
    assert!(YouTubeProvider::matches("https://youtu.be/xF8l17MJkMk?si=abc"));
    assert!(YouTubeProvider::matches("https://www.youtube.com/watch?v=xF8l17MJkMk"));
    assert!(YouTubeProvider::matches("https://music.youtube.com/watch?v=xF8l17MJkMk"));
    assert!(!YouTubeProvider::matches("https://example.com/watch?v=xF8l17MJkMk"));
}

#[test]
fn detects_channel_urls_and_limits_channel_capture() {
    assert!(YouTubeProvider::is_channel_url("https://www.youtube.com/@canal/videos"));
    assert!(YouTubeProvider::is_channel_url("https://www.youtube.com/channel/UCabc123"));
    assert!(!YouTubeProvider::is_channel_url("https://www.youtube.com/watch?v=xF8l17MJkMk"));
    assert_eq!(
        YouTubeProvider::channel_limit("https://www.youtube.com/@canal#ytdlp_channel_limit=40"),
        Some(40)
    );
    assert_eq!(
        YouTubeProvider::channel_limit("https://www.youtube.com/@canal#ytdlp_channel_limit=5000"),
        Some(1_000)
    );
}

#[test]
fn parses_ytdlp_progress_with_real_total() {
    let (downloaded, total, speed, eta) =
        YouTubeProvider::parse_progress("GDLPROG  42.5% 1.5MiB/s 00:09").unwrap();
    assert_eq!(downloaded, 4250);
    assert_eq!(total, SYNTHETIC_PROGRESS_TOTAL);
    assert_eq!(speed, 1_572_864);
    assert_eq!(eta, 9);
}

#[test]
fn parses_ytdlp_progress_with_synthetic_total_when_total_is_na() {
    let (downloaded, total, _, _) =
        YouTubeProvider::parse_progress("GDLPROG  25.0% N/A 00:00").unwrap();
    assert_eq!(downloaded, SYNTHETIC_PROGRESS_TOTAL / 4);
    assert_eq!(total, SYNTHETIC_PROGRESS_TOTAL);
}

#[test]
fn split_media_phase_progress_does_not_overlap_ranges() {
    assert_eq!(YouTubeProvider::phase_progress(SYNTHETIC_PROGRESS_TOTAL, 1, true), 4_500);
    assert_eq!(YouTubeProvider::phase_progress(0, 2, true), 4_500);
    assert_eq!(YouTubeProvider::phase_progress(SYNTHETIC_PROGRESS_TOTAL, 2, true), 9_000);
    assert_eq!(YouTubeProvider::merge_progress(true), 9_250);
}

#[test]
fn reads_selected_merge_format_from_child_fragment() {
    let children = Some(vec![
        "https://www.youtube.com/watch?v=x#ytdlp_format=bestvideo%2Bbestaudio&ytdlp_merge_format=mkv".to_string(),
    ]);
    assert_eq!(
        YouTubeProvider::selected_value(&children, "ytdlp_merge_format").as_deref(),
        Some("mkv")
    );
    assert_eq!(YouTubeProvider::normalize_merge_format("MKV").as_deref(), Some("mkv"));
    assert_eq!(YouTubeProvider::normalize_merge_format("avi"), None);
}
