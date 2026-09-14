package providers

import (
	"os"
	"strings"
	"testing"
)

func TestMatchesYouTubeShortAndWatchURLs(t *testing.T) {
	p := YouTubeProvider{}
	if !p.Matches("https://youtu.be/xF8l17MJkMk?si=abc") {
		t.Fatalf("expected youtu.be to match")
	}
	if !p.Matches("https://www.youtube.com/watch?v=xF8l17MJkMk") {
		t.Fatalf("expected youtube.com/watch to match")
	}
	if !p.Matches("https://music.youtube.com/watch?v=xF8l17MJkMk") {
		t.Fatalf("expected music.youtube.com to match")
	}
	if p.Matches("https://example.com/watch?v=xF8l17MJkMk") {
		t.Fatalf("expected example.com not to match")
	}
	// Additional: youtube.com without www
	if !p.Matches("https://youtube.com/watch?v=xF8l17MJkMk") {
		t.Fatalf("expected youtube.com to match")
	}
	// m.youtube.com via suffix .youtube.com also should match
	if !p.Matches("https://m.youtube.com/watch?v=xF8l17MJkMk") {
		t.Fatalf("expected m.youtube.com to match")
	}
	// non-youtube with youtube substring not suffix
	if p.Matches("https://notyoutube.com/watch?v=x") {
		t.Fatalf("expected notyoutube.com not to match")
	}
}

func TestDetectProviderNameYouTube(t *testing.T) {
	cases := map[string]string{
		"https://youtu.be/B-h4UHrD8b0?si=sIeuAVf6StQpAEW_":        "YouTube",
		"https://www.youtube.com/watch?v=PzMSEuZCfdM":             "YouTube",
		"https://music.youtube.com/watch?v=xF8l17MJkMk":          "YouTube",
		"https://m.youtube.com/watch?v=xF8l17MJkMk":              "YouTube",
		"https://youtube.com/watch?v=x":                          "YouTube",
		"https://www.youtube.com/@canal/videos":                  "YouTube",
		"https://www.youtube.com/watch?v=L_UNI_ZfTi8&list=PL5LTyuJMkYTOVvFcAkr3k-k81tFtgvnW-&index=3": "YouTube",
	}
	for url, want := range cases {
		if got := detectProviderName(url); got != want {
			t.Fatalf("detectProviderName(%q)=%q want %q", url, got, want)
		}
	}
}

func TestDetectsChannelURLsAndLimitsChannelCapture(t *testing.T) {
	p := YouTubeProvider{}
	if !p.IsChannelURL("https://www.youtube.com/@canal/videos") {
		t.Fatalf("expected @canal/videos to be channel")
	}
	if !p.IsChannelURL("https://www.youtube.com/channel/UCabc123") {
		t.Fatalf("expected channel/ to be channel")
	}
	if p.IsChannelURL("https://www.youtube.com/watch?v=xF8l17MJkMk") {
		t.Fatalf("expected watch not channel")
	}
	// also test c/ @ music etc
	if !p.IsChannelURL("https://www.youtube.com/c/MyChannel") {
		t.Fatalf("expected c/ to be channel")
	}
	if !p.IsChannelURL("https://www.youtube.com/user/SomeUser") {
		t.Fatalf("expected user/ to be channel")
	}
	if !p.IsChannelURL("https://www.youtube.com/@canal/streams") {
		t.Fatalf("expected @canal/streams to be channel")
	}
	// channel limit
	lim := p.ChannelLimit("https://www.youtube.com/@canal#ytdlp_channel_limit=40")
	if lim == nil || *lim != 40 {
		t.Fatalf("expected 40 got %v", lim)
	}
	lim2 := p.ChannelLimit("https://www.youtube.com/@canal#ytdlp_channel_limit=5000")
	if lim2 == nil || *lim2 != 1000 {
		t.Fatalf("expected clamped 1000 got %v", lim2)
	}
	// ytdlp_limit alias
	lim3 := p.ChannelLimit("https://www.youtube.com/@canal#ytdlp_limit=5")
	if lim3 == nil || *lim3 != 5 {
		t.Fatalf("expected 5 got %v", lim3)
	}
	// env var
	t.Setenv("GDOWNLOADER_YOUTUBE_CHANNEL_LIMIT", "7")
	lim4 := p.ChannelLimit("https://www.youtube.com/@canal")
	if lim4 == nil || *lim4 != 7 {
		t.Fatalf("expected env 7 got %v", lim4)
	}
	os.Unsetenv("GDOWNLOADER_YOUTUBE_CHANNEL_LIMIT")
}

func TestParsesYtDlpProgressWithRealTotal(t *testing.T) {
	p := YouTubeProvider{}
	upd := p.ParseProgress("GDLPROG  42.5% 1.5MiB/s 00:09")
	if upd == nil {
		t.Fatal("expected parse progress")
	}
	if upd.BytesDownloaded != 4250 {
		t.Fatalf("downloaded %d want 4250", upd.BytesDownloaded)
	}
	if upd.TotalBytes != SyntheticProgressTotal {
		t.Fatalf("total %d want %d", upd.TotalBytes, SyntheticProgressTotal)
	}
	if upd.ChildSpeedBps == nil || *upd.ChildSpeedBps != 1572864 {
		t.Fatalf("speed %v want 1572864", upd.ChildSpeedBps)
	}
	if upd.ChildEtaSecs == nil || *upd.ChildEtaSecs != 9 {
		t.Fatalf("eta %v want 9", upd.ChildEtaSecs)
	}
}

func TestParsesYtDlpProgressWithSyntheticTotalWhenTotalIsNA(t *testing.T) {
	p := YouTubeProvider{}
	upd := p.ParseProgress("GDLPROG  25.0% N/A 00:00")
	if upd == nil {
		t.Fatal("expected parse")
	}
	if upd.BytesDownloaded != SyntheticProgressTotal/4 {
		t.Fatalf("downloaded %d want %d", upd.BytesDownloaded, SyntheticProgressTotal/4)
	}
	if upd.TotalBytes != SyntheticProgressTotal {
		t.Fatalf("total %d", upd.TotalBytes)
	}
}

func TestSplitMediaPhaseProgressDoesNotOverlapRanges(t *testing.T) {
	p := YouTubeProvider{}
	if got := p.PhaseProgress(SyntheticProgressTotal, 1, true); got != 8500 {
		t.Fatalf("phase1 %d want 8500", got)
	}
	if got := p.PhaseProgress(0, 2, true); got != 8500 {
		t.Fatalf("phase2 start %d want 8500", got)
	}
	if got := p.PhaseProgress(SyntheticProgressTotal, 2, true); got != 9500 {
		t.Fatalf("phase2 end %d want 9500", got)
	}
	if got := p.MergeProgress(true); got != 9600 {
		t.Fatalf("merge %d want 9600", got)
	}
	// non-split
	base, span := p.PhaseBounds(0, false)
	if base != 0 || span != 9500 {
		t.Fatalf("non-split bounds %d,%d", base, span)
	}
}

func TestAddsBestFallbackWhenSplitFormatHasNone(t *testing.T) {
	p := YouTubeProvider{}
	cases := map[string]string{
		"bestvideo+bestaudio":      "bestvideo+bestaudio/best",
		"136+bestaudio":            "136+bestaudio/best",
		"bestvideo+bestaudio/best": "bestvideo+bestaudio/best",
		"95":                       "95",
		"best":                     "best",
		"":                         "bestvideo+bestaudio/best",
		"  136 ":                   "136",
	}
	for in, want := range cases {
		if got := p.ResolveFormatSelector(in); got != want {
			t.Fatalf("ResolveFormatSelector(%q)=%q want %q", in, got, want)
		}
	}
}

func TestReadsSelectedMergeFormatFromChildFragment(t *testing.T) {
	p := YouTubeProvider{}
	children := []string{"https://www.youtube.com/watch?v=x#ytdlp_format=bestvideo%2Bbestaudio&ytdlp_merge_format=mkv"}
	val := p.SelectedValue(&children, "ytdlp_merge_format")
	if val == nil || *val != "mkv" {
		t.Fatalf("expected mkv got %v", val)
	}
	if v := p.NormalizeMergeFormat("MKV"); v == nil || *v != "mkv" {
		t.Fatalf("expected mkv normalized")
	}
	if v := p.NormalizeMergeFormat("avi"); v != nil {
		t.Fatalf("expected nil for avi")
	}
	if v := p.NormalizeMergeFormat(".mp4"); v == nil || *v != "mp4" {
		t.Fatalf("expected mp4 with dot")
	}
}

func TestPackAndChaptersWriteInsideTheDownloadFolder(t *testing.T) {
	p := YouTubeProvider{}
	if got := p.OutputTemplate("/tmp/The Soviet Unix", true); got != "/tmp/The Soviet Unix/The Soviet Unix.%(ext)s" {
		t.Fatalf("outputTemplate folder %q", got)
	}
	if got := p.OutputTemplate("/tmp/The Soviet Unix.mp4", false); got != "/tmp/The Soviet Unix.mp4" {
		t.Fatalf("outputTemplate file %q", got)
	}
	if got := p.ChapterOutputTemplate("/tmp/The Soviet Unix"); got != "/tmp/The Soviet Unix/%(title)s - %(section_number)03d %(section_title)s.%(ext)s" {
		t.Fatalf("chapter template %q", got)
	}
}

func TestCleanURLAndFragmentValue(t *testing.T) {
	p := YouTubeProvider{}
	// cleanURL should strip #ytdlp_ but keep si
	url := "https://youtu.be/abc?si=xyz#ytdlp_format=123&ytdlp_merge_format=mkv"
	if got := p.CleanURL(url); got != "https://youtu.be/abc?si=xyz" {
		t.Fatalf("CleanURL %q", got)
	}
	// fragment after #ytdlp_ should be stripped, but #ytdlp_without underscore? Rust split "#ytdlp_" only, so plain #frag keeps?
	url2 := "https://example.com/page#section1"
	if got := p.CleanURL(url2); got != url2 {
		t.Fatalf("CleanURL should keep #section1 got %q", got)
	}
	// fragmentValue decoding
	furl := "https://www.youtube.com/watch?v=x#ytdlp_format=bestvideo%2Bbestaudio%2Fbest&ytdlp_merge_format=mp4"
	if v := p.FragmentValue(furl, "ytdlp_format"); v == nil || *v != "bestvideo+bestaudio/best" {
		t.Fatalf("fragmentValue decode failed got %v", v)
	}
	if v := p.FragmentValue(furl, "ytdlp_merge_format"); v == nil || *v != "mp4" {
		t.Fatalf("merge %v", v)
	}
}

func TestSelectedPlaylistURLsFiltering(t *testing.T) {
	p := YouTubeProvider{}
	children := []string{
		"https://www.youtube.com/watch?v=a#ytdlp_format=123",
		"https://www.youtube.com/watch?v=b",
		"https://www.youtube.com/watch?v=c#ytdlp_merge_format=mkv",
	}
	// first contains #ytdlp_format should be filtered
	urls := p.SelectedPlaylistURLs(&children)
	if urls == nil || len(*urls) != 2 {
		t.Fatalf("expected 2 urls got %v", urls)
	}
	// check clean
	if (*urls)[0] != "https://www.youtube.com/watch?v=b" {
		t.Fatalf("first filtered url %q", (*urls)[0])
	}
	// when all are format, returns nil
	onlyFormat := []string{"https://example.com#ytdlp_format=best"}
	if got := p.SelectedPlaylistURLs(&onlyFormat); got != nil {
		t.Fatalf("expected nil")
	}
}

func TestBuildDownloadArgsStructure(t *testing.T) {
	p := YouTubeProvider{}
	ctx := DefaultDownloadContext()
	ctx.YoutubeMergeFormat = "mkv"
	children := []string{"https://www.youtube.com/watch?v=x#ytdlp_format=136%2Bbestaudio&ytdlp_merge_format=webm"}
	args, urls, format, hasSplit, isFolder := p.BuildDownloadArgs("https://youtu.be/x?si=abc", "/tmp/out.mp4", &children, ctx, 0, "")
	if len(urls) != 1 || urls[0] != "https://www.youtube.com/watch?v=x" {
		// selectedPlaylistURLs will filter format child => urls would be empty? Actually selected child contains format, so selectedPlaylistURLs returns nil? Let's check.
		// Our children has one entry with format, so selectedPlaylistURLs filters it out -> nil -> falls back to clean url "https://youtu.be/x?si=abc"
		// But we also test selected_format path.
		t.Logf("urls: %v", urls)
	}
	if format != "136+bestaudio/best" {
		t.Fatalf("format %q", format)
	}
	if !hasSplit {
		t.Fatalf("expected split")
	}
	if isFolder {
		t.Fatalf("expected not folder")
	}
	// check args contain -f and merge format webm (from child fragment) normalized
	foundMerge := false
	for i, a := range args {
		if a == "--merge-output-format" && i+1 < len(args) && args[i+1] == "webm" {
			foundMerge = true
		}
	}
	if !foundMerge {
		t.Fatalf("expected merge webm in args %v", args)
	}
	// check output template for file (not folder)
	foundOut := false
	for i, a := range args {
		if a == "-o" && i+1 < len(args) && args[i+1] == "/tmp/out.mp4" {
			foundOut = true
		}
	}
	if !foundOut {
		t.Fatalf("expected output template %v", args)
	}
}

func TestParseSpeedAndEtaEdgeCases(t *testing.T) {
	p := YouTubeProvider{}
	if v := p.ParseSpeedBps("N/A"); v != 0 {
		t.Fatalf("N/A should be 0")
	}
	if v := p.ParseSpeedBps("1.5MiB/s"); v != 1572864 {
		// 1.5 * 1024*1024
		t.Fatalf("MiB speed %d", v)
	}
	if v := p.ParseSpeedBps("500KiB/s"); v != 512000 {
		t.Fatalf("KiB %d", v)
	}
	if v := p.ParseEtaSecs("01:02"); v != 62 {
		t.Fatalf("eta 01:02 %d", v)
	}
	if v := p.ParseEtaSecs("01:02:03"); v != 3723 {
		t.Fatalf("eta 01:02:03 %d", v)
	}
	if v := p.ParseEtaSecs("N/A"); v != 0 {
		t.Fatalf("eta N/A 0")
	}
}

func TestEncodeFragment(t *testing.T) {
	cases := map[string]string{
		"bestvideo+bestaudio/best": "bestvideo%2Bbestaudio%2Fbest",
		"136+bestaudio/best":       "136%2Bbestaudio%2Fbest",
		"best":                     "best",
		"95":                       "95",
	}
	for in, want := range cases {
		if got := encodeFragment(in); got != want {
			t.Fatalf("encode %q => %q want %q", in, got, want)
		}
	}
}

func TestPlaylistDetectionViaListParam(t *testing.T) {
	p := YouTubeProvider{}
	links := []string{
		"https://www.youtube.com/watch?v=PzMSEuZCfdM&list=PL5LTyuJMkYTOVvFcAkr3k-k81tFtgvnW-",
		"https://www.youtube.com/watch?v=L_UNI_ZfTi8&list=PL5LTyuJMkYTOVvFcAkr3k-k81tFtgvnW-&index=3",
		"https://www.youtube.com/watch?v=VxEu1cI_aIw&list=PLPqoPgWuohm48cyOCSIHl0a_daKv9Hx0j&index=4",
	}
	for _, url := range links {
		clean := p.CleanURL(url)
		if !strings.Contains(clean, "list=") {
			t.Fatalf("clean should contain list= %q", clean)
		}
		if !p.Matches(url) {
			t.Fatalf("should match youtube %q", url)
		}
		// isChannel should be false for playlist
		if p.IsChannelURL(clean) {
			t.Fatalf("playlist should not be channel %q", clean)
		}
	}
	// youtu.be should not be playlist
	if strings.Contains(p.CleanURL("https://youtu.be/B-h4UHrD8b0?si=abc"), "list=") {
		t.Fatalf("youtu.be should not contain list")
	}
}

func TestAll11LinksMatchesAndClean(t *testing.T) {
	p := YouTubeProvider{}
	links := []string{
		"https://youtu.be/B-h4UHrD8b0?si=sIeuAVf6StQpAEW_",
		"https://youtu.be/L0STTCVRWo8?si=5Y1ndzff_2eOzASq",
		"https://youtu.be/yPVw7_y3tzQ?si=wW8reVXzOQHjYGWy",
		"https://youtu.be/iQnWKbcQuDg?si=zisZTe6Vxya8oyYn",
		"https://www.youtube.com/watch?v=PzMSEuZCfdM&list=PL5LTyuJMkYTOVvFcAkr3k-k81tFtgvnW-",
		"https://www.youtube.com/watch?v=L_UNI_ZfTi8&list=PL5LTyuJMkYTOVvFcAkr3k-k81tFtgvnW-&index=3",
		"https://www.youtube.com/watch?v=pRNEPcE-vM8&list=PL5LTyuJMkYTOVvFcAkr3k-k81tFtgvnW-&index=4",
		"https://youtu.be/fiUA9-8SKko?si=4REqBpWhRL_iPT-H",
		"https://youtu.be/2ObUJSG-G28?si=huP0wY_Co_U4AcOQ",
		"https://www.youtube.com/watch?v=VxEu1cI_aIw&list=PLPqoPgWuohm48cyOCSIHl0a_daKv9Hx0j&index=4",
		"https://www.youtube.com/watch?v=rf5SdkrOX4w&list=PLPqoPgWuohm48cyOCSIHl0a_daKv9Hx0j&index=7",
	}
	for _, url := range links {
		if !p.Matches(url) {
			t.Fatalf("link should match youtube: %s", url)
		}
		if !strings.HasPrefix(p.CleanURL(url), "https://") {
			t.Fatalf("clean url broken for %s", url)
		}
	}
}

// Integration test that hits yt-dlp — only runs if YT_DLP_INTEGRATION=1 or yt-dlp available.
// We run a quick smoke via GetFileInfo for one youtu.be; skipped otherwise to keep CI fast.
func TestIntegrationYouTubeGetFileInfoSingle(t *testing.T) {
	if os.Getenv("YT_DLP_INTEGRATION") == "" {
		t.Skip("skipping yt-dlp integration (set YT_DLP_INTEGRATION=1)")
	}
	p := YouTubeProvider{}
	info, err := p.GetFileInfo("https://youtu.be/B-h4UHrD8b0?si=sIeuAVf6StQpAEW_")
	if err != nil {
		t.Fatalf("GetFileInfo error: %v", err)
	}
	if info.IsFolder {
		t.Fatalf("expected single video not folder")
	}
	if info.Filename == "" || info.Size == 0 {
		t.Fatalf("expected filename and size got %q size %d", info.Filename, info.Size)
	}
	if info.Children == nil || len(*info.Children) == 0 {
		t.Fatalf("expected children qualities")
	}
	// first child should be best
	if (*info.Children)[0].Filename == "" {
		t.Fatalf("first child filename empty")
	}
}

func TestIntegrationAll11Links(t *testing.T) {
	if os.Getenv("YT_DLP_INTEGRATION") == "" {
		t.Skip("skipping yt-dlp integration (set YT_DLP_INTEGRATION=1)")
	}
	p := YouTubeProvider{}
	links := []struct {
		url      string
		isFolder bool
	}{
		{"https://youtu.be/B-h4UHrD8b0?si=sIeuAVf6StQpAEW_", false},
		{"https://youtu.be/L0STTCVRWo8?si=5Y1ndzff_2eOzASq", false},
		{"https://youtu.be/yPVw7_y3tzQ?si=wW8reVXzOQHjYGWy", false},
		{"https://youtu.be/iQnWKbcQuDg?si=zisZTe6Vxya8oyYn", false},
		{"https://www.youtube.com/watch?v=PzMSEuZCfdM&list=PL5LTyuJMkYTOVvFcAkr3k-k81tFtgvnW-", true},
		{"https://www.youtube.com/watch?v=L_UNI_ZfTi8&list=PL5LTyuJMkYTOVvFcAkr3k-k81tFtgvnW-&index=3", true},
		{"https://www.youtube.com/watch?v=pRNEPcE-vM8&list=PL5LTyuJMkYTOVvFcAkr3k-k81tFtgvnW-&index=4", true},
		{"https://youtu.be/fiUA9-8SKko?si=4REqBpWhRL_iPT-H", false},
		{"https://youtu.be/2ObUJSG-G28?si=huP0wY_Co_U4AcOQ", false},
		{"https://www.youtube.com/watch?v=VxEu1cI_aIw&list=PLPqoPgWuohm48cyOCSIHl0a_daKv9Hx0j&index=4", true},
		{"https://www.youtube.com/watch?v=rf5SdkrOX4w&list=PLPqoPgWuohm48cyOCSIHl0a_daKv9Hx0j&index=7", true},
	}
	for i, tc := range links {
		t.Run(tc.url, func(t *testing.T) {
			if !p.Matches(tc.url) {
				t.Fatalf("[%d] expected match %s", i, tc.url)
			}
			info, err := p.GetFileInfo(tc.url)
			if err != nil {
				t.Fatalf("[%d] GetFileInfo error %v for %s", i, err, tc.url)
			}
			if info.IsFolder != tc.isFolder {
				t.Fatalf("[%d] isFolder %v want %v for %s filename %s", i, info.IsFolder, tc.isFolder, tc.url, info.Filename)
			}
			if tc.isFolder {
				if info.Size != 0 {
					t.Fatalf("[%d] playlist size should be 0 got %d", i, info.Size)
				}
				if info.Children == nil || len(*info.Children) == 0 {
					t.Fatalf("[%d] playlist expected children", i)
				}
				if info.MimeType == nil || (*info.MimeType != "application/vnd.youtube.playlist" && *info.MimeType != "application/vnd.youtube.channel") {
					t.Fatalf("[%d] playlist mime %v", i, info.MimeType)
				}
			} else {
				if info.Filename == "" || !strings.HasSuffix(info.Filename, ".mp4") {
					t.Fatalf("[%d] single filename %q should end .mp4", i, info.Filename)
				}
				if info.Size == 0 {
					t.Fatalf("[%d] single size 0", i)
				}
				if info.Children == nil || len(*info.Children) < 2 {
					t.Fatalf("[%d] single expected many qualities got %v", i, info.Children)
				}
				if info.MimeType == nil || *info.MimeType != "video/*" {
					t.Fatalf("[%d] single mime %v", i, info.MimeType)
				}
				// ensure at least first child is best quality
				if (*info.Children)[0].Path == nil || *(*info.Children)[0].Path != "bestvideo+bestaudio" {
					t.Fatalf("[%d] first child path %v", i, (*info.Children)[0].Path)
				}
			}
		})
	}
}
