package providers

import (
	"fmt"
	"io"
	"net/http"
	"regexp"
	"strings"
)

// Portado de backend/src/providers/moondl.rs

type MoonDLProvider struct{}

func (p MoonDLProvider) Matches(rawURL string) bool {
	if !HostMatches(rawURL, []string{"moondl.com", "www.moondl.com"}) {
		return false
	}
	segs := PathSegments(rawURL)
	if len(segs) == 0 {
		return false
	}
	return moondlLooksLikeCode(segs[0])
}

func moondlLooksLikeCode(value string) bool {
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

func (p MoonDLProvider) FileCode(rawURL string) *string {
	segs := PathSegments(rawURL)
	if len(segs) > 0 && moondlLooksLikeCode(segs[0]) {
		v := segs[0]
		return &v
	}
	return nil
}

func moondlDecodeHTML(value string) string {
	replacer := strings.NewReplacer("&gt;", ">", "&lt;", "<", "&amp;", "&", "&quot;", "\"", "&#039;", "'", "&#133;", "...")
	return strings.TrimSpace(replacer.Replace(value))
}

func moondlParseHiddenInputs(html string) [][2]string {
	re := regexp.MustCompile(`(?is)<input[^>]*name=["']([^"']+)["'][^>]*value=["']([^"']*)["'][^>]*>`)
	if re == nil {
		return nil
	}
	var out [][2]string
	for _, m := range re.FindAllStringSubmatch(html, -1) {
		out = append(out, [2]string{strings.TrimSpace(m[1]), moondlDecodeHTML(strings.TrimSpace(m[2]))})
	}
	return out
}

func moondlHiddenInput(html, name string) *string {
	for _, kv := range moondlParseHiddenInputs(html) {
		if kv[0] == name {
			v := kv[1]
			return &v
		}
	}
	return nil
}

func (p MoonDLProvider) ExtractFilename(html string) *string {
	if v := moondlHiddenInput(html, "fname"); v != nil && strings.TrimSpace(*v) != "" {
		return v
	}
	re := regexp.MustCompile(`(?is)<title>\s*Download\s+(.+?)\s*-\s*MoonDL\s*</title>`)
	if m := re.FindStringSubmatch(html); len(m) > 1 {
		title := moondlDecodeHTML(m[1])
		if title != "" {
			return &title
		}
	}
	return nil
}

func (p MoonDLProvider) ExtractSize(html string) uint64 {
	patterns := []string{
		`(?is)File size[^0-9]{0,40}([0-9]+(?:[.,][0-9]+)?\s*(?:KB|MB|GB|TB))`,
		`(?is)\(([0-9]+(?:[.,][0-9]+)?\s*(?:KB|MB|GB|TB))\)`,
	}
	for _, pat := range patterns {
		re := regexp.MustCompile(pat)
		if m := re.FindStringSubmatch(html); len(m) > 1 {
			size := ParseHumanSize(moondlDecodeHTML(m[1]))
			if size > 0 {
				return size
			}
		}
	}
	return 0
}

func (p MoonDLProvider) ExtractError(html string) *string {
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
	msg := moondlDecodeHTML(text)
	if msg == "" {
		return nil
	}
	return &msg
}

func (p MoonDLProvider) IsPremiumRequiredMessage(msg string) bool {
	lower := strings.ToLower(msg)
	return strings.Contains(lower, "upgrade your account") || strings.Contains(lower, "premium") || strings.Contains(lower, "1000 mb only") || strings.Contains(lower, "download files up to 1000 mb only")
}

func (p MoonDLProvider) ExtractWaitSeconds(html string) *uint64 {
	patterns := []string{
		`class=["']seconds["'][^>]*>\s*(\d+)\s*<`,
		`var\s+countdown\s*=\s*(\d+)`,
		`var\s+seconds\s*=\s*(\d+)`,
		`estimated_time\s*=\s*(\d+)`,
	}
	for _, pat := range patterns {
		re := regexp.MustCompile(pat)
		if m := re.FindStringSubmatch(html); len(m) > 1 {
			var v uint64
			fmt.Sscanf(m[1], "%d", &v)
			return &v
		}
	}
	if strings.Contains(html, "countdown('#countdown .seconds')") {
		v := uint64(60)
		return &v
	}
	return nil
}

func (p MoonDLProvider) DetectRecaptchaSitekey(html string) *string {
	re := regexp.MustCompile(`g-recaptcha[^>]+data-sitekey=["']([^"']+)["']`)
	if m := re.FindStringSubmatch(html); len(m) > 1 {
		return &m[1]
	}
	return nil
}

func (p MoonDLProvider) ExtractDirectDownloadURL(html string) *string {
	patterns := []string{
		`(?is)href=["'](https?://[^"']+)["'][^>]*id=["']downloadbtn`,
		`(?is)href=["'](https?://[^"']+)["'][^>]*class=["'][^"']*downloadbtn`,
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

func (p MoonDLProvider) ExtractCaptchaToken(rawURL string) *string {
	u := ParseURL(rawURL)
	frag := ""
	if u != nil {
		frag = u.Fragment
	}
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

func (p MoonDLProvider) BuildDownload1Payload(html, code, referer string) map[string]string {
	payload := map[string]string{
		"op":          "download1",
		"id":          code,
		"referer":     referer,
		"method_free": "1",
	}
	if v := moondlHiddenInput(html, "usr_login"); v != nil && *v != "" {
		payload["usr_login"] = *v
	}
	if v := moondlHiddenInput(html, "fname"); v != nil && *v != "" {
		payload["fname"] = *v
	} else {
		payload["fname"] = "arquivo_moondl"
	}
	return payload
}

func (p MoonDLProvider) BuildDownload2Payload(html, referer string, captchaToken *string) map[string]string {
	payload := map[string]string{}
	for _, kv := range moondlParseHiddenInputs(html) {
		switch kv[0] {
		case "op", "id", "rand", "referer", "method_free", "method_premium", "usr_login", "fname":
			payload[kv[0]] = kv[1]
		}
	}
	if _, ok := payload["referer"]; !ok {
		payload["referer"] = referer
	}
	if captchaToken != nil {
		payload["g-recaptcha-response"] = *captchaToken
		payload["h-captcha-response"] = *captchaToken
	}
	return payload
}

func (p MoonDLProvider) IsBinaryDownloadResponse(resp *http.Response) bool {
	if resp.Header.Get("Content-Disposition") != "" {
		return true
	}
	ct := strings.ToLower(resp.Header.Get("Content-Type"))
	if ct == "" {
		return false
	}
	return !strings.HasPrefix(ct, "text/html") && !strings.HasPrefix(ct, "text/plain") && !strings.HasPrefix(ct, "application/json")
}

func (p MoonDLProvider) IsRemovedPage(html string) bool {
	lower := strings.ToLower(html)
	return strings.Contains(lower, "file was deleted") || strings.Contains(lower, "file not found") || strings.Contains(lower, "404 not found")
}

func (p MoonDLProvider) FetchFileInfo(client *http.Client, rawURL string) (string, uint64, error) {
	resp, err := client.Get(rawURL)
	if err != nil {
		return "", 0, err
	}
	defer resp.Body.Close()
	body, _ := io.ReadAll(io.LimitReader(resp.Body, 512*1024))
	html := string(body)
	if p.IsRemovedPage(html) {
		return "", 0, fmt.Errorf("REMOVED:MoonDL:Arquivo não localizado no MoonDL")
	}
	filename := "arquivo_moondl"
	if v := p.ExtractFilename(html); v != nil {
		filename = *v
	}
	size := p.ExtractSize(html)
	return SanitizeFilename(filename, "arquivo_moondl"), size, nil
}
