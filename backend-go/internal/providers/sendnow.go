package providers

import (
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
	"regexp"
	"strings"
)

// Portado de backend/src/providers/sendnow.rs

const sendNowHome = "https://send.now/"

var sendNowHosts = []string{"send.now", "www.send.now"}

type SendNowProvider struct{}

type sendNowTargetType string

const (
	sendNowFile   sendNowTargetType = "file"
	sendNowFolder sendNowTargetType = "folder"
)

type SendNowTarget struct {
	Type sendNowTargetType
	URL  string
	Name string
	ID   string
}

type SendNowFile struct {
	ID        string `json:"id"`
	Name      string `json:"name"`
	Size      uint64 `json:"size"`
	SourceURL string `json:"sourceUrl"`
}

func (p SendNowProvider) Matches(rawURL string) bool {
	if !HostMatches(rawURL, sendNowHosts) {
		return false
	}
	return p.ParseTarget(rawURL) != nil
}

func (p SendNowProvider) ParseTarget(rawURL string) *SendNowTarget {
	if !HostMatches(rawURL, sendNowHosts) {
		return nil
	}
	segs := PathSegments(rawURL)
	switch {
	case len(segs) >= 3 && segs[0] == "s" && segs[1] != "":
		// /s/{folder_id}/{folder_name}
		name := sendNowDecodeHTML(segs[2])
		// strip fragment already via ParseURL
		clean := strings.Split(rawURL, "#")[0]
		return &SendNowTarget{Type: sendNowFolder, URL: clean, Name: name, ID: segs[1]}
	case len(segs) >= 2 && segs[0] == "d" && segs[1] != "":
		return &SendNowTarget{Type: sendNowFile, ID: segs[1]}
	case len(segs) == 1 && len(segs[0]) >= 6 && isAlphanumeric(segs[0]):
		return &SendNowTarget{Type: sendNowFile, ID: segs[0]}
	default:
		return nil
	}
}

func sendNowDecodeHTML(value string) string {
	replacer := strings.NewReplacer("&amp;", "&", "&quot;", "\"", "&#039;", "'", "&lt;", "<", "&gt;", ">")
	return strings.TrimSpace(replacer.Replace(value))
}

func isAlphanumeric(s string) bool {
	for _, ch := range s {
		if !((ch >= 'a' && ch <= 'z') || (ch >= 'A' && ch <= 'Z') || (ch >= '0' && ch <= '9')) {
			return false
		}
	}
	return true
}

func (p SendNowProvider) ResponseIsCloudflareChallenge(status int, body string) bool {
	return status == 403 && (strings.Contains(body, "Just a moment") || strings.Contains(body, "cf-mitigated"))
}

func (p SendNowProvider) FetchHTML(client *http.Client, rawURL string) (string, error) {
	resp, err := client.Get(rawURL)
	if err != nil {
		return "", err
	}
	defer resp.Body.Close()
	body, _ := io.ReadAll(io.LimitReader(resp.Body, 2*1024*1024))
	text := string(body)
	if p.ResponseIsCloudflareChallenge(resp.StatusCode, text) {
		return "", fmt.Errorf("Send.now pediu a verificação do Cloudflare. Abra o link uma vez no navegador e tente novamente")
	}
	if resp.StatusCode >= 400 {
		return "", fmt.Errorf("Send.now respondeu HTTP %d", resp.StatusCode)
	}
	return text, nil
}

func (p SendNowProvider) ParseFolderFiles(html string) []SendNowFile {
	re := regexp.MustCompile(`(?is)<tr\b[^>]*\bclass\s*=\s*["'][^"']*\bselectable\b[^"']*["'][^>]*>.*?<a\b[^>]*\bhref\s*=\s*["']https?://(?:www\.)?send\.now/(?:d/)?([A-Za-z0-9]+)[^"']*["'][^>]*>\s*(.*?)\s*</a>.*?<span\b[^>]*>\s*([^<]+?)\s*</span>.*?</tr>`)
	if re == nil {
		return nil
	}
	var out []SendNowFile
	for _, m := range re.FindAllStringSubmatch(html, -1) {
		if len(m) < 4 {
			continue
		}
		id := strings.TrimSpace(m[1])
		rawName := regexp.MustCompile(`(?is)<[^>]+>`).ReplaceAllString(m[2], " ")
		name := SanitizeFilename(sendNowDecodeHTML(rawName), "arquivo_sendnow")
		if id == "" || name == "" {
			continue
		}
		size := ParseHumanSize(sendNowDecodeHTML(m[3]))
		out = append(out, SendNowFile{
			ID:        id,
			Name:      name,
			Size:      size,
			SourceURL: fmt.Sprintf("https://send.now/%s", id),
		})
	}
	return out
}

func (p SendNowProvider) IsDocumentResponse(resp *http.Response) bool {
	ct := strings.ToLower(resp.Header.Get("Content-Type"))
	hasAttachment := resp.Header.Get("Content-Disposition") != ""
	return !hasAttachment && (strings.Contains(ct, "text/html") || strings.Contains(ct, "application/javascript") || strings.Contains(ct, "text/javascript"))
}

func sendNowElectronProxyPort() string {
	return os.Getenv("SENDNOW_PROXY_PORT")
}

func sendNowHelperProxyToken() string {
	return strings.TrimSpace(os.Getenv("GDOWNLOADER_HELPER_TOKEN"))
}

func (p SendNowProvider) browserAction(payload interface{}) (map[string]interface{}, error) {
	port := sendNowElectronProxyPort()
	token := sendNowHelperProxyToken()
	if port == "" || token == "" {
		return nil, fmt.Errorf("Helper local do Send.now não disponível")
	}
	body, _ := json.Marshal(payload)
	req, _ := http.NewRequest("POST", fmt.Sprintf("http://127.0.0.1:%s/", port), strings.NewReader(string(body)))
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("X-GDownloader-Token", token)
	client := &http.Client{}
	resp, err := client.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	b, _ := io.ReadAll(resp.Body)
	if resp.StatusCode >= 400 {
		return nil, fmt.Errorf("Helper local do Send.now respondeu HTTP %d: %s", resp.StatusCode, strings.TrimSpace(string(b)))
	}
	var out map[string]interface{}
	if err := json.Unmarshal(b, &out); err != nil {
		return nil, fmt.Errorf("Resposta inválida do helper local do Send.now: %v: %s", err, string(b))
	}
	return out, nil
}

func (p SendNowProvider) ResolveDirectURL(client *http.Client, id string) (string, error) {
	sourceURL := fmt.Sprintf("https://send.now/%s", id)
	// Try browser helper first
	result, browserErr := p.browserAction(map[string]interface{}{"action": "sendnow_resolve", "url": sourceURL})
	if browserErr == nil {
		if urlStr, ok := result["url"].(string); ok && (strings.HasPrefix(urlStr, "http://") || strings.HasPrefix(urlStr, "https://")) {
			return urlStr, nil
		}
	}
	// Fallback: emulate POST to SEND_NOW_HOME
	resolver := &http.Client{
		CheckRedirect: func(req *http.Request, via []*http.Request) error {
			return http.ErrUseLastResponse
		},
	}
	downloadID := id
	randVal := ""
	referer := sourceURL
	if result != nil {
		if v, ok := result["downloadId"].(string); ok && v != "" {
			allAlphaNum := true
			for _, ch := range v {
				if !((ch >= 'a' && ch <= 'z') || (ch >= 'A' && ch <= 'Z') || (ch >= '0' && ch <= '9')) {
					allAlphaNum = false
					break
				}
			}
			if allAlphaNum {
				downloadID = v
			}
		}
		if v, ok := result["rand"].(string); ok {
			randVal = v
		}
		if v, ok := result["referer"].(string); ok && strings.HasPrefix(v, "https://send.now/") {
			referer = v
		}
	}
	form := fmt.Sprintf("op=download2&id=%s&rand=%s&referer=%s&method_free=&method_premium=", downloadID, randVal, referer)
	// url encode referer
	// quick escape
	form = strings.ReplaceAll(form, " ", "%20")
	req, _ := http.NewRequest("POST", sendNowHome, strings.NewReader(form))
	req.Header.Set("Content-Type", "application/x-www-form-urlencoded")
	req.Header.Set("Referer", sendNowHome)
	if result != nil {
		if ua, ok := result["userAgent"].(string); ok && strings.TrimSpace(ua) != "" {
			req.Header.Set("User-Agent", ua)
		}
		if cookie, ok := result["cookieHeader"].(string); ok && strings.TrimSpace(cookie) != "" {
			req.Header.Set("Cookie", cookie)
		}
	}
	resp, err := resolver.Do(req)
	if err != nil {
		return "", err
	}
	defer resp.Body.Close()
	loc := resp.Header.Get("Location")
	if loc != "" && (strings.HasPrefix(loc, "http://") || strings.HasPrefix(loc, "https://")) {
		return loc, nil
	}
	detail := ""
	if browserErr != nil {
		detail = fmt.Sprintf(" (%v)", browserErr)
	}
	return "", fmt.Errorf("Send.now não retornou o link temporário do arquivo%s", detail)
}

func (p SendNowProvider) FilenameFromContentDisposition(resp *http.Response) *string {
	cd := resp.Header.Get("Content-Disposition")
	if cd == "" {
		return nil
	}
	re := regexp.MustCompile(`(?i)filename\*?=(?:UTF-8''|")?([^;"]+)`)
	if m := re.FindStringSubmatch(cd); len(m) > 1 {
		decoded := m[1]
		// url decode if needed
		if strings.Contains(decoded, "%") {
			if u, err := regexp.Compile(`%[0-9A-Fa-f]{2}`); err == nil && u.MatchString(decoded) {
				// simple unescape
				decoded = strings.ReplaceAll(decoded, "%20", " ")
			}
		}
		decoded = strings.TrimSpace(strings.Trim(decoded, `"`))
		if decoded != "" {
			return &decoded
		}
	}
	return nil
}

func (p SendNowProvider) ProbeSingleFile(client *http.Client, id string) (*string, uint64, error) {
	directURL, err := p.ResolveDirectURL(client, id)
	if err != nil {
		return nil, 0, err
	}
	req, _ := http.NewRequest("GET", directURL, nil)
	req.Header.Set("Referer", sendNowHome)
	req.Header.Set("Range", "bytes=0-0")
	resp, err := client.Do(req)
	if err != nil {
		return nil, 0, err
	}
	defer resp.Body.Close()
	if resp.StatusCode >= 400 {
		return nil, 0, fmt.Errorf("Send.now probe HTTP %d", resp.StatusCode)
	}
	name := p.FilenameFromContentDisposition(resp)
	var size uint64
	if cr := resp.Header.Get("Content-Range"); cr != "" {
		if parts := strings.Split(cr, "/"); len(parts) > 1 {
			fmt.Sscanf(strings.TrimSpace(parts[len(parts)-1]), "%d", &size)
		}
	} else if cl := resp.Header.Get("Content-Length"); cl != "" {
		fmt.Sscanf(cl, "%d", &size)
		if resp.StatusCode == 206 {
			// content-length is 1, total is in content-range, but fallback
		}
	}
	return name, size, nil
}
