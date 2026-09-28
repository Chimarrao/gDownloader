package providers

import (
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"regexp"
	"strings"
)

// Portado de backend/src/providers/rapidgator.rs

type RapidgatorProvider struct{}

func (p RapidgatorProvider) Matches(rawURL string) bool {
	if !HostMatches(rawURL, []string{"rapidgator.net", "www.rapidgator.net"}) {
		return false
	}
	segs := PathSegments(rawURL)
	if len(segs) < 2 {
		return false
	}
	return segs[0] == "file"
}

func (p RapidgatorProvider) FileID(rawURL string) *string {
	clean := strings.Split(rawURL, "#")[0]
	re := regexp.MustCompile(`rapidgator\.net/file/([a-f0-9]+)`)
	if m := re.FindStringSubmatch(clean); len(m) > 1 {
		v := m[1]
		return &v
	}
	return nil
}

func (p RapidgatorProvider) ExtractCaptchaToken(rawURL string) *string {
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

func (p RapidgatorProvider) DetectRecaptchaSitekey(html string) *string {
	re := regexp.MustCompile(`g-recaptcha[^>]+data-sitekey=["']([^"']+)["']`)
	if m := re.FindStringSubmatch(html); len(m) > 1 {
		return &m[1]
	}
	return nil
}

func (p RapidgatorProvider) DetectHCaptchaSitekey(html string) *string {
	re := regexp.MustCompile(`h-captcha[^>]+data-sitekey=["']([^"']+)["']`)
	if m := re.FindStringSubmatch(html); len(m) > 1 {
		return &m[1]
	}
	return nil
}

func (p RapidgatorProvider) ParseWaitTime(html string) *uint64 {
	lower := strings.ToLower(html)
	for _, phrase := range []string{"please wait ", "try again in ", "wait "} {
		if pos := strings.Index(lower, phrase); pos >= 0 {
			rest := lower[pos+len(phrase):]
			var digits strings.Builder
			for _, ch := range rest {
				if ch >= '0' && ch <= '9' {
					digits.WriteRune(ch)
				} else if digits.Len() > 0 {
					break
				}
			}
			if digits.Len() > 0 {
				var n uint64
				fmt.Sscanf(digits.String(), "%d", &n)
				after := strings.TrimSpace(rest[digits.Len():])
				if strings.HasPrefix(after, "hour") {
					v := n * 3600
					return &v
				}
				if strings.HasPrefix(after, "minute") {
					v := n * 60
					return &v
				}
				if strings.HasPrefix(after, "second") {
					return &n
				}
			}
		}
	}
	if pos := strings.Index(html, `"delay":`); pos >= 0 {
		rest := html[pos+8:]
		var digits strings.Builder
		for _, ch := range rest {
			if ch >= '0' && ch <= '9' {
				digits.WriteRune(ch)
			} else if digits.Len() > 0 {
				break
			}
		}
		if digits.Len() > 0 {
			var n uint64
			fmt.Sscanf(digits.String(), "%d", &n)
			if n > 0 {
				return &n
			}
		}
	}
	return nil
}

func (p RapidgatorProvider) ExtractFilename(html string) *string {
	lower := strings.ToLower(html)
	if pos := strings.Index(lower, "<title>"); pos >= 0 {
		rest := html[pos+7:]
		if end := strings.Index(rest, "</title>"); end >= 0 {
			title := strings.TrimSpace(rest[:end])
			cleaned := strings.TrimSpace(strings.TrimPrefix(strings.TrimPrefix(title, "Download file "), "Download "))
			if cleaned != "" && !strings.Contains(strings.ToLower(cleaned), "rapidgator") {
				return &cleaned
			}
		}
	}
	for _, tag := range []string{"<h1>", "<h1 "} {
		if pos := strings.Index(html, tag); pos >= 0 {
			rest := html[pos:]
			if start := strings.Index(rest, ">"); start >= 0 {
				rest2 := rest[start+1:]
				if end := strings.Index(rest2, "</h1>"); end >= 0 {
					name := strings.TrimSpace(rest2[:end])
					if name != "" && !strings.Contains(strings.ToLower(name), "rapidgator") {
						return &name
					}
				}
			}
		}
	}
	return nil
}

func (p RapidgatorProvider) ExtractSize(html string) uint64 {
	lower := strings.ToLower(html)
	if pos := strings.Index(lower, "file size:"); pos >= 0 {
		slice := html[pos:min(pos+300, len(html))]
		re := regexp.MustCompile(`([0-9]+(?:[.,][0-9]+)?)\s*(KB|MB|GB|TB)`)
		if m := re.FindStringSubmatch(slice); len(m) > 2 {
			return ParseHumanSize(m[1] + " " + m[2])
		}
	}
	for _, unit := range []string{"kb", "mb", "gb", "tb"} {
		if pos := strings.Index(lower, unit); pos >= 0 {
			before := strings.TrimSpace(html[:pos])
			// find start of number
			start := -1
			for i := len(before) - 1; i >= 0; i-- {
				ch := before[i]
				if (ch >= '0' && ch <= '9') || ch == '.' || ch == ',' {
					start = i
				} else {
					if start != -1 {
						break
					}
				}
			}
			if start >= 0 {
				numStr := strings.TrimSpace(before[start:])
				if numStr != "" {
					return ParseHumanSize(numStr + " " + strings.ToUpper(unit))
				}
			}
		}
	}
	return 0
}

func (p RapidgatorProvider) IsRemovedPage(html string) bool {
	lower := strings.ToLower(html)
	return strings.Contains(lower, "file not found") || strings.Contains(lower, "file was removed") || strings.Contains(lower, "download file not found") || strings.Contains(lower, "error 404")
}

func (p RapidgatorProvider) ExtractNumericFID(html string) *string {
	re := regexp.MustCompile(`var\s+fid\s*=\s*(\d+)`)
	if m := re.FindStringSubmatch(html); len(m) > 1 {
		return &m[1]
	}
	return nil
}

func (p RapidgatorProvider) ExtractJSStringVar(html, name string) *string {
	pattern := fmt.Sprintf(`var\s+%s\s*=\s*'([^']*)'`, regexp.QuoteMeta(name))
	re := regexp.MustCompile(pattern)
	if m := re.FindStringSubmatch(html); len(m) > 1 {
		return &m[1]
	}
	return nil
}

func (p RapidgatorProvider) ExtractJSNumberVar(html, name string) *uint64 {
	pattern := fmt.Sprintf(`var\s+%s\s*=\s*(\d+)`, regexp.QuoteMeta(name))
	re := regexp.MustCompile(pattern)
	if m := re.FindStringSubmatch(html); len(m) > 1 {
		var v uint64
		fmt.Sscanf(m[1], "%d", &v)
		return &v
	}
	return nil
}

func (p RapidgatorProvider) AbsoluteURL(pathOrURL string) string {
	if strings.HasPrefix(pathOrURL, "http://") || strings.HasPrefix(pathOrURL, "https://") {
		return pathOrURL
	}
	return "https://rapidgator.net" + pathOrURL
}

func (p RapidgatorProvider) IsFreeLimitBlock(html string, size uint64) bool {
	return size > 1024*1024*1024 && strings.Contains(strings.ToLower(html), "download files up to 1 gb in free mode")
}

func (p RapidgatorProvider) ExtractReadyDownloadLink(html string) *string {
	if m := regexp.MustCompile(`download_link\s*=\s*'([^']+)'`).FindStringSubmatch(html); len(m) > 1 {
		return &m[1]
	}
	re := regexp.MustCompile(`href=["'](https?://[^"']+)["'][^>]*class=["'][^"']*btn-download`)
	if m := re.FindStringSubmatch(html); len(m) > 1 {
		return &m[1]
	}
	return nil
}

func (p RapidgatorProvider) FetchFileInfo(client *http.Client, rawURL string) (string, uint64, error) {
	if p.FileID(rawURL) == nil {
		return "", 0, fmt.Errorf("URL do Rapidgator inválida")
	}
	resp, err := client.Get(rawURL)
	if err != nil {
		return "", 0, err
	}
	defer resp.Body.Close()
	body, _ := io.ReadAll(io.LimitReader(resp.Body, 512*1024))
	html := string(body)
	if p.IsRemovedPage(html) {
		return "", 0, fmt.Errorf("REMOVED:Rapidgator:Arquivo não localizado no Rapidgator")
	}
	if secs := p.ParseWaitTime(html); secs != nil {
		return "", 0, fmt.Errorf("RATE_LIMIT:%d:Rapidgator: aguarde %d hora(s)", *secs, *secs/3600)
	}
	if sk := p.DetectRecaptchaSitekey(html); sk != nil {
		return "", 0, fmt.Errorf("CAPTCHA_REQUIRED:recaptcha2:%s:%s", *sk, rawURL)
	}
	if sk := p.DetectHCaptchaSitekey(html); sk != nil {
		return "", 0, fmt.Errorf("CAPTCHA_REQUIRED:hcaptcha:%s:%s", *sk, rawURL)
	}
	filename := "arquivo_rapidgator"
	if v := p.ExtractFilename(html); v != nil {
		filename = *v
	}
	size := p.ExtractSize(html)
	return SanitizeFilename(filename, "arquivo_rapidgator"), size, nil
}

func (p RapidgatorProvider) ResolveCaptchaPage(client *http.Client, captchaPageURL, referer string, captchaToken *string) (*string, error) {
	if captchaToken != nil {
		// POST captcha
		bodyStr := fmt.Sprintf("g-recaptcha-response=%s&h-captcha-response=%s&captcha_token=%s", *captchaToken, *captchaToken, *captchaToken)
		req, _ := http.NewRequest("POST", captchaPageURL, strings.NewReader(bodyStr))
		req.Header.Set("Content-Type", "application/x-www-form-urlencoded")
		req.Header.Set("Referer", referer)
		resp, err := client.Do(req)
		if err != nil {
			return nil, err
		}
		defer resp.Body.Close()
		finalURL := resp.Request.URL.String()
		b, _ := io.ReadAll(io.LimitReader(resp.Body, 512*1024))
		html := string(b)
		if !strings.Contains(finalURL, "/download/captcha") {
			return &finalURL, nil
		}
		if v := p.ExtractReadyDownloadLink(html); v != nil {
			return v, nil
		}
	}
	resp, err := client.Get(captchaPageURL)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	b, _ := io.ReadAll(io.LimitReader(resp.Body, 512*1024))
	html := string(b)
	if sk := p.DetectRecaptchaSitekey(html); sk != nil {
		return nil, fmt.Errorf("CAPTCHA_REQUIRED:recaptcha2:%s:%s", *sk, captchaPageURL)
	}
	if sk := p.DetectHCaptchaSitekey(html); sk != nil {
		return nil, fmt.Errorf("CAPTCHA_REQUIRED:hcaptcha:%s:%s", *sk, captchaPageURL)
	}
	return p.ExtractReadyDownloadLink(html), nil
}

func rapidgatorFetchJSON(client *http.Client, url string, params map[string]string) (map[string]interface{}, error) {
	req, _ := http.NewRequest("GET", url, nil)
	q := req.URL.Query()
	for k, v := range params {
		q.Set(k, v)
	}
	req.URL.RawQuery = q.Encode()
	req.Header.Set("X-Requested-With", "XMLHttpRequest")
	resp, err := client.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	var out map[string]interface{}
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		return nil, err
	}
	return out, nil
}

func min(a, b int) int {
	if a < b {
		return a
	}
	return b
}
