package providers

import (
	"fmt"
	"io"
	"net/http"
	"regexp"
	"strings"
)

// Portado de backend/src/providers/brfiles.rs

type BrfilesProvider struct{}

type BrfilesFolderEntry struct {
	Filename  string `json:"filename"`
	Path      string `json:"path"`
	Size      uint64 `json:"size"`
	SourceURL string `json:"sourceUrl"`
}

func (p BrfilesProvider) Matches(rawURL string) bool {
	if !HostMatches(rawURL, []string{"brfiles.com", "www.brfiles.com"}) {
		return false
	}
	segs := PathSegments(rawURL)
	if len(segs) < 2 {
		return false
	}
	return segs[0] == "f" || segs[0] == "d"
}

func (p BrfilesProvider) IsFolderURL(rawURL string) bool {
	segs := PathSegments(rawURL)
	if len(segs) < 1 {
		return false
	}
	return segs[0] == "d"
}

func brfilesDecodeHTML(value string) string {
	replacer := strings.NewReplacer("&gt;", ">", "&lt;", "<", "&amp;", "&", "&quot;", "\"", "&#039;", "'", "&#133;", "...")
	return strings.TrimSpace(replacer.Replace(value))
}

func brfilesStripHTML(value string) string {
	brRe := regexp.MustCompile(`(?is)<br\s*/?>`)
	withoutBreaks := brRe.ReplaceAllString(value, " ")
	tagRe := regexp.MustCompile(`(?is)<[^>]+>`)
	withoutTags := tagRe.ReplaceAllString(withoutBreaks, " ")
	return brfilesDecodeHTML(strings.ReplaceAll(withoutTags, "\n", " "))
}

func brfilesCollectCookieHeader(headers http.Header) *string {
	var pairs []string
	for _, v := range headers.Values("Set-Cookie") {
		if first := strings.Split(v, ";")[0]; strings.TrimSpace(first) != "" {
			pairs = append(pairs, strings.TrimSpace(first))
		}
	}
	if len(pairs) == 0 {
		return nil
	}
	joined := strings.Join(pairs, "; ")
	return &joined
}

func brfilesMergeCookieHeaders(left, right *string) *string {
	cookies := map[string]string{}
	for _, src := range []*string{left, right} {
		if src == nil {
			continue
		}
		for _, pair := range strings.Split(*src, ";") {
			trimmed := strings.TrimSpace(pair)
			if trimmed == "" {
				continue
			}
			parts := strings.SplitN(trimmed, "=", 2)
			if len(parts) != 2 {
				continue
			}
			cookies[strings.TrimSpace(parts[0])] = strings.TrimSpace(parts[1])
		}
	}
	if len(cookies) == 0 {
		return nil
	}
	var parts []string
	for k, v := range cookies {
		parts = append(parts, fmt.Sprintf("%s=%s", k, v))
	}
	joined := strings.Join(parts, "; ")
	return &joined
}

func (p BrfilesProvider) ExtractFilename(html string) *string {
	re := regexp.MustCompile(`(?is)<title>\s*([^<]+?)\s*-\s*BRFiles\s*</title>`)
	if m := re.FindStringSubmatch(html); len(m) > 1 {
		clean := strings.TrimSpace(brfilesDecodeHTML(m[1]))
		if clean != "" {
			return &clean
		}
	}
	return nil
}

func (p BrfilesProvider) ExtractSize(html string) uint64 {
	re := regexp.MustCompile(`(?is)<p[^>]*class=["'][^"']*tamanho-arquivo[^"']*["'][^>]*>\s*([^<]+)\s*</p>`)
	if m := re.FindStringSubmatch(html); len(m) > 1 {
		return ParseHumanSize(brfilesDecodeHTML(m[1]))
	}
	return 0
}

func (p BrfilesProvider) ExtractWaitSeconds(html string) *uint64 {
	re := regexp.MustCompile(`var\s+seconds\s*=\s*(\d+)`)
	if m := re.FindStringSubmatch(html); len(m) > 1 {
		var v uint64
		fmt.Sscanf(m[1], "%d", &v)
		return &v
	}
	return nil
}

func (p BrfilesProvider) ExtractPtURL(html string) *string {
	re := regexp.MustCompile(`href=['"](https?://(?:www\.)?brfiles\.com[^'"]+\?pt=[^'"]+)['"]`)
	if m := re.FindStringSubmatch(html); len(m) > 1 {
		return &m[1]
	}
	return nil
}

func (p BrfilesProvider) ExtractFreeButtonURL(html string) *string {
	re := regexp.MustCompile(`(?is)<a[^>]*class=['"][^'"]*btn-free[^'"]*['"][^>]*href=['"]([^'"]+)['"]`)
	if m := re.FindStringSubmatch(html); len(m) > 1 {
		return &m[1]
	}
	return nil
}

func (p BrfilesProvider) ExtractFolderName(html string) *string {
	re := regexp.MustCompile(`(?is)<h2[^>]*>\s*Arquivos contidos na pasta\s*([^<]+)\s*</h2>`)
	if m := re.FindStringSubmatch(html); len(m) > 1 {
		clean := strings.TrimSpace(brfilesDecodeHTML(m[1]))
		if clean != "" {
			return &clean
		}
	}
	return nil
}

func (p BrfilesProvider) ExtractFolderEntries(html string) []BrfilesFolderEntry {
	re := regexp.MustCompile(`(?is)<a[^>]*class=["'][^"']*responsiveInfoTable[^"']*["'][^>]*href=["'](https?://(?:www\.)?brfiles\.com/f/[^"']+)["'][^>]*>\s*([^<]+)\s*</a>`)
	if re == nil {
		return nil
	}
	var out []BrfilesFolderEntry
	for _, m := range re.FindAllStringSubmatch(html, -1) {
		sourceURL := strings.TrimSpace(m[1])
		filename := SanitizeFilename(brfilesDecodeHTML(m[2]), "arquivo_brfiles")
		if sourceURL == "" || filename == "" {
			continue
		}
		out = append(out, BrfilesFolderEntry{
			Path:      filename,
			Filename:  filename,
			Size:      0,
			SourceURL: sourceURL,
		})
	}
	return out
}

func (p BrfilesProvider) IsBinaryDownloadResponse(resp *http.Response) bool {
	if resp.Header.Get("Content-Disposition") != "" {
		return true
	}
	ct := strings.ToLower(resp.Header.Get("Content-Type"))
	if ct == "" {
		return false
	}
	return !strings.HasPrefix(ct, "text/html") && !strings.HasPrefix(ct, "text/plain") && !strings.HasPrefix(ct, "application/json")
}

func (p BrfilesProvider) ExtractRateLimitSeconds(html string) *uint64 {
	text := strings.ToLower(brfilesStripHTML(html))
	relevant := strings.Contains(text, "aguarde") || strings.Contains(text, "limite") || strings.Contains(text, "download simult") || strings.Contains(text, "outro download") || strings.Contains(text, "ip")
	if !relevant {
		return nil
	}
	var total uint64
	matched := false
	for _, m := range regexp.MustCompile(`(\d+)\s*hora`).FindAllStringSubmatch(text, -1) {
		var v uint64
		fmt.Sscanf(m[1], "%d", &v)
		total += v * 3600
		matched = true
	}
	for _, m := range regexp.MustCompile(`(\d+)\s*minuto`).FindAllStringSubmatch(text, -1) {
		var v uint64
		fmt.Sscanf(m[1], "%d", &v)
		total += v * 60
		matched = true
	}
	for _, m := range regexp.MustCompile(`(\d+)\s*segundo`).FindAllStringSubmatch(text, -1) {
		var v uint64
		fmt.Sscanf(m[1], "%d", &v)
		total += v
		matched = true
	}
	if matched && total > 0 {
		return &total
	}
	return nil
}

func (p BrfilesProvider) ExtractRateLimitMessage(html string) *string {
	text := brfilesStripHTML(html)
	lower := strings.ToLower(text)
	if strings.Contains(lower, "limite") || strings.Contains(lower, "aguarde") || strings.Contains(lower, "download simult") || strings.Contains(lower, "outro download") || strings.Contains(lower, "ip") {
		return &text
	}
	return nil
}

func (p BrfilesProvider) FetchFileInfo(client *http.Client, rawURL string) (string, uint64, bool, []BrfilesFolderEntry, error) {
	resp, err := client.Get(rawURL)
	if err != nil {
		return "", 0, false, nil, err
	}
	defer resp.Body.Close()
	body, _ := io.ReadAll(io.LimitReader(resp.Body, 512*1024))
	html := string(body)
	if p.IsFolderURL(rawURL) {
		var folderName string
		if n := p.ExtractFolderName(html); n != nil {
			folderName = *n
		} else {
			folderName = "pasta_brfiles"
		}
		entries := p.ExtractFolderEntries(html)
		// enrich sizes async would require parallel fetch; keep zero for quick probe
		return SanitizeFilename(folderName, "pasta_brfiles"), 0, true, entries, nil
	}
	filename := "arquivo_brfiles"
	if v := p.ExtractFilename(html); v != nil {
		filename = *v
	}
	size := p.ExtractSize(html)
	return SanitizeFilename(filename, "arquivo_brfiles"), size, false, nil, nil
}
