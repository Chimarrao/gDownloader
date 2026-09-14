package providers

import (
	"bufio"
	"context"
	"encoding/json"
	"fmt"
	"net/url"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"sort"
	"strconv"
	"strings"
	"time"

	"gdownloader-go/internal/models"
)

// Portado de backend/src/providers/youtube.rs — 100% compatible

type YouTubeProvider struct{}

const SyntheticProgressTotal uint64 = 10_000

// --- Ytdlp structs — mirror Rust Deserialize structs ---

type YtdlpInfo struct {
	ID         *string       `json:"id"`
	Title      *string       `json:"title"`
	Thumbnail  *string       `json:"thumbnail"`
	Duration   *float64      `json:"duration"`
	WebpageURL *string       `json:"webpage_url"`
	Entries    *[]YtdlpEntry `json:"entries"`
	Formats    *[]YtdlpFormat `json:"formats"`
	Uploader   *string       `json:"uploader"`
	ChannelID  *string       `json:"channel_id"`
}

type YtdlpChannelInfo struct {
	Thumbnail *string `json:"thumbnail"`
	Title     *string `json:"title"`
}

type YtdlpEntry struct {
	ID         *string  `json:"id"`
	Title      *string  `json:"title"`
	URL        *string  `json:"url"`
	WebpageURL *string  `json:"webpage_url"`
	Duration   *float64 `json:"duration"`
	Thumbnail  *string  `json:"thumbnail"`
}

type YtdlpFormat struct {
	FormatID      *string  `json:"format_id"`
	Ext           *string  `json:"ext"`
	Resolution    *string  `json:"resolution"`
	Width         *uint64  `json:"width"`
	Height        *uint64  `json:"height"`
	FPS           *float64 `json:"fps"`
	Filesize      *uint64  `json:"filesize"`
	FilesizeApprox *uint64 `json:"filesize_approx"`
	VCodec        *string  `json:"vcodec"`
	ACodec        *string  `json:"acodec"`
	TBR           *float64 `json:"tbr"`
	FormatNote    *string  `json:"format_note"`
}

// DownloadContext mirrors Rust crate::providers::DownloadContext (subset relevant for youtube)
type DownloadContext struct {
	DbPath                      *string
	ProxyMode                   string
	ProxyHost                   string
	ProxyPort                   uint16
	ProxyUsername               *string
	ProxyPassword               *string
	YoutubeUseCookies           bool
	YoutubeCookieBrowser        string
	YoutubeCookiesFile          string
	YoutubeMergeFormat          string
	YoutubeDownloadSubs         bool
	YoutubeSubLangs             string
	YoutubeEmbedSubs            bool
	YoutubeSplitChapters        bool
	YoutubeDownloadPack         bool
	RequestHeaders              map[string]string
	CachedChannelThumbnailURL   *string
}

func DefaultDownloadContext() DownloadContext {
	return DownloadContext{
		ProxyMode:            "none",
		YoutubeMergeFormat:   "mp4",
		YoutubeSubLangs:      "pt,en",
		YoutubeCookieBrowser: "chrome",
		RequestHeaders:       make(map[string]string),
	}
}

// ProgressUpdate mirrors Rust ProgressUpdate
type YouTubeProgressUpdate struct {
	BytesDownloaded      uint64
	TotalBytes           uint64
	ChildPath            *string
	ChildFilename        *string
	ChildBytesDownloaded *uint64
	ChildTotalBytes      *uint64
	ChildSpeedBps        *uint64
	ChildEtaSecs         *uint64
}

// Matches implements Rust YouTubeProvider::matches
func (p YouTubeProvider) Matches(rawURL string) bool {
	u := ParseURL(rawURL)
	if u == nil {
		return false
	}
	host := strings.ToLower(u.Hostname())
	return host == "youtu.be" ||
		strings.HasSuffix(host, ".youtube.com") ||
		host == "youtube.com" ||
		host == "music.youtube.com"
}

func ytdlpBin() string {
	if v := os.Getenv("GDOWNLOADER_YTDLP_BIN"); v != "" {
		return v
	}
	return "yt-dlp"
}

func cleanURL(raw string) string {
	parts := strings.SplitN(raw, "#ytdlp_", 2)
	return strings.TrimSpace(parts[0])
}

func fragmentValue(rawURL, key string) *string {
	idx := strings.Index(rawURL, "#")
	if idx < 0 {
		return nil
	}
	fragment := rawURL[idx+1:]
	for _, part := range strings.Split(fragment, "&") {
		eq := strings.Index(part, "=")
		if eq < 0 {
			continue
		}
		name := part[:eq]
		value := part[eq+1:]
		if name == key {
			decoded, err := url.PathUnescape(value)
			if err != nil {
				// fallback to QueryUnescape then raw
				if d2, err2 := url.QueryUnescape(value); err2 == nil {
					return &d2
				}
				return &value
			}
			// PathUnescape keeps + as +, which matches Rust urlencoding::decode
			// Also try to handle + encoded? keep as is.
			return &decoded
		}
	}
	return nil
}

func selectedFormat(selectedChildren *[]string) *string {
	if selectedChildren == nil {
		return nil
	}
	for _, child := range *selectedChildren {
		if v := fragmentValue(child, "ytdlp_format"); v != nil {
			return v
		}
	}
	return nil
}

func resolveFormatSelector(format string) string {
	trimmed := strings.TrimSpace(format)
	if trimmed == "" {
		return "bestvideo+bestaudio/best"
	}
	if strings.Contains(trimmed, "/") {
		return trimmed
	}
	if strings.Contains(trimmed, "+") || strings.Contains(trimmed, "bestvideo") {
		return trimmed + "/best"
	}
	return trimmed
}

func selectedValue(selectedChildren *[]string, key string) *string {
	if selectedChildren == nil {
		return nil
	}
	for _, child := range *selectedChildren {
		if v := fragmentValue(child, key); v != nil {
			return v
		}
	}
	return nil
}

func selectedFlag(selectedChildren *[]string, key string) bool {
	v := selectedValue(selectedChildren, key)
	if v == nil {
		return false
	}
	switch *v {
	case "1", "true", "yes", "on":
		return true
	default:
		return false
	}
}

func normalizeMergeFormat(value string) *string {
	normalized := strings.ToLower(strings.TrimSpace(strings.TrimLeft(strings.TrimSpace(value), ".")))
	switch normalized {
	case "mp4", "mkv", "webm":
		return &normalized
	default:
		return nil
	}
}

func selectedPlaylistURLs(selectedChildren *[]string) *[]string {
	if selectedChildren == nil {
		return nil
	}
	var urls []string
	for _, child := range *selectedChildren {
		if strings.Contains(child, "#ytdlp_format=") {
			continue
		}
		urls = append(urls, cleanURL(child))
	}
	if len(urls) == 0 {
		return nil
	}
	return &urls
}

func outputTemplate(destPath string, outputIsFolder bool) string {
	if !outputIsFolder {
		return destPath
	}
	base := filepath.Base(destPath)
	if strings.TrimSpace(base) == "" || base == "." || base == "/" {
		base = "YouTube"
	}
	// Need to handle when destPath is "/" or "."? fallback.
	// Use filepath.Join
	return filepath.Join(destPath, base+".%(ext)s")
}

func chapterOutputTemplate(destPath string) string {
	return filepath.Join(destPath, "%(title)s - %(section_number)03d %(section_title)s.%(ext)s")
}

func formatLabel(f YtdlpFormat) string {
	id := "best"
	if f.FormatID != nil && *f.FormatID != "" {
		id = *f.FormatID
	}
	ext := ""
	if f.Ext != nil {
		ext = *f.Ext
	}
	resolution := ""
	if f.Resolution != nil && *f.Resolution != "" && *f.Resolution != "audio only" {
		resolution = *f.Resolution
	} else if f.Height != nil {
		resolution = fmt.Sprintf("%dp", *f.Height)
	} else {
		resolution = "Audio"
	}
	note := ""
	if f.FormatNote != nil {
		note = *f.FormatNote
	}
	vcodec := "none"
	if f.VCodec != nil {
		vcodec = *f.VCodec
	}
	acodec := "none"
	if f.ACodec != nil {
		acodec = *f.ACodec
	}
	codec := "media"
	hasVideo := vcodec != "none"
	hasAudio := acodec != "none"
	switch {
	case hasVideo && hasAudio:
		codec = "video+audio"
	case hasVideo && !hasAudio:
		codec = "video"
	case !hasVideo && hasAudio:
		codec = "audio"
	}
	fps := ""
	if f.FPS != nil && *f.FPS >= 1.0 {
		fps = fmt.Sprintf("%dfps", uint64(*f.FPS+0.5)) // round
	}
	parts := []string{resolution, fps, ext, codec, note, fmt.Sprintf("#%s", id)}
	var filtered []string
	for _, p := range parts {
		if strings.TrimSpace(p) != "" {
			filtered = append(filtered, p)
		}
	}
	return strings.Join(filtered, " · ")
}

func resolutionLabel(f YtdlpFormat) string {
	var base string
	if f.Width != nil && f.Height != nil && *f.Width > 0 && *f.Height > 0 {
		base = fmt.Sprintf("%dx%d", *f.Width, *f.Height)
	} else {
		if f.Resolution != nil && *f.Resolution != "" && *f.Resolution != "audio only" {
			base = *f.Resolution
		} else if f.Height != nil {
			base = fmt.Sprintf("%dp", *f.Height)
		} else {
			base = "Audio"
		}
	}
	fps := ""
	if f.FPS != nil && *f.FPS >= 1.0 {
		fps = fmt.Sprintf(" %dfps", uint64(*f.FPS+0.5))
	}
	return base + fps
}

func formatSize(f YtdlpFormat, durationSecs uint64) uint64 {
	if f.Filesize != nil {
		return *f.Filesize
	}
	if f.FilesizeApprox != nil {
		return *f.FilesizeApprox
	}
	if f.TBR != nil {
		if durationSecs == 0 {
			return 0
		}
		// tbr is kbit/s -> bytes = tbr*1000/8*duration
		val := (*f.TBR * 1000.0 / 8.0) * float64(durationSecs)
		if val < 0 {
			return 0
		}
		return uint64(val + 0.5)
	}
	return 0
}

func bestAudioFormat(formats []YtdlpFormat, durationSecs uint64) *YtdlpFormat {
	var best *YtdlpFormat
	for i := range formats {
		f := &formats[i]
		vcodec := "none"
		if f.VCodec != nil {
			vcodec = *f.VCodec
		}
		acodec := "none"
		if f.ACodec != nil {
			acodec = *f.ACodec
		}
		if vcodec != "none" || acodec == "none" {
			continue
		}
		if best == nil {
			best = f
			continue
		}
		leftTBR := 0.0
		if f.TBR != nil {
			leftTBR = *f.TBR
		}
		rightTBR := 0.0
		if best.TBR != nil {
			rightTBR = *best.TBR
		}
		if leftTBR != rightTBR {
			if leftTBR > rightTBR {
				best = f
			}
			continue
		}
		if formatSize(*f, durationSecs) > formatSize(*best, durationSecs) {
			best = f
		}
	}
	return best
}

func bestVideoFormat(formats []YtdlpFormat, durationSecs uint64) *YtdlpFormat {
	var best *YtdlpFormat
	for i := range formats {
		f := &formats[i]
		vcodec := "none"
		if f.VCodec != nil {
			vcodec = *f.VCodec
		}
		if vcodec == "none" {
			continue
		}
		if best == nil {
			best = f
			continue
		}
		// compare height, fps, size
		leftH := uint64(0)
		if f.Height != nil {
			leftH = *f.Height
		}
		rightH := uint64(0)
		if best.Height != nil {
			rightH = *best.Height
		}
		if leftH != rightH {
			if leftH > rightH {
				best = f
			}
			continue
		}
		leftFPS := 0.0
		if f.FPS != nil {
			leftFPS = *f.FPS
		}
		rightFPS := 0.0
		if best.FPS != nil {
			rightFPS = *best.FPS
		}
		if leftFPS != rightFPS {
			if leftFPS > rightFPS {
				best = f
			}
			continue
		}
		if formatSize(*f, durationSecs) > formatSize(*best, durationSecs) {
			best = f
		}
	}
	return best
}

func buildFormatChildren(rawURL string, formats []YtdlpFormat, durationSecs uint64) []models.FileChildInfo {
	bestVideo := bestVideoFormat(formats, durationSecs)
	bestAudio := bestAudioFormat(formats, durationSecs)
	bestResolution := "Melhor disponivel"
	if bestVideo != nil {
		bestResolution = resolutionLabel(*bestVideo)
	}
	var bestSize uint64
	if bestVideo != nil {
		bestSize += formatSize(*bestVideo, durationSecs)
	}
	if bestAudio != nil {
		bestSize += formatSize(*bestAudio, durationSecs)
	}
	clean := cleanURL(rawURL)
	encodedBest := url.PathEscape("bestvideo+bestaudio/best")
	// Rust uses urlencoding::encode which encodes + as %2B, but PathEscape does similar? url.PathEscape encodes + as %2B? Actually PathEscape encodes? Let's check: PathEscape will not encode +? Need QueryEscape for +? Rust urlencoding encodes + as %2B. Go's PathEscape leaves unreserved? Test: PathEscape("bestvideo+bestaudio/best") => "bestvideo+bestaudio%2Fbest" — keeps + unencoded! Need QueryEscape for + but that encodes space as + but also encodes +? QueryEscape encodes + as %2B? Let's use url.QueryEscape then replace? Simpler mimic Rust: url.QueryEscape and keep / handling? Actually Rust encode will encode "/" as %2F, but we see Rust uses urlencoding::encode(&resolved) where resolved like "bestvideo+bestaudio/best" -> that becomes "bestvideo%2Bbestaudio%2Fbest". So "/" is encoded as %2F. Go QueryEscape does same? QueryEscape encodes "/" as %2F? No, QueryEscape encodes space as +, but leaves /? Actually QueryEscape encodes / as %2F? Let's check. We'll handle via custom: strings.ReplaceAll(url.PathEscape(...), "+", "%2B")? Let's test.
	// To ensure 1:1, do percent encoding similar to Rust: encode all except unreserved? We'll use url.QueryEscape then adjust?
	// Simplify: use url.QueryEscape and replace "+" with "%20" etc? We'll just use template that Rust would produce: it uses urlencoding::encode which percent-encodes everything except alphanumeric and -_.~ . So we can use url.PathEscape but ensure + is encoded.
	// We'll implement encodeFragment helper.
	encode := func(s string) string {
		return encodeFragment(s)
	}
	encodedBest = encode("bestvideo+bestaudio/best")

	children := []models.FileChildInfo{
		{
			Filename:  fmt.Sprintf("Melhor qualidade - %s", bestResolution),
			Size:      bestSize,
			MimeType:  strPtr("video/*"),
			IsFolder:  false,
			Path:      strPtr("bestvideo+bestaudio"),
			SourceURL: strPtr(fmt.Sprintf("%s#ytdlp_format=%s", clean, encodedBest)),
			BytesDownloaded: u64Ptr(0),
			SpeedBps:        u64Ptr(0),
			EtaSecs:         u64Ptr(0),
			Status:          statusPtr(models.StatusPending),
		},
	}

	// usable formats
	var usable []YtdlpFormat
	for _, f := range formats {
		if f.FormatID == nil || *f.FormatID == "" {
			continue
		}
		vcodec := "none"
		if f.VCodec != nil {
			vcodec = *f.VCodec
		}
		acodec := "none"
		if f.ACodec != nil {
			acodec = *f.ACodec
		}
		if vcodec == "none" && acodec == "none" {
			continue
		}
		usable = append(usable, f)
	}
	sort.Slice(usable, func(i, j int) bool {
		// b height desc then size desc
		hi := uint64(0)
		if usable[i].Height != nil {
			hi = *usable[i].Height
		}
		hj := uint64(0)
		if usable[j].Height != nil {
			hj = *usable[j].Height
		}
		if hi != hj {
			return hi > hj
		}
		si := formatSize(usable[i], durationSecs)
		sj := formatSize(usable[j], durationSecs)
		return si > sj
	})
	if len(usable) > 80 {
		usable = usable[:80]
	}
	for _, f := range usable {
		id := ""
		if f.FormatID != nil {
			id = *f.FormatID
		}
		if id == "" {
			id = "best"
		}
		hasVideo := false
		if f.VCodec != nil && *f.VCodec != "none" {
			hasVideo = true
		}
		hasAudio := false
		if f.ACodec != nil && *f.ACodec != "none" {
			hasAudio = true
		}
		resolved := id
		if hasVideo && !hasAudio {
			resolved = id + "+bestaudio/best"
		}
		mime := "video/*"
		if hasAudio && !hasVideo {
			mime = "audio/*"
		}
		children = append(children, models.FileChildInfo{
			Filename:  formatLabel(f),
			Size:      formatSize(f, durationSecs),
			MimeType:  strPtr(mime),
			IsFolder:  false,
			Path:      strPtr(id),
			SourceURL: strPtr(fmt.Sprintf("%s#ytdlp_format=%s", clean, encode(resolved))),
			BytesDownloaded: u64Ptr(0),
			SpeedBps:        u64Ptr(0),
			EtaSecs:         u64Ptr(0),
			Status:          statusPtr(models.StatusPending),
		})
	}
	return children
}

func encodeFragment(s string) string {
	// Mimic Rust urlencoding::encode — percent-encode all except ALPHA / DIGIT / "-" / "_" / "." / "~"
	var b strings.Builder
	for i := 0; i < len(s); i++ {
		ch := s[i]
		if (ch >= 'A' && ch <= 'Z') || (ch >= 'a' && ch <= 'z') || (ch >= '0' && ch <= '9') || ch == '-' || ch == '_' || ch == '.' || ch == '~' {
			b.WriteByte(ch)
		} else {
			b.WriteString(fmt.Sprintf("%%%02X", ch))
		}
	}
	return b.String()
}

func playlistEntryURL(entry YtdlpEntry) *string {
	if entry.WebpageURL != nil && *entry.WebpageURL != "" {
		return entry.WebpageURL
	}
	if entry.URL != nil {
		u := *entry.URL
		if strings.HasPrefix(u, "http://") || strings.HasPrefix(u, "https://") {
			return &u
		}
		if entry.ID != nil && *entry.ID != "" {
			constructed := fmt.Sprintf("https://www.youtube.com/watch?v=%s", *entry.ID)
			return &constructed
		}
		constructed := fmt.Sprintf("https://www.youtube.com/watch?v=%s", u)
		return &constructed
	}
	return nil
}

func isChannelURL(rawURL string) bool {
	parsed := ParseURL(rawURL)
	if parsed == nil {
		return false
	}
	host := strings.ToLower(parsed.Hostname())
	if !(host == "youtube.com" || strings.HasSuffix(host, ".youtube.com")) {
		return false
	}
	path := strings.Trim(strings.TrimSpace(parsed.Path), "/")
	lower := strings.ToLower(path)
	return strings.HasPrefix(lower, "@") ||
		strings.HasPrefix(lower, "channel/") ||
		strings.HasPrefix(lower, "c/") ||
		strings.HasPrefix(lower, "user/") ||
		strings.HasSuffix(lower, "/videos") ||
		strings.HasSuffix(lower, "/streams")
}

func channelLimit(rawURL string) *uint32 {
	if v := fragmentValue(rawURL, "ytdlp_channel_limit"); v != nil {
		if n, err := strconv.ParseUint(*v, 10, 32); err == nil {
			clamped := uint32(n)
			if clamped < 1 {
				clamped = 1
			}
			if clamped > 1000 {
				clamped = 1000
			}
			return &clamped
		}
	}
	if v := fragmentValue(rawURL, "ytdlp_limit"); v != nil {
		if n, err := strconv.ParseUint(*v, 10, 32); err == nil {
			clamped := uint32(n)
			if clamped < 1 {
				clamped = 1
			}
			if clamped > 1000 {
				clamped = 1000
			}
			return &clamped
		}
	}
	if envVal := os.Getenv("GDOWNLOADER_YOUTUBE_CHANNEL_LIMIT"); envVal != "" {
		if n, err := strconv.ParseUint(envVal, 10, 32); err == nil {
			clamped := uint32(n)
			if clamped < 1 {
				clamped = 1
			}
			if clamped > 1000 {
				clamped = 1000
			}
			return &clamped
		}
	}
	return nil
}

func isSessionRequiredError(err error) bool {
	if err == nil {
		return false
	}
	msg := strings.ToLower(err.Error())
	return strings.Contains(msg, "sessão válida") || strings.Contains(msg, "session") || strings.Contains(msg, "sign in") || strings.Contains(msg, "confirm you're not a bot") || strings.Contains(msg, "cookies") || strings.Contains(msg, "confirm you") || strings.Contains(msg, "bot")
}

func infoWithCookiesRetry(cleanURL string, isPlaylist bool, limit *uint32, ctx *DownloadContext) (*YtdlpInfo, error) {
	info, err := readInfoSeparated(cleanURL, isPlaylist, limit, ctx)
	if err != nil && isSessionRequiredError(err) {
		// Try without any cookies first (public videos)
		var noCookieCtx DownloadContext
		if ctx != nil {
			noCookieCtx = *ctx
		} else {
			noCookieCtx = DefaultDownloadContext()
		}
		noCookieCtx.YoutubeUseCookies = false
		noCookieCtx.YoutubeCookiesFile = ""
		noCookieCtx.YoutubeCookieBrowser = ""
		if info2, err2 := readInfoSeparated(cleanURL, isPlaylist, limit, &noCookieCtx); err2 == nil {
			return info2, nil
		}
		// Then try with chrome auto-extract
		var chromeCtx DownloadContext
		if ctx != nil {
			chromeCtx = *ctx
		} else {
			chromeCtx = DefaultDownloadContext()
		}
		chromeCtx.YoutubeUseCookies = true
		chromeCtx.YoutubeCookiesFile = ""
		if strings.TrimSpace(chromeCtx.YoutubeCookieBrowser) == "" {
			chromeCtx.YoutubeCookieBrowser = "chrome"
		}
		if info2, err2 := readInfoSeparated(cleanURL, isPlaylist, limit, &chromeCtx); err2 == nil {
			return info2, nil
		}
	}
	return info, err
}

func isPublicEntry(entry YtdlpEntry) bool {
	title := ""
	if entry.Title != nil {
		title = strings.ToLower(*entry.Title)
	}
	blocked := strings.Contains(title, "private video") ||
		strings.Contains(title, "deleted video") ||
		strings.Contains(title, "unavailable") ||
		strings.Contains(title, "members-only") ||
		strings.Contains(title, "members only")
	if blocked {
		return false
	}
	return playlistEntryURL(entry) != nil
}

func applyCookiesArgs(args *[]string, ctx DownloadContext) {
	if !ctx.YoutubeUseCookies {
		return
	}
	if strings.TrimSpace(ctx.YoutubeCookiesFile) != "" {
		*args = append(*args, "--cookies", ctx.YoutubeCookiesFile)
		return
	}
	*args = append(*args, "--cookies-from-browser")
	browser := ctx.YoutubeCookieBrowser
	if strings.TrimSpace(browser) == "" {
		browser = "chrome"
	}
	*args = append(*args, browser)
}

func applyProxyArgs(args *[]string, ctx DownloadContext) {
	var proxy string
	switch ctx.ProxyMode {
	case "tor":
		host := ctx.ProxyHost
		if strings.TrimSpace(host) == "" {
			host = "127.0.0.1"
		}
		port := ctx.ProxyPort
		if port == 0 {
			port = 9050
		}
		if ctx.ProxyUsername != nil && *ctx.ProxyUsername != "" {
			pass := ""
			if ctx.ProxyPassword != nil {
				pass = *ctx.ProxyPassword
			}
			proxy = fmt.Sprintf("socks5://%s:%s@%s:%d", *ctx.ProxyUsername, pass, host, port)
		} else {
			proxy = fmt.Sprintf("socks5://%s:%d", host, port)
		}
	case "socks5":
		proxy = fmt.Sprintf("socks5://%s:%d", ctx.ProxyHost, ctx.ProxyPort)
	case "http", "https":
		proxy = fmt.Sprintf("http://%s:%d", ctx.ProxyHost, ctx.ProxyPort)
	default:
		proxy = ""
	}
	if proxy != "" {
		*args = append(*args, "--proxy", proxy)
	}
}

func readInfo(ctx context.Context, rawURL string, flatPlaylist bool, playlistEnd *uint32, dctx *DownloadContext) (*YtdlpInfo, error) {
	var args []string
	if dctx != nil {
		applyCookiesArgs(&args, *dctx)
		applyProxyArgs(&args, *dctx)
	}
	args = append(args, "-J", "--no-warnings")
	if flatPlaylist {
		args = append(args, "--flat-playlist")
		if playlistEnd != nil {
			args = append(args, "--playlist-end", fmt.Sprintf("%d", *playlistEnd))
		}
	} else {
		args = append(args, "--no-playlist")
	}
	args = append(args, rawURL)

	bin := ytdlpBin()
	// timeout 45s like Rust
	cctx, cancel := context.WithTimeout(ctx, 45*time.Second)
	defer cancel()
	cmd := exec.CommandContext(cctx, bin, args...)
	out, err := cmd.CombinedOutput()
	// Need to differentiate? Rust checks status success and uses stderr or stdout separately.
	// We'll run with Output and capture both — CombinedOutput already merges.
	// To mimic exact error, check exit err.
	if err != nil {
		// If context deadline, wrap
		if cctx.Err() == context.DeadlineExceeded {
			return nil, fmt.Errorf("yt-dlp demorou demais para ler metadados do YouTube")
		}
		// Try to extract stderr from output
		msg := strings.TrimSpace(string(out))
		if msg == "" {
			msg = "yt-dlp falhou ao ler o link do YouTube"
		}
		return nil, fmt.Errorf("%s", msg)
	}
	// Successful: output is JSON; but CombinedOutput includes possibly warnings but we passed --no-warnings.
	// Need to get just stdout? Using CombinedOutput mixes; use Output instead for clean separation.
	// However above we already handled err nil path; we need to re-run logic to capture correctly: instead use cmd.Output capturing stdout and stderr separately.
	// For simplicity we captured via exec with Output — but we already used CombinedOutput. We'll instead parse out.
	// Since warnings are disabled, parsing combined should be same; but we trimmed? Use out as JSON.
	// In case of success, combined output is JSON; parse.
	var info YtdlpInfo
	if err := json.Unmarshal(out, &info); err != nil {
		return nil, fmt.Errorf("falha ao parsear JSON yt-dlp: %w output=%s", err, string(out[:ytMin(len(out), 500)]))
	}
	return &info, nil
}

// readInfoStrict uses separate stdout/stderr like Rust to give better errors — we keep alternate impl for non-combined.
// But above Combined suffices. We'll keep helper to re-issue with correct exec if JSON parse fails due to interleaving.

func readInfoSeparated(rawURL string, flatPlaylist bool, playlistEnd *uint32, dctx *DownloadContext) (*YtdlpInfo, error) {
	var args []string
	if dctx != nil {
		applyCookiesArgs(&args, *dctx)
		applyProxyArgs(&args, *dctx)
	}
	args = append(args, "-J", "--no-warnings")
	if flatPlaylist {
		args = append(args, "--flat-playlist")
		if playlistEnd != nil {
			args = append(args, "--playlist-end", fmt.Sprintf("%d", *playlistEnd))
		}
	} else {
		args = append(args, "--no-playlist")
	}
	args = append(args, rawURL)
	ctx, cancel := context.WithTimeout(context.Background(), 45*time.Second)
	defer cancel()
	cmd := exec.CommandContext(ctx, ytdlpBin(), args...)
	stdout, err := cmd.Output()
	if err != nil {
		if ctx.Err() == context.DeadlineExceeded {
			return nil, fmt.Errorf("yt-dlp demorou demais para ler metadados do YouTube")
		}
		if ee, ok := err.(*exec.ExitError); ok {
			stderr := strings.TrimSpace(string(ee.Stderr))
			if stderr == "" {
				stderr = "yt-dlp falhou ao ler o link do YouTube"
			}
			return nil, fmt.Errorf("%s", stderr)
		}
		return nil, fmt.Errorf("yt-dlp falhou ao ler o link do YouTube: %w", err)
	}
	var info YtdlpInfo
	if err := json.Unmarshal(stdout, &info); err != nil {
		return nil, err
	}
	return &info, nil
}

func ytMin(a, b int) int { if a < b { return a }; return b }

func readChannelInfo(channelID string) *YtdlpChannelInfo {
	channelURL := fmt.Sprintf("https://www.youtube.com/channel/%s", channelID)
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	cmd := exec.CommandContext(ctx, ytdlpBin(), "-J", "--flat-playlist", "--playlist-items", "0", "--no-warnings", channelURL)
	out, err := cmd.Output()
	if err != nil {
		return nil
	}
	var info YtdlpChannelInfo
	if err := json.Unmarshal(out, &info); err != nil {
		return nil
	}
	return &info
}

// InfoFor mirrors Rust info_for
func (p YouTubeProvider) InfoFor(rawURL string, dctx *DownloadContext) (models.FileInfo, error) {
	clean := cleanURL(rawURL)
	// Watch URLs with both v= and list= are single videos from a playlist, not the whole playlist.
	hasVideoParam := func(u string) bool {
		parsed := ParseURL(u)
		if parsed == nil {
			return false
		}
		q := parsed.Query()
		return q.Get("v") != "" || strings.Contains(strings.ToLower(parsed.Path), "/shorts/")
	}
	isWatchWithVideo := hasVideoParam(clean)
	isPlaylistParam := strings.Contains(clean, "list=")
	isPlaylistURL := strings.Contains(strings.ToLower(clean), "/playlist")
	isPlaylist := (isPlaylistURL || (isPlaylistParam && !isWatchWithVideo))
	isChannel := !isPlaylist && isChannelURL(clean)
	if isPlaylist || isChannel {
		var limit *uint32
		if isChannel {
			limit = channelLimit(rawURL)
		}
		info, err := infoWithCookiesRetry(clean, true, limit, dctx)
		if err != nil {
			return models.FileInfo{}, err
		}
		entries := []YtdlpEntry{}
		if info.Entries != nil {
			entries = *info.Entries
		}
		var children []models.FileChildInfo
		for _, entry := range entries {
			if isChannel && !isPublicEntry(entry) {
				continue
			}
			src := playlistEntryURL(entry)
			if src == nil {
				continue
			}
			title := "Video"
			if entry.Title != nil && *entry.Title != "" {
				title = *entry.Title
			}
			filename := SanitizeFilename(title, "Video")
			idCopy := ""
			if entry.ID != nil {
				idCopy = *entry.ID
			}
			var pathPtr *string
			if idCopy != "" {
				pathPtr = &idCopy
			}
			children = append(children, models.FileChildInfo{
				Filename:  filename,
				Size:      0,
				MimeType:  strPtr("video/*"),
				IsFolder:  false,
				Path:      pathPtr,
				SourceURL: src,
				BytesDownloaded: u64Ptr(0),
				SpeedBps:        u64Ptr(0),
				EtaSecs:         u64Ptr(0),
				Status:          statusPtr(models.StatusPending),
			})
		}
		if len(children) == 0 {
			if isChannel {
				return models.FileInfo{}, fmt.Errorf("Canal do YouTube sem videos publicos acessiveis")
			}
			return models.FileInfo{}, fmt.Errorf("Playlist do YouTube sem videos acessiveis")
		}
		titleFallback := "YouTube Playlist"
		if isChannel {
			titleFallback = "YouTube Channel"
		}
		title := titleFallback
		if info.Title != nil && *info.Title != "" {
			title = *info.Title
		}
		filename := SanitizeFilename(title, titleFallback)
		mime := "application/vnd.youtube.playlist"
		if isChannel {
			mime = "application/vnd.youtube.channel"
		}
		return models.FileInfo{
			Filename: filename,
			Size:     0,
			MimeType: strPtr(mime),
			IsFolder: true,
			Children: &children,
		}, nil
	}

	info, err := infoWithCookiesRetry(clean, false, nil, dctx)
	if err != nil {
		return models.FileInfo{}, err
	}
	title := "YouTube Video"
	if info.Title != nil && *info.Title != "" {
		title = *info.Title
	}
	safeTitle := SanitizeFilename(title, "YouTube Video")
	mergeFormat := "mp4"
	if dctx != nil {
		if v := normalizeMergeFormat(dctx.YoutubeMergeFormat); v != nil {
			mergeFormat = *v
		}
	}
	filename := fmt.Sprintf("%s.%s", safeTitle, mergeFormat)
	var durationSecs *uint64
	durationVal := uint64(0)
	if info.Duration != nil && *info.Duration > 0 {
		// round like Rust: .max(0.0).round() as u64
		// Go: math.Round
		durationVal = uint64(*info.Duration + 0.5)
		if durationVal > 0 {
			durationSecs = &durationVal
		}
	}
	var formats []YtdlpFormat
	if info.Formats != nil {
		formats = *info.Formats
	}
	children := buildFormatChildren(clean, formats, durationVal)
	var maxSize uint64
	for _, ch := range children {
		if ch.Size > maxSize {
			maxSize = ch.Size
		}
	}
	// channel thumbnail logic — reuse cache if present, else fetch
	var channelThumb *string
	if dctx != nil && dctx.CachedChannelThumbnailURL != nil && *dctx.CachedChannelThumbnailURL != "" {
		channelThumb = dctx.CachedChannelThumbnailURL
	} else if info.ChannelID != nil && *info.ChannelID != "" {
		if ci := readChannelInfo(*info.ChannelID); ci != nil {
			channelThumb = ci.Thumbnail
		}
	}
	var thumb *string
	if info.Thumbnail != nil {
		thumb = info.Thumbnail
	}
	var channelName *string
	if info.Uploader != nil {
		channelName = info.Uploader
	}
	return models.FileInfo{
		Filename:            filename,
		Size:                maxSize,
		DurationSecs:        durationSecs,
		MimeType:            strPtr("video/*"),
		IsFolder:            false,
		Children:            &children,
		ThumbnailURL:        thumb,
		ChannelName:         channelName,
		ChannelThumbnailURL: channelThumb,
	}, nil
}

// GetFileInfo is convenience wrapper without context (like Rust Provider::get_file_info)
func (p YouTubeProvider) GetFileInfo(rawURL string) (models.FileInfo, error) {
	return p.InfoFor(rawURL, nil)
}

// GetFileInfoWithContext mirrors Rust Provider::get_file_info_with_context
func (p YouTubeProvider) GetFileInfoWithContext(rawURL string, ctx DownloadContext) (models.FileInfo, error) {
	return p.InfoFor(rawURL, &ctx)
}

// --- Download helpers ---

func parseSpeedBps(value string) uint64 {
	trimmed := strings.TrimSpace(value)
	if trimmed == "" || trimmed == "N/A" || trimmed == "--" {
		return 0
	}
	re := regexp.MustCompile(`([0-9.]+)\s*([KMGT]?)(?:i?B)?/s`)
	m := re.FindStringSubmatch(trimmed)
	if len(m) < 3 {
		return 0
	}
	num, err := strconv.ParseFloat(m[1], 64)
	if err != nil {
		return 0
	}
	mult := 1.0
	switch strings.ToUpper(m[2]) {
	case "T":
		mult = 1024.0 * 1024.0 * 1024.0 * 1024.0
	case "G":
		mult = 1024.0 * 1024.0 * 1024.0
	case "M":
		mult = 1024.0 * 1024.0
	case "K":
		mult = 1024.0
	}
	return uint64(num*mult + 0.5)
}

func parseEtaSecs(value string) uint64 {
	v := strings.TrimSpace(value)
	if v == "" || v == "N/A" || v == "--" {
		return 0
	}
	parts := strings.Split(v, ":")
	var nums []uint64
	for _, p := range parts {
		if n, err := strconv.ParseUint(p, 10, 64); err == nil {
			nums = append(nums, n)
		} else {
			return 0
		}
	}
	switch len(nums) {
	case 3:
		return nums[0]*3600 + nums[1]*60 + nums[2]
	case 2:
		return nums[0]*60 + nums[1]
	case 1:
		return nums[0]
	default:
		return 0
	}
}

func parseProgress(line string) *YouTubeProgressUpdate {
	// Rust regex: GDLPROG\s*([0-9.]+)%\s+(.+?)\s+(\S+)\s*$
	re := regexp.MustCompile(`GDLPROG\s*([0-9.]+)%\s+(.+?)\s+(\S+)\s*$`)
	m := re.FindStringSubmatch(line)
	if len(m) < 4 {
		return nil
	}
	percent, err := strconv.ParseFloat(m[1], 64)
	if err != nil {
		percent = 0
	}
	if percent < 0 {
		percent = 0
	}
	if percent > 100 {
		percent = 100
	}
	speed := parseSpeedBps(m[2])
	eta := parseEtaSecs(m[3])
	total := SyntheticProgressTotal
	downloaded := uint64(float64(total)*percent/100.0 + 0.5)
	return &YouTubeProgressUpdate{
		BytesDownloaded: downloaded,
		TotalBytes:      total,
		ChildSpeedBps:   &speed,
		ChildEtaSecs:    &eta,
	}
}

func phaseBounds(phaseCount uint32, hasSplitMedia bool) (uint64, uint64) {
	if !hasSplitMedia {
		return 0, 9500
	}
	switch phaseCount {
	case 0, 1:
		return 0, 8500
	case 2:
		return 8500, 1000
	default:
		return 9500, 0
	}
}

func phaseProgress(downloaded uint64, phaseCount uint32, hasSplitMedia bool) uint64 {
	base, span := phaseBounds(phaseCount, hasSplitMedia)
	if span == 0 {
		return base
	}
	if downloaded > SyntheticProgressTotal {
		downloaded = SyntheticProgressTotal
	}
	return base + downloaded*span/SyntheticProgressTotal
}

func mergeProgress(hasSplitMedia bool) uint64 {
	if hasSplitMedia {
		return 9600
	}
	return 9750
}

// BuildDownloadArgs mirrors Rust download_with_context arg building — exposed for testing.
func (p YouTubeProvider) BuildDownloadArgs(rawURL, destPath string, selectedChildren *[]string, ctx DownloadContext, speedLimitBps uint64, ffmpegBin string) (args []string, urls []string, format string, hasSplitMedia bool, outputIsFolder bool) {
	downloadPack := ctx.YoutubeDownloadPack || selectedFlag(selectedChildren, "ytdlp_download_pack")
	if !downloadPack {
		if v := fragmentValue(rawURL, "ytdlp_download_pack"); v != nil {
			switch *v {
			case "1", "true", "yes", "on":
				downloadPack = true
			}
		}
	}
	splitChapters := ctx.YoutubeSplitChapters || selectedFlag(selectedChildren, "ytdlp_split_chapters")
	if !splitChapters {
		if v := fragmentValue(rawURL, "ytdlp_split_chapters"); v != nil {
			switch *v {
			case "1", "true", "yes", "on":
				splitChapters = true
			}
		}
	}
	subLangs := selectedValue(selectedChildren, "ytdlp_sub_langs")
	if subLangs == nil {
		subLangs = fragmentValue(rawURL, "ytdlp_sub_langs")
	}
	outputIsFolder = downloadPack || splitChapters
	if sel := selectedPlaylistURLs(selectedChildren); sel != nil {
		urls = *sel
	} else {
		urls = []string{cleanURL(rawURL)}
	}
	formatSel := ""
	if v := selectedFormat(selectedChildren); v != nil {
		formatSel = *v
	} else if v := fragmentValue(rawURL, "ytdlp_format"); v != nil {
		formatSel = *v
	} else {
		formatSel = "bestvideo+bestaudio/best"
	}
	format = resolveFormatSelector(formatSel)
	hasSplitMedia = strings.Contains(format, "+") || strings.Contains(format, "bestvideo")
	selectedMergeFormat := selectedValue(selectedChildren, "ytdlp_merge_format")
	if selectedMergeFormat == nil {
		if v := fragmentValue(rawURL, "ytdlp_merge_format"); v != nil {
			selectedMergeFormat = v
		}
	}
	var normalizedMerge *string
	if selectedMergeFormat != nil {
		normalizedMerge = normalizeMergeFormat(*selectedMergeFormat)
	}
	mergeFormat := ""
	if normalizedMerge != nil {
		mergeFormat = *normalizedMerge
	} else {
		if v := strings.TrimSpace(ctx.YoutubeMergeFormat); v != "" {
			if nv := normalizeMergeFormat(v); nv != nil {
				mergeFormat = *nv
			}
		}
	}
	if mergeFormat == "" {
		mergeFormat = "mp4"
	}
	writeThumbnail := selectedFlag(selectedChildren, "ytdlp_write_thumbnail")
	if !writeThumbnail {
		if v := fragmentValue(rawURL, "ytdlp_write_thumbnail"); v != nil {
			switch *v {
			case "1", "true", "yes", "on":
				writeThumbnail = true
			}
		}
	}
	writeSubsSelected := selectedFlag(selectedChildren, "ytdlp_write_subs")
	if !writeSubsSelected {
		if v := fragmentValue(rawURL, "ytdlp_write_subs"); v != nil {
			switch *v {
			case "1", "true", "yes", "on":
				writeSubsSelected = true
			}
		}
	}
	multiAudio := selectedFlag(selectedChildren, "ytdlp_multi_audio")
	if !multiAudio {
		if v := fragmentValue(rawURL, "ytdlp_multi_audio"); v != nil {
			switch *v {
			case "1", "true", "yes", "on":
				multiAudio = true
			}
		}
	}
	// For first URL only, mimic Rust loop building args per item — we'll build for first item as example
	// But to allow caller to loop, we return base args without final item_url; caller appends item_url.
	var a []string
	applyCookiesArgs(&a, ctx)
	applyProxyArgs(&a, ctx)
	a = append(a,
		"-f", format,
		"--no-playlist",
		"--merge-output-format", mergeFormat,
		"--newline",
		"--progress-template", "download:GDLPROG %(progress._percent_str)s %(progress._speed_str)s %(progress._eta_str)s",
		"--retries", "3",
		"--fragment-retries", "3",
		"-c",
		"-o", outputTemplate(destPath, outputIsFolder),
	)
	if speedLimitBps > 0 {
		a = append(a, "--limit-rate", fmt.Sprintf("%d", speedLimitBps))
	}
	if writeThumbnail || downloadPack {
		a = append(a, "--write-thumbnail")
	}
	if downloadPack {
		a = append(a, "--write-description", "--write-info-json", "--ignore-errors")
	}
	if multiAudio {
		a = append(a, "--audio-multistreams")
	}
	if downloadPack || ctx.YoutubeDownloadSubs || writeSubsSelected {
		a = append(a, "--write-subs", "--write-auto-sub", "--sub-lang")
		lang := ""
		if subLangs != nil {
			lang = *subLangs
		}
		if strings.TrimSpace(lang) == "" {
			lang = ctx.YoutubeSubLangs
		}
		if strings.TrimSpace(lang) == "" {
			lang = "pt,en"
		}
		a = append(a, lang)
		if ctx.YoutubeEmbedSubs {
			a = append(a, "--embed-subs")
		}
	}
	if splitChapters {
		a = append(a, "--split-chapters", "-o", fmt.Sprintf("chapter:%s", chapterOutputTemplate(destPath)))
	}
	if strings.TrimSpace(ffmpegBin) != "" {
		a = append(a, "--ffmpeg-location", ffmpegBin)
	}
	return a, urls, format, hasSplitMedia, outputIsFolder
}

// Download executes yt-dlp download similarly to Rust download_with_context but simplified sync version.
// It returns total bytes downloaded (from filesystem) or error.
func (p YouTubeProvider) Download(rawURL, destPath string, selectedChildren *[]string, ctx DownloadContext, speedLimitBps uint64, progressCb func(YouTubeProgressUpdate)) (uint64, error) {
	downloadPack := ctx.YoutubeDownloadPack || selectedFlag(selectedChildren, "ytdlp_download_pack")
	if !downloadPack {
		if v := fragmentValue(rawURL, "ytdlp_download_pack"); v != nil {
			switch *v {
			case "1", "true", "yes", "on":
				downloadPack = true
			}
		}
	}
	splitChapters := ctx.YoutubeSplitChapters || selectedFlag(selectedChildren, "ytdlp_split_chapters")
	if !splitChapters {
		if v := fragmentValue(rawURL, "ytdlp_split_chapters"); v != nil {
			switch *v {
			case "1", "true", "yes", "on":
				splitChapters = true
			}
		}
	}
	subLangs := selectedValue(selectedChildren, "ytdlp_sub_langs")
	if subLangs == nil {
		subLangs = fragmentValue(rawURL, "ytdlp_sub_langs")
	}
	outputIsFolder := downloadPack || splitChapters
	if outputIsFolder {
		if err := os.MkdirAll(destPath, 0755); err != nil {
			return 0, err
		}
	} else {
		dir := filepath.Dir(destPath)
		if dir != "." && dir != "" {
			_ = os.MkdirAll(dir, 0755)
		}
	}
	var urls []string
	if sel := selectedPlaylistURLs(selectedChildren); sel != nil {
		urls = *sel
	} else {
		urls = []string{cleanURL(rawURL)}
	}
	formatSel := ""
	if v := selectedFormat(selectedChildren); v != nil {
		formatSel = *v
	} else if v := fragmentValue(rawURL, "ytdlp_format"); v != nil {
		formatSel = *v
	} else {
		formatSel = "bestvideo+bestaudio/best"
	}
	format := resolveFormatSelector(formatSel)
	hasSplitMedia := strings.Contains(format, "+") || strings.Contains(format, "bestvideo")
	selectedMergeFormat := selectedValue(selectedChildren, "ytdlp_merge_format")
	if selectedMergeFormat == nil {
		if v := fragmentValue(rawURL, "ytdlp_merge_format"); v != nil {
			selectedMergeFormat = v
		}
	}
	var normalizedMerge *string
	if selectedMergeFormat != nil {
		normalizedMerge = normalizeMergeFormat(*selectedMergeFormat)
	}
	mergeFormat := ""
	if normalizedMerge != nil {
		mergeFormat = *normalizedMerge
	} else {
		if v := strings.TrimSpace(ctx.YoutubeMergeFormat); v != "" {
			if nv := normalizeMergeFormat(v); nv != nil {
				mergeFormat = *nv
			}
		}
	}
	if mergeFormat == "" {
		mergeFormat = "mp4"
	}
	writeThumbnail := selectedFlag(selectedChildren, "ytdlp_write_thumbnail")
	if !writeThumbnail {
		if v := fragmentValue(rawURL, "ytdlp_write_thumbnail"); v != nil {
			switch *v {
			case "1", "true", "yes", "on":
				writeThumbnail = true
			}
		}
	}
	writeSubsSelected := selectedFlag(selectedChildren, "ytdlp_write_subs")
	if !writeSubsSelected {
		if v := fragmentValue(rawURL, "ytdlp_write_subs"); v != nil {
			switch *v {
			case "1", "true", "yes", "on":
				writeSubsSelected = true
			}
		}
	}
	multiAudio := selectedFlag(selectedChildren, "ytdlp_multi_audio")
	if !multiAudio {
		if v := fragmentValue(rawURL, "ytdlp_multi_audio"); v != nil {
			switch *v {
			case "1", "true", "yes", "on":
				multiAudio = true
			}
		}
	}
	progressChildKey := format
	if selectedChildren != nil && len(*selectedChildren) > 0 {
		progressChildKey = (*selectedChildren)[0]
	}
	var grandTotal uint64
	var grandDownloaded uint64
	startedAt := time.Now()

	ffmpegBin := os.Getenv("GDOWNLOADER_FFMPEG_BIN")

	for _, itemURL := range urls {
		var args []string
		applyCookiesArgs(&args, ctx)
		applyProxyArgs(&args, ctx)
		args = append(args,
			"-f", format,
			"--no-playlist",
			"--merge-output-format", mergeFormat,
			"--newline",
			"--progress-template", "download:GDLPROG %(progress._percent_str)s %(progress._speed_str)s %(progress._eta_str)s",
			"--retries", "3",
			"--fragment-retries", "3",
			"-c",
			"-o", outputTemplate(destPath, outputIsFolder),
		)
		if speedLimitBps > 0 {
			args = append(args, "--limit-rate", fmt.Sprintf("%d", speedLimitBps))
		}
		if writeThumbnail || downloadPack {
			args = append(args, "--write-thumbnail")
		}
		if downloadPack {
			args = append(args, "--write-description", "--write-info-json", "--ignore-errors")
		}
		if multiAudio {
			args = append(args, "--audio-multistreams")
		}
		if downloadPack || ctx.YoutubeDownloadSubs || writeSubsSelected {
			args = append(args, "--write-subs", "--write-auto-sub", "--sub-lang")
			lang := ""
			if subLangs != nil {
				lang = *subLangs
			}
			if strings.TrimSpace(lang) == "" {
				lang = ctx.YoutubeSubLangs
			}
			if strings.TrimSpace(lang) == "" {
				lang = "pt,en"
			}
			args = append(args, lang)
			if ctx.YoutubeEmbedSubs {
				args = append(args, "--embed-subs")
			}
		}
		if splitChapters {
			args = append(args, "--split-chapters", "-o", fmt.Sprintf("chapter:%s", chapterOutputTemplate(destPath)))
		}
		if strings.TrimSpace(ffmpegBin) != "" {
			args = append(args, "--ffmpeg-location", ffmpegBin)
		}
		args = append(args, itemURL)

		cmd := exec.Command(ytdlpBin(), args...)
		stdout, err := cmd.StdoutPipe()
		if err != nil {
			return grandDownloaded, err
		}
		stderrPipe, _ := cmd.StderrPipe()
		if err := cmd.Start(); err != nil {
			return grandDownloaded, err
		}
		// capture stderr async
		stderrCh := make(chan string, 1)
		go func() {
			// read all stderr
			var sb strings.Builder
			if stderrPipe != nil {
				buf := make([]byte, 4096)
				for {
					n, err := stderrPipe.Read(buf)
					if n > 0 {
						sb.Write(buf[:n])
					}
					if err != nil {
						break
					}
				}
			}
			stderrCh <- sb.String()
		}()

		scanner := bufio.NewScanner(stdout)
		// allow long lines
		buf := make([]byte, 64*1024)
		scanner.Buffer(buf, 1024*1024)
		itemName := filepath.Base(itemURL)
		if itemName == "" {
			itemName = "YouTube"
		}
		var itemDownloaded uint64
		var itemTotal uint64
		var phaseCount uint32
		currentStage := ""
		var lastSyntheticDownloaded uint64

		sendStage := func(stage string, bytesDl, total uint64) {
			if progressCb != nil {
				cp := progressChildKey
				cf := stage
				cbd := bytesDl
				ctb := total
				progressCb(YouTubeProgressUpdate{
					BytesDownloaded:      bytesDl,
					TotalBytes:           total,
					ChildPath:            &cp,
					ChildFilename:        &cf,
					ChildBytesDownloaded: &cbd,
					ChildTotalBytes:      &ctb,
					ChildSpeedBps:        u64Ptr(0),
					ChildEtaSecs:         u64Ptr(0),
				})
			}
		}

		for scanner.Scan() {
			line := scanner.Text()
			trimmed := strings.TrimSpace(line)
			if strings.HasPrefix(trimmed, "[download] Destination:") {
				phaseCount++
				if phaseCount <= 1 {
					currentStage = "Baixando vídeo"
				} else {
					currentStage = "Baixando áudio"
				}
				val := phaseProgress(itemDownloaded, phaseCount, hasSplitMedia)
				if val > lastSyntheticDownloaded {
					lastSyntheticDownloaded = val
				}
				sendStage(currentStage, grandDownloaded+lastSyntheticDownloaded, maxU64(grandTotal, SyntheticProgressTotal))
			} else if strings.HasPrefix(trimmed, "[download] Resuming download at byte") && currentStage == "" {
				currentStage = "Baixando vídeo"
				sendStage(currentStage, grandDownloaded+lastSyntheticDownloaded, maxU64(grandTotal, SyntheticProgressTotal))
			} else if strings.HasPrefix(trimmed, "[Merger]") || strings.HasPrefix(trimmed, "[ffmpeg]") || strings.Contains(trimmed, "Merging formats") {
				currentStage = "Mesclando arquivos"
				if v := mergeProgress(hasSplitMedia); v > lastSyntheticDownloaded {
					lastSyntheticDownloaded = v
				}
				sendStage(currentStage, grandDownloaded+lastSyntheticDownloaded, maxU64(maxU64(grandTotal, itemTotal), SyntheticProgressTotal))
			}
			if upd := parseProgress(line); upd != nil {
				if upd.TotalBytes > 0 {
					itemTotal = upd.TotalBytes
					if grandDownloaded+itemTotal > grandTotal {
						grandTotal = grandDownloaded + itemTotal
					}
				}
				itemDownloaded = upd.BytesDownloaded
				synth := phaseProgress(upd.BytesDownloaded, phaseCount, hasSplitMedia)
				if synth > lastSyntheticDownloaded {
					lastSyntheticDownloaded = synth
				}
				if progressCb != nil {
					filename := currentStage
					if filename == "" {
						filename = itemName
					}
					cp := progressChildKey
					cbd := lastSyntheticDownloaded
					ctb := itemTotal
					speed := uint64(0)
					if upd.ChildSpeedBps != nil {
						speed = *upd.ChildSpeedBps
					}
					eta := uint64(0)
					if upd.ChildEtaSecs != nil {
						eta = *upd.ChildEtaSecs
					}
					progressCb(YouTubeProgressUpdate{
						BytesDownloaded:      grandDownloaded + lastSyntheticDownloaded,
						TotalBytes:           grandTotal,
						ChildPath:            &cp,
						ChildFilename:        &filename,
						ChildBytesDownloaded: &cbd,
						ChildTotalBytes:      &ctb,
						ChildSpeedBps:        &speed,
						ChildEtaSecs:         &eta,
					})
				}
			}
		}
		if err := scanner.Err(); err != nil {
			// ignore?
			_ = err
		}
		err = cmd.Wait()
		stderrStr := <-stderrCh
		if err != nil {
			msg := strings.TrimSpace(stderrStr)
			if msg == "" {
				msg = "yt-dlp falhou ao baixar o video"
			}
			return grandDownloaded, fmt.Errorf("%s", msg)
		}
		completedSynthetic := lastSyntheticDownloaded
		if v := phaseProgress(maxU64(itemDownloaded, itemTotal), phaseCount, hasSplitMedia); v > completedSynthetic {
			completedSynthetic = v
		}
		if SyntheticProgressTotal > completedSynthetic {
			completedSynthetic = SyntheticProgressTotal
		}
		grandDownloaded += completedSynthetic
		if grandDownloaded > grandTotal {
			grandTotal = grandDownloaded
		}
		finalSize := outputSize(destPath, startedAt)
		if downloadPack {
			if !hasVideoOutput(destPath) {
				return grandDownloaded, fmt.Errorf("yt-dlp concluiu o Pack sem produzir um arquivo de vídeo")
			}
		}
		if finalSize > 0 {
			grandDownloaded = finalSize
		}
		sendStage("Concluído", SyntheticProgressTotal, SyntheticProgressTotal)
	}
	return grandDownloaded, nil
}

func maxU64(a, b uint64) uint64 {
	if a > b {
		return a
	}
	return b
}

func outputSize(destPath string, startedAt time.Time) uint64 {
	info, err := os.Stat(destPath)
	if err == nil {
		if !info.IsDir() {
			return uint64(info.Size())
		}
		// is dir
		entries, err := os.ReadDir(destPath)
		if err == nil {
			var total uint64
			for _, e := range entries {
				if !e.IsDir() {
					if fi, err := e.Info(); err == nil {
						total += uint64(fi.Size())
					}
				}
			}
			return total
		}
		return 0
	}
	// fallback like Rust: look at parent dir for files starting with stem with newest modified >= startedAt
	dir := filepath.Dir(destPath)
	base := strings.TrimSuffix(filepath.Base(destPath), filepath.Ext(destPath))
	if base == "" {
		base = filepath.Base(destPath)
	}
	// For destPath without ext case, file_stem handling slightly different; use base without ext logic similar to Rust:
	// Rust uses path.file_stem()
	stem := base
	if ext := filepath.Ext(destPath); ext != "" {
		stem = strings.TrimSuffix(filepath.Base(destPath), ext)
	}
	if stem == "" {
		return 0
	}
	entries, err := os.ReadDir(dir)
	if err != nil {
		return 0
	}
	var newestSize uint64
	newestModified := startedAt
	for _, e := range entries {
		if e.IsDir() {
			continue
		}
		name := e.Name()
		if !strings.HasPrefix(name, stem) {
			continue
		}
		fi, err := e.Info()
		if err != nil {
			continue
		}
		mod := fi.ModTime()
		if !mod.Before(newestModified) {
			newestModified = mod
			newestSize = uint64(fi.Size())
		}
	}
	return newestSize
}

func hasVideoOutput(destPath string) bool {
	videoExts := map[string]bool{"mp4": true, "mkv": true, "webm": true, "mov": true, "avi": true, "m4v": true}
	checkIsVideo := func(p string) bool {
		ext := strings.ToLower(strings.TrimPrefix(filepath.Ext(p), "."))
		return videoExts[ext]
	}
	info, err := os.Stat(destPath)
	if err == nil && !info.IsDir() {
		return checkIsVideo(destPath)
	}
	entries, err := os.ReadDir(destPath)
	if err != nil {
		return false
	}
	for _, e := range entries {
		if e.IsDir() {
			continue
		}
		if checkIsVideo(e.Name()) {
			return true
		}
		// also check full path
		if checkIsVideo(filepath.Join(destPath, e.Name())) {
			return true
		}
	}
	return false
}

// helpers for pointers

func strPtr(s string) *string { return &s }

func statusPtr(s models.DownloadStatus) *models.DownloadStatus { return &s }

// Exposed helpers for testing — wrappers around private funcs to allow external test access via same package

func (p YouTubeProvider) CleanURL(raw string) string { return cleanURL(raw) }
func (p YouTubeProvider) FragmentValue(rawURL, key string) *string { return fragmentValue(rawURL, key) }
func (p YouTubeProvider) ResolveFormatSelector(format string) string { return resolveFormatSelector(format) }
func (p YouTubeProvider) NormalizeMergeFormat(value string) *string { return normalizeMergeFormat(value) }
func (p YouTubeProvider) SelectedFormat(children *[]string) *string { return selectedFormat(children) }
func (p YouTubeProvider) SelectedValue(children *[]string, key string) *string { return selectedValue(children, key) }
func (p YouTubeProvider) SelectedFlag(children *[]string, key string) bool { return selectedFlag(children, key) }
func (p YouTubeProvider) SelectedPlaylistURLs(children *[]string) *[]string { return selectedPlaylistURLs(children) }
func (p YouTubeProvider) OutputTemplate(dest string, isFolder bool) string { return outputTemplate(dest, isFolder) }
func (p YouTubeProvider) ChapterOutputTemplate(dest string) string { return chapterOutputTemplate(dest) }
func (p YouTubeProvider) IsChannelURL(raw string) bool { return isChannelURL(raw) }
func (p YouTubeProvider) ChannelLimit(raw string) *uint32 { return channelLimit(raw) }
func (p YouTubeProvider) ParseSpeedBps(v string) uint64 { return parseSpeedBps(v) }
func (p YouTubeProvider) ParseEtaSecs(v string) uint64 { return parseEtaSecs(v) }
func (p YouTubeProvider) ParseProgress(line string) *YouTubeProgressUpdate { return parseProgress(line) }
func (p YouTubeProvider) PhaseBounds(c uint32, hasSplit bool) (uint64, uint64) { return phaseBounds(c, hasSplit) }
func (p YouTubeProvider) PhaseProgress(d uint64, c uint32, hasSplit bool) uint64 { return phaseProgress(d, c, hasSplit) }
func (p YouTubeProvider) MergeProgress(hasSplit bool) uint64 { return mergeProgress(hasSplit) }
