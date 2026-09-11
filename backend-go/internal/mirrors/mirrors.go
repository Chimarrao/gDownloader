package mirrors

import (
	"regexp"
	"strings"
)

// Portado de backend/src/mirrors/mod.rs

var Hosters = []string{
	"1fichier.com",
	"rapidgator.net",
	"mega.nz",
	"mediafire.com",
	"pixeldrain.com",
	"gofile.io",
	"katfile.com",
	"katfile.ws",
	"brfiles.com",
	"terabox.com",
	"1024tera.com",
	"akirabox.to",
	"1cloudfile.com",
	"ddownload.com",
	"drop.download",
	"filefactory.com",
	"fikper.com",
	"hitfile.net",
	"nitroflare.com",
	"turbobit.net",
	"uploadgig.com",
	"clicknupload.to",
	"dailyuploads.net",
	"dropgalaxy.com",
	"file-upload.org",
	"send.cm",
	"userupload.net",
	"usersdrive.com",
	"drive.google.com",
	"onedrive.live.com",
	"sharepoint.com",
	"dropbox.com",
	"box.com",
	"archive.org",
	"wetransfer.com",
	"smash.com",
	"transfernow.net",
	"sendspace.com",
	"filemail.com",
	"multiup.io",
	"multiup.org",
	"workupload.com",
}

var (
	partSuffixRe  = regexp.MustCompile(`(?i)\.part\.?\d+$`)
	fileExtRe     = regexp.MustCompile(`(?i)^(.+)\.(mkv|avi|mp4|mov|wmv|flv|zip|rar|7z|iso|exe|pdf|epub|ts|m2ts)$`)
	nonAlnumRe    = regexp.MustCompile(`[^a-zA-Z0-9]+`)
	uploaderTagRe = regexp.MustCompile(`^[A-Za-z][A-Za-z0-9]{2,11}$`)
)

type MirrorResult struct {
	Source string  `json:"source"`
	URL    string  `json:"url"`
	Label  string  `json:"label"`
	Hoster *string `json:"hoster,omitempty"`
	Score  int     `json:"score"`
}

// NormalizeFilename portado de mod.rs:81
func NormalizeFilename(raw string) (string, string) {
	name := strings.TrimSpace(raw)
	name = partSuffixRe.ReplaceAllString(name, "")
	var base, ext string
	if m := fileExtRe.FindStringSubmatch(name); len(m) > 2 {
		base = m[1]
		ext = strings.ToLower(m[2])
	} else {
		base = name
		ext = ""
	}
	slug := strings.TrimSpace(nonAlnumRe.ReplaceAllString(base, " "))
	return slug, ext
}

func DetectHoster(rawURL string) *string {
	lower := strings.ToLower(rawURL)
	for _, h := range Hosters {
		if strings.Contains(lower, h) {
			return &h
		}
	}
	return nil
}

func ExtractUploaderTag(slug string) *string {
	noise := map[string]bool{
		"mkv": true, "avi": true, "mp4": true, "bluray": true, "bdrip": true, "brrip": true, "webrip": true, "web": true, "dl": true,
		"1080p": true, "720p": true, "4k": true, "uhd": true, "dts": true, "hd": true, "5": true, "1": true, "2": true, "ac3": true, "aac": true,
		"x264": true, "x265": true, "hevc": true, "remux": true, "dual": true, "multi": true, "pt": true, "br": true, "eng": true,
		"subs": true, "sub": true, "dub": true, "dubbed": true, "extended": true, "directors": true, "cut": true,
	}
	words := strings.Fields(slug)
	for i := len(words) - 1; i >= 0; i-- {
		word := words[i]
		lower := strings.ToLower(word)
		if noise[lower] {
			continue
		}
		if uploaderTagRe.MatchString(word) {
			return &word
		}
	}
	return nil
}

func haystackForResult(r *MirrorResult) string {
	return strings.ToLower(r.URL + " " + r.Label + " " + r.Source)
}

func CoreTerms(slug string, uploader *string) []string {
	uploaderLower := ""
	if uploader != nil {
		uploaderLower = strings.ToLower(*uploader)
	}
	var terms []string
	seen := map[string]bool{}
	for _, word := range strings.Fields(slug) {
		lower := strings.ToLower(word)
		if len(word) < 3 {
			continue
		}
		if _, err := parseUint(word); err == nil {
			continue
		}
		if lower == uploaderLower {
			continue
		}
		if !seen[lower] {
			seen[lower] = true
			terms = append(terms, lower)
		}
		if len(terms) >= 6 {
			break
		}
	}
	return terms
}

func parseUint(s string) (uint64, error) {
	var v uint64
	for _, ch := range s {
		if ch < '0' || ch > '9' {
			return 0, &parseErr{}
		}
		v = v*10 + uint64(ch-'0')
	}
	return v, nil
}

type parseErr struct{}

func (e *parseErr) Error() string { return "not uint" }

func TitleHitCount(result *MirrorResult, slug string, uploader *string) int {
	haystack := haystackForResult(result)
	count := 0
	for _, term := range CoreTerms(slug, uploader) {
		if strings.Contains(haystack, term) {
			count++
		}
	}
	return count
}

func UploaderHit(result *MirrorResult, uploader *string) bool {
	if uploader == nil {
		return false
	}
	return strings.Contains(haystackForResult(result), strings.ToLower(*uploader))
}

func IsRelevantResult(result *MirrorResult, slug string, uploader *string) bool {
	haystack := haystackForResult(result)
	titleHits := TitleHitCount(result, slug, uploader)
	hasHoster := result.Hoster != nil
	hasUploader := UploaderHit(result, uploader)
	exactSlug := strings.Contains(haystack, strings.ToLower(slug))

	if exactSlug {
		return true
	}
	if uploader != nil {
		return (hasUploader && titleHits >= 1) || titleHits >= 3 || (hasHoster && titleHits >= 2)
	}
	return titleHits >= 2 || (hasHoster && titleHits >= 1)
}

func ScoreResult(result *MirrorResult, slug string, uploader *string) int {
	haystack := haystackForResult(result)
	titleHits := TitleHitCount(result, slug, uploader)
	s := 0
	if strings.Contains(haystack, strings.ToLower(slug)) {
		s += 18
	}
	if result.Hoster != nil {
		s += 10
	}
	s += titleHits * 4
	if UploaderHit(result, uploader) {
		s += 12
	}
	if strings.Contains(haystack, ".mkv") || strings.Contains(haystack, ".mp4") || strings.Contains(haystack, ".zip") || strings.Contains(haystack, ".rar") {
		s += 3
	}
	if titleHits == 0 {
		s -= 8
	} else if titleHits == 1 {
		s -= 2
	}
	return s
}

func MakeResult(source, rawURL, label string) MirrorResult {
	hoster := DetectHoster(rawURL)
	if hoster == nil {
		// fallback to hostname
		if u := parseURLHost(rawURL); u != nil {
			h := *u
			hoster = &h
		}
	}
	normalizedSource := strings.ReplaceAll(strings.TrimSpace(source), "  ", " ")
	normalizedLabel := strings.TrimSpace(label)
	if normalizedLabel == "" {
		if hoster != nil {
			normalizedLabel = *hoster
		} else {
			normalizedLabel = normalizedSource
		}
	}
	return MirrorResult{
		Source: normalizedSource,
		URL:    rawURL,
		Label:  normalizedLabel,
		Hoster: hoster,
		Score:  0,
	}
}

func parseURLHost(raw string) *string {
	// minimal host extraction without net/url to avoid import cycle
	lower := raw
	if idx := strings.Index(lower, "://"); idx >= 0 {
		rest := lower[idx+3:]
		end := strings.IndexAny(rest, "/?#")
		var host string
		if end >= 0 {
			host = rest[:end]
		} else {
			host = rest
		}
		// strip port
		if colon := strings.Index(host, ":"); colon >= 0 {
			host = host[:colon]
		}
		if host != "" {
			return &host
		}
	}
	return nil
}
