package providers

import (
	"fmt"
	"net/http"
	"net/url"
	"strings"
)

// Portado de backend/src/providers/sharepoint.rs — OneDrive/SharePoint public links.

type SharePointProvider struct{}

func (p SharePointProvider) Matches(rawURL string) bool {
	if HostMatches(rawURL, []string{"onedrive.live.com", "1drv.ms"}) {
		return true
	}
	u := ParseURL(rawURL)
	if u == nil {
		return false
	}
	host := strings.ToLower(u.Hostname())
	return strings.HasSuffix(host, "sharepoint.com")
}

func (p SharePointProvider) BuildDownloadURL(rawURL string) (*url.URL, error) {
	parsed, err := url.Parse(rawURL)
	if err != nil {
		return nil, fmt.Errorf("URL do OneDrive/SharePoint inválida: %s", rawURL)
	}
	q := parsed.Query()
	q.Set("download", "1")
	parsed.RawQuery = q.Encode()
	return parsed, nil
}

func (p SharePointProvider) LooksLikeLogin(u *url.URL) bool {
	host := strings.ToLower(u.Hostname())
	return host == "login.microsoftonline.com" || host == "login.live.com"
}

func (p SharePointProvider) IsBinaryResponse(resp *http.Response) bool {
	if resp.Header.Get("Content-Disposition") != "" {
		return true
	}
	ct := strings.ToLower(resp.Header.Get("Content-Type"))
	if ct == "" {
		return false
	}
	return !strings.Contains(ct, "text/html") && !strings.Contains(ct, "application/json")
}

func sharepointDecodePathSegment(value string) string {
	// decode %XX and + -> space, similar to Rust
	var decoded strings.Builder
	for i := 0; i < len(value); {
		if value[i] == '%' && i+2 < len(value) {
			hex := value[i+1 : i+3]
			var v int
			if _, err := fmt.Sscanf(hex, "%02x", &v); err == nil {
				decoded.WriteByte(byte(v))
				i += 3
				continue
			}
		}
		if value[i] == '+' {
			decoded.WriteByte(' ')
		} else {
			decoded.WriteByte(value[i])
		}
		i++
	}
	return decoded.String()
}

func (p SharePointProvider) FilenameFromResponse(resp *http.Response) *string {
	cd := resp.Header.Get("Content-Disposition")
	if cd == "" {
		return nil
	}
	for _, part := range strings.Split(cd, ";") {
		trimmed := strings.TrimSpace(part)
		if strings.HasPrefix(trimmed, "filename*=") {
			value := trimmed[len("filename*="):]
			if idx := strings.LastIndex(value, "''"); idx >= 0 {
				value = value[idx+2:]
			}
			value = strings.Trim(value, `"`)
			decoded := sharepointDecodePathSegment(value)
			if decoded != "" {
				return &decoded
			}
		}
		if strings.HasPrefix(trimmed, "filename=") {
			value := strings.Trim(trimmed[len("filename="):], `"`)
			decoded := sharepointDecodePathSegment(value)
			if decoded != "" {
				return &decoded
			}
		}
	}
	return nil
}

func (p SharePointProvider) FallbackFilename(u *url.URL) string {
	segs := strings.Split(strings.Trim(u.Path, "/"), "/")
	var candidate string
	if len(segs) > 0 {
		last := segs[len(segs)-1]
		if last != "" {
			candidate = sharepointDecodePathSegment(last)
		}
	}
	if candidate == "" {
		candidate = "arquivo_onedrive"
	}
	return SanitizeFilename(candidate, "arquivo_onedrive")
}

func (p SharePointProvider) RequestPublicDownload(client *http.Client, rawURL string) (*http.Response, error) {
	downloadURL, err := p.BuildDownloadURL(rawURL)
	if err != nil {
		return nil, err
	}
	req, _ := http.NewRequest("GET", downloadURL.String(), nil)
	resp, err := client.Do(req)
	if err != nil {
		return nil, err
	}
	if resp.StatusCode >= 400 {
		return nil, fmt.Errorf("OneDrive/SharePoint HTTP %d", resp.StatusCode)
	}
	return resp, nil
}

func (p SharePointProvider) FetchFileInfo(client *http.Client, rawURL string) (string, uint64, *string, error) {
	resp, err := p.RequestPublicDownload(client, rawURL)
	if err != nil {
		return "", 0, nil, err
	}
	defer resp.Body.Close()
	if p.LooksLikeLogin(resp.Request.URL) {
		return "", 0, nil, fmt.Errorf("UNSUPPORTED:OneDrive/SharePoint:Link não suportado pelo fluxo atual do OneDrive/SharePoint")
	}
	if !p.IsBinaryResponse(resp) {
		return "", 0, nil, fmt.Errorf("UNSUPPORTED:OneDrive/SharePoint:Link não suportado pelo fluxo atual do OneDrive/SharePoint")
	}
	var filename string
	if v := p.FilenameFromResponse(resp); v != nil {
		filename = *v
	} else {
		filename = p.FallbackFilename(resp.Request.URL)
	}
	var size uint64
	if cl := resp.Header.Get("Content-Length"); cl != "" {
		fmt.Sscanf(cl, "%d", &size)
	}
	var mime *string
	if ct := resp.Header.Get("Content-Type"); ct != "" {
		mime = &ct
	}
	return SanitizeFilename(filename, "arquivo_onedrive"), size, mime, nil
}
