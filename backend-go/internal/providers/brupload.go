package providers

import (
	"fmt"
	"io"
	"net/http"
	"net/url"
	"regexp"
	"strings"
)

// Portado de backend/src/providers/brupload.rs

type BruploadProvider struct{}

type BruploadFolderEntry struct {
	Filename  string `json:"filename"`
	Path      string `json:"path"`
	Size      uint64 `json:"size"`
	SourceURL string `json:"sourceUrl"`
}

func (p BruploadProvider) Matches(rawURL string) bool {
	if !HostMatches(rawURL, []string{"brupload.net", "www.brupload.net"}) {
		return false
	}
	return p.FileCode(rawURL) != nil || p.IsFolderURL(rawURL)
}

func (p BruploadProvider) IsFolderURL(rawURL string) bool {
	segs := PathSegments(rawURL)
	if len(segs) < 3 {
		return false
	}
	return segs[0] == "users"
}

func (p BruploadProvider) ExtractFolderEntries(html string) []BruploadFolderEntry {
	re := regexp.MustCompile(`(?is)href=["'](?:https?://(?:www\.)?brupload\.net)?/(?:d/)?([A-Za-z0-9]{8,})["']`)
	if re == nil {
		return nil
	}
	seen := map[string]bool{}
	var entries []BruploadFolderEntry
	for _, m := range re.FindAllStringSubmatch(html, -1) {
		code := strings.TrimSpace(m[1])
		sourceURL := fmt.Sprintf("https://brupload.net/%s", code)
		if seen[sourceURL] {
			continue
		}
		seen[sourceURL] = true
		entries = append(entries, BruploadFolderEntry{
			Path:      code,
			Filename:  code,
			Size:      0,
			SourceURL: sourceURL,
		})
	}
	return entries
}

func (p BruploadProvider) ExtractFolderName(html, rawURL string) string {
	re := regexp.MustCompile(`(?is)<title>\s*([^<]+?)\s*</title>`)
	if m := re.FindStringSubmatch(html); len(m) > 1 {
		title := bruploadDecodeHTML(m[1])
		if title != "" && !strings.Contains(strings.ToLower(title), "brupload") {
			return title
		}
	}
	segs := PathSegments(rawURL)
	if len(segs) > 0 {
		return segs[len(segs)-1]
	}
	return "pasta_brupload"
}

func (p BruploadProvider) FileCode(rawURL string) *string {
	u := ParseURL(rawURL)
	if u == nil {
		return nil
	}
	parts := strings.Split(strings.Trim(u.Path, "/"), "/")
	var clean []string
	for _, p := range parts {
		if p != "" {
			clean = append(clean, p)
		}
	}
	if len(clean) == 1 && bruploadLooksLikeCode(clean[0]) {
		return &clean[0]
	}
	if len(clean) == 2 && clean[0] == "d" && bruploadLooksLikeCode(clean[1]) {
		return &clean[1]
	}
	return nil
}

func bruploadLooksLikeCode(value string) bool {
	if len(value) < 8 {
		return false
	}
	for _, ch := range value {
		if !((ch >= 'a' && ch <= 'z') || (ch >= 'A' && ch <= 'Z') || (ch >= '0' && ch <= '9')) {
			return false
		}
	}
	return true
}

func bruploadDecodeHTML(value string) string {
	replacer := strings.NewReplacer("&gt;", ">", "&lt;", "<", "&amp;", "&", "&quot;", "\"", "&#039;", "'", "&#39;", "'", "&#133;", "...")
	return strings.TrimSpace(replacer.Replace(value))
}

func bruploadParseHiddenInputs(html string) [][2]string {
	re := regexp.MustCompile(`(?is)<input[^>]*name=["']([^"']+)["'][^>]*value=["']([^"']*)["'][^>]*>`)
	if re == nil {
		return nil
	}
	var out [][2]string
	for _, m := range re.FindAllStringSubmatch(html, -1) {
		out = append(out, [2]string{strings.TrimSpace(m[1]), bruploadDecodeHTML(strings.TrimSpace(m[2]))})
	}
	return out
}

func bruploadHiddenInput(html, name string) *string {
	for _, kv := range bruploadParseHiddenInputs(html) {
		if kv[0] == name {
			v := kv[1]
			return &v
		}
	}
	return nil
}

func (p BruploadProvider) ExtractFilename(html string) *string {
	if v := bruploadHiddenInput(html, "fname"); v != nil && strings.TrimSpace(*v) != "" {
		return v
	}
	re := regexp.MustCompile(`(?is)<title>\s*Download\s+([^<]+?)\s*</title>`)
	if m := re.FindStringSubmatch(html); len(m) > 1 {
		title := bruploadDecodeHTML(m[1])
		if title != "" {
			return &title
		}
	}
	return nil
}

func (p BruploadProvider) ExtractSize(html string) uint64 {
	patterns := []string{
		`(?is)<span class="statd">\s*(?:tamanho|size)\s*</span>\s*<span>\s*([^<]+)\s*</span>`,
		`(?is)(?:tamanho|size)\s*:\s*</[^>]+>\s*<[^>]+>\s*([^<]+)\s*<`,
		`(?is)(?:tamanho|size)\s*:\s*([0-9][0-9.,]*\s*[KMGT]?B)`,
	}
	for _, pat := range patterns {
		re := regexp.MustCompile(pat)
		if m := re.FindStringSubmatch(html); len(m) > 1 {
			size := ParseHumanSize(bruploadDecodeHTML(m[1]))
			if size > 0 {
				return size
			}
		}
	}
	return 0
}

func (p BruploadProvider) ExtractError(html string) *string {
	re := regexp.MustCompile(`(?is)<div class="err">\s*(.*?)\s*</div>`)
	m := re.FindStringSubmatch(html)
	if len(m) < 2 {
		return nil
	}
	raw := m[1]
	brRe := regexp.MustCompile(`(?is)<br\s*/?>`)
	cleaned := brRe.ReplaceAllString(raw, " ")
	tagRe := regexp.MustCompile(`(?is)<[^>]+>`)
	text := tagRe.ReplaceAllString(cleaned, "")
	msg := bruploadDecodeHTML(text)
	if msg == "" {
		return nil
	}
	return &msg
}

func (p BruploadProvider) ExtractWaitSeconds(html string) *uint64 {
	patterns := []string{`class="seconds">\s*(\d+)\s*<`, `var\s+estimated_time\s*=\s*(\d+)`, `var\s+seconds\s*=\s*(\d+)`}
	for _, pat := range patterns {
		re := regexp.MustCompile(pat)
		if m := re.FindStringSubmatch(html); len(m) > 1 {
			var v uint64
			fmt.Sscanf(m[1], "%d", &v)
			return &v
		}
	}
	return nil
}

func (p BruploadProvider) DetectRecaptchaSitekey(html string) *string {
	re := regexp.MustCompile(`g-recaptcha[^>]+data-sitekey=["']([^"']+)["']`)
	if m := re.FindStringSubmatch(html); len(m) > 1 {
		return &m[1]
	}
	return nil
}

func (p BruploadProvider) DetectHCaptchaSitekey(html string) *string {
	re := regexp.MustCompile(`h-captcha[^>]+data-sitekey=["']([^"']+)["']`)
	if m := re.FindStringSubmatch(html); len(m) > 1 {
		return &m[1]
	}
	return nil
}

func (p BruploadProvider) ExtractCaptchaToken(rawURL string) *string {
	u := ParseURL(rawURL)
	if u == nil {
		return nil
	}
	frag := u.Fragment
	// also handle raw fragment without ParseURL fragment (since ParseURL strips #)
	if frag == "" {
		if idx := strings.Index(rawURL, "#"); idx >= 0 {
			frag = rawURL[idx+1:]
		}
	}
	for _, part := range strings.Split(frag, "&") {
		if strings.HasPrefix(part, "captcha_token=") {
			v := part[len("captcha_token="):]
			return &v
		}
	}
	return nil
}

func (p BruploadProvider) ExtractDirectDownloadURL(html string) *string {
	patterns := []string{
		`(?is)href=["'](https?://[^"']+)["'][^>]*class=["'][^"']*downloadbtn`,
		`(?is)href=["'](https?://[^"']+)["'][^>]*id=["']downloadbtn`,
		`(?is)window\.location\s*=\s*["'](https?://[^"']+)["']`,
		`(?is)document\.location\s*=\s*["'](https?://[^"']+)["']`,
	}
	for _, pat := range patterns {
		re := regexp.MustCompile(pat)
		if m := re.FindStringSubmatch(html); len(m) > 1 {
			return &m[1]
		}
	}
	return nil
}

func (p BruploadProvider) IsBinaryDownloadResponse(resp *http.Response) bool {
	if resp.Header.Get("Content-Disposition") != "" {
		return true
	}
	ct := strings.ToLower(resp.Header.Get("Content-Type"))
	if ct == "" {
		return false
	}
	return !strings.HasPrefix(ct, "text/html") && !strings.HasPrefix(ct, "text/plain") && !strings.HasPrefix(ct, "application/json")
}

func (p BruploadProvider) BuildDownload1Payload(html, code, referer string) map[string]string {
	payload := map[string]string{
		"op":  "download1",
		"id":  code,
		"referer": referer,
	}
	if v := bruploadHiddenInput(html, "usr_login"); v != nil && *v != "" {
		payload["usr_login"] = *v
	}
	if v := bruploadHiddenInput(html, "fname"); v != nil && *v != "" {
		payload["fname"] = *v
	} else {
		payload["fname"] = "arquivo_brupload"
	}
	if v := bruploadHiddenInput(html, "method_free"); v != nil && *v != "" {
		payload["method_free"] = *v
	} else {
		payload["method_free"] = "Download Gratuito >>"
	}
	return payload
}

func (p BruploadProvider) BuildDownload2Payload(html, referer string, captchaToken *string) map[string]string {
	payload := map[string]string{}
	for _, kv := range bruploadParseHiddenInputs(html) {
		switch kv[0] {
		case "op", "id", "rand", "referer", "method_free", "method_premium", "usr_login", "fname":
			payload[kv[0]] = kv[1]
		}
	}
	if _, ok := payload["referer"]; !ok {
		payload["referer"] = referer
	}
	if _, ok := payload["adblock_detected"]; !ok {
		payload["adblock_detected"] = "0"
	}
	if captchaToken != nil {
		payload["g-recaptcha-response"] = *captchaToken
		payload["h-captcha-response"] = *captchaToken
	}
	return payload
}

func (p BruploadProvider) FetchFileInfo(client *http.Client, rawURL string) (string, uint64, bool, []BruploadFolderEntry, error) {
	if p.IsFolderURL(rawURL) {
		resp, err := client.Get(rawURL)
		if err != nil {
			return "", 0, false, nil, err
		}
		defer resp.Body.Close()
		body, _ := io.ReadAll(io.LimitReader(resp.Body, 2*1024*1024))
		html := string(body)
		folderName := p.ExtractFolderName(html, rawURL)
		entries := p.ExtractFolderEntries(html)
		return SanitizeFilename(folderName, "pasta_brupload"), 0, true, entries, nil
	}
	code := p.FileCode(rawURL)
	if code == nil {
		return "", 0, false, nil, fmt.Errorf("URL do BRUpload inválida")
	}
	resp, err := client.Get(rawURL)
	if err != nil {
		return "", 0, false, nil, err
	}
	defer resp.Body.Close()
	body, _ := io.ReadAll(io.LimitReader(resp.Body, 512*1024))
	html := string(body)
	filename := "arquivo_brupload"
	if v := p.ExtractFilename(html); v != nil {
		filename = *v
	}
	size := p.ExtractSize(html)
	// fallback via download1 if size==0
	if size == 0 {
		payload := p.BuildDownload1Payload(html, *code, rawURL)
		form := encodeForm(payload)
		req, _ := http.NewRequest("POST", rawURL, strings.NewReader(form))
		req.Header.Set("Content-Type", "application/x-www-form-urlencoded")
		req.Header.Set("Referer", rawURL)
		if resp2, err := client.Do(req); err == nil {
			defer resp2.Body.Close()
			b2, _ := io.ReadAll(io.LimitReader(resp2.Body, 512*1024))
			html2 := string(b2)
			if filename == "arquivo_brupload" {
				if v := p.ExtractFilename(html2); v != nil {
					filename = *v
				}
			}
			if s := p.ExtractSize(html2); s > size {
				size = s
			}
		}
	}
	return SanitizeFilename(filename, "arquivo_brupload"), size, false, nil, nil
}

// helpers

func encodeForm(m map[string]string) string {
	vals := url.Values{}
	for k, v := range m {
		vals.Set(k, v)
	}
	return vals.Encode()
}
