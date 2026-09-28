package providers

import (
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"regexp"
	"strings"
)

// Portado de backend/src/providers/gdrive.rs

type GDriveProvider struct{}

const GDriveFolderMime = "application/vnd.google-apps.folder"

func (p GDriveProvider) Matches(rawURL string) bool {
	return HostMatches(rawURL, []string{"drive.google.com"})
}

func (p GDriveProvider) IsFolderURL(rawURL string) bool {
	return strings.Contains(rawURL, "/drive/folders/")
}

func (p GDriveProvider) ExtractFolderID(rawURL string) *string {
	pos := strings.Index(rawURL, "/drive/folders/")
	if pos < 0 {
		return nil
	}
	after := rawURL[pos+15:]
	parts := strings.Split(after, "/")
	id := strings.Split(parts[0], "?")[0]
	id = strings.Split(id, "#")[0]
	id = strings.TrimSpace(id)
	if id == "" {
		return nil
	}
	return &id
}

func (p GDriveProvider) ExtractID(rawURL string) *string {
	if pos := strings.Index(rawURL, "/file/d/"); pos >= 0 {
		after := rawURL[pos+8:]
		parts := strings.Split(after, "/")
		id := strings.Split(parts[0], "?")[0]
		if id != "" {
			return &id
		}
	}
	if pos := strings.Index(rawURL, "id="); pos >= 0 {
		after := rawURL[pos+3:]
		parts := strings.Split(after, "&")
		id := strings.Split(parts[0], "#")[0]
		if id != "" {
			return &id
		}
	}
	return nil
}

func (p GDriveProvider) DownloadURL(id string) string {
	return fmt.Sprintf("https://drive.google.com/uc?export=download&id=%s", id)
}

func isBinaryResponse(resp *http.Response) bool {
	if resp.Header.Get("Content-Disposition") != "" {
		return true
	}
	ct := strings.ToLower(resp.Header.Get("Content-Type"))
	if ct == "" {
		return false
	}
	return !strings.Contains(ct, "text/html")
}

func isHTMLResponse(resp *http.Response) bool {
	ct := strings.ToLower(resp.Header.Get("Content-Type"))
	return strings.Contains(ct, "text/html")
}

func decodeHTML(value string) string {
	replacer := strings.NewReplacer("&amp;", "&", "&quot;", "\"", "&#039;", "'", "&#39;", "'", "&lt;", "<", "&gt;", ">")
	return replacer.Replace(value)
}

func stripHTML(value string) string {
	re := regexp.MustCompile(`(?is)<[^>]+>`)
	without := re.ReplaceAllString(value, " ")
	decoded := decodeHTML(without)
	fields := strings.Fields(decoded)
	return strings.Join(fields, " ")
}

func extractTitle(html, fallback string) string {
	re := regexp.MustCompile(`(?is)<title>\s*(.*?)\s*(?:-\s*Google Drive)?\s*</title>`)
	m := re.FindStringSubmatch(html)
	if len(m) > 1 {
		title := decodeHTML(strings.TrimSpace(m[1]))
		if title != "" {
			return title
		}
	}
	return fallback
}

func (p GDriveProvider) ExtractDriveIVD(html string) *string {
	marker := "window['_DRIVE_ivd'] = '"
	pos := strings.Index(html, marker)
	if pos < 0 {
		return nil
	}
	start := pos + len(marker)
	end := strings.Index(html[start:], "';")
	if end < 0 {
		return nil
	}
	raw := html[start : start+end]
	decoded := decodeJSString(raw)
	decoded = strings.ReplaceAll(decoded, "\\=", "=")
	return &decoded
}

func decodeJSString(value string) string {
	var b strings.Builder
	runes := []rune(value)
	for i := 0; i < len(runes); i++ {
		ch := runes[i]
		if ch != '\\' {
			b.WriteRune(ch)
			continue
		}
		if i+1 >= len(runes) {
			b.WriteRune('\\')
			break
		}
		next := runes[i+1]
		i++
		switch next {
		case 'x':
			if i+2 < len(runes) {
				hex := string(runes[i+1 : i+3])
				var v int
				if _, err := fmt.Sscanf(hex, "%02x", &v); err == nil {
					b.WriteRune(rune(v))
					i += 2
				}
			}
		case 'u':
			if i+4 < len(runes) {
				hex := string(runes[i+1 : i+5])
				var v int
				if _, err := fmt.Sscanf(hex, "%04x", &v); err == nil {
					b.WriteRune(rune(v))
					i += 4
				}
			}
		case '/':
			b.WriteRune('/')
		case '\\':
			b.WriteRune('\\')
		case '"':
			b.WriteRune('"')
		case '\'':
			b.WriteRune('\'')
		case 'n':
			b.WriteRune('\n')
		case 'r':
			b.WriteRune('\r')
		case 't':
			b.WriteRune('\t')
		default:
			b.WriteRune('\\')
			b.WriteRune(next)
		}
	}
	return b.String()
}

func (p GDriveProvider) ExtractConfirmDownloadURL(html string) *string {
	re := regexp.MustCompile(`id="download-form"[^>]*action="([^"]+)"`)
	m := re.FindStringSubmatch(html)
	if len(m) < 2 {
		return nil
	}
	u, err := ParseURL(m[1]).Parse(m[1])
	_ = u
	_ = err
	// Use net/url parsing
	parsed, err := regexp.Compile(`type="hidden"\s+name="([^"]+)"\s+value="([^"]*)"`)
	if err != nil {
		return nil
	}
	base := m[1]
	// Append hidden inputs as query
	for _, cap := range parsed.FindAllStringSubmatch(html, -1) {
		name := cap[1]
		value := decodeHTML(cap[2])
		if strings.Contains(base, "?") {
			base += "&" + name + "=" + value
		} else {
			base += "?" + name + "=" + value
		}
	}
	return &base
}

func (p GDriveProvider) ExtractWarningPageMetadata(html, fallbackID string) (string, uint64) {
	filename := fallbackID
	re := regexp.MustCompile(`(?is)<span[^>]*class="uc-name-size"[^>]*>\s*<a [^>]+>([^<]+)</a>`)
	if m := re.FindStringSubmatch(html); len(m) > 1 {
		decoded := decodeHTML(strings.TrimSpace(m[1]))
		if decoded != "" {
			filename = decoded
		}
	}
	size := uint64(0)
	reSize := regexp.MustCompile(`(?is)\(([0-9]+(?:\.[0-9]+)?)\s*([KMGT]?B?)\).*?too large`)
	if m := reSize.FindStringSubmatch(html); len(m) > 1 {
		var num float64
		fmt.Sscanf(m[1], "%f", &num)
		unit := strings.ToUpper(m[2])
		var mult float64 = 1
		switch unit {
		case "KB", "K":
			mult = 1024
		case "MB", "M":
			mult = 1024 * 1024
		case "GB", "G":
			mult = 1024 * 1024 * 1024
		case "TB", "T":
			mult = 1024 * 1024 * 1024 * 1024
		}
		size = uint64(num * mult)
	}
	return filename, size
}

func (p GDriveProvider) ResolveDownloadURL(client *http.Client, id string) (string, *string, uint64, error) {
	initialURL := p.DownloadURL(id)
	resp, err := client.Get(initialURL)
	if err != nil {
		return "", nil, 0, err
	}
	defer resp.Body.Close()
	if isBinaryResponse(resp) {
		var filename *string
		if cd := resp.Header.Get("Content-Disposition"); cd != "" {
			if pos := strings.Index(cd, "filename=\""); pos >= 0 {
				start := pos + 10
				end := strings.Index(cd[start:], "\"")
				if end >= 0 {
					f := cd[start : start+end]
					filename = &f
				}
			}
		}
		var size uint64
		if cl := resp.Header.Get("Content-Length"); cl != "" {
			fmt.Sscanf(cl, "%d", &size)
		}
		return initialURL, filename, size, nil
	}
	body, _ := io.ReadAll(io.LimitReader(resp.Body, 4*1024*1024))
	html := string(body)
	filename, size := p.ExtractWarningPageMetadata(html, id)
	confirmURL := p.ExtractConfirmDownloadURL(html)
	if confirmURL == nil {
		return "", nil, 0, fmt.Errorf("Google Drive não expôs o link final de confirmação")
	}
	return *confirmURL, &filename, size, nil
}

func (p GDriveProvider) EnsureDownloadResponse(resp *http.Response) (*http.Response, error) {
	if !isHTMLResponse(resp) {
		return resp, nil
	}
	body, _ := io.ReadAll(io.LimitReader(resp.Body, 2*1024*1024))
	html := string(body)
	title := extractTitle(html, "Google Drive retornou uma página HTML")
	re := regexp.MustCompile(`(?is)<p[^>]*class="uc-(?:error|warning)-(?:caption|subcaption)"[^>]*>(.*?)</p>`)
	matches := re.FindAllStringSubmatch(html, -1)
	var messages []string
	for _, m := range matches {
		v := stripHTML(m[1])
		if strings.TrimSpace(v) != "" {
			messages = append(messages, v)
		}
	}
	message := title
	if len(messages) > 0 {
		message = strings.Join(messages, " ")
	}
	lower := strings.ToLower(message)
	if strings.Contains(lower, "too many users") || strings.Contains(lower, "quota exceeded") || strings.Contains(lower, "view or download this file at this time") {
		return nil, fmt.Errorf("RATE_LIMIT:86400:Google Drive atingiu a cota pública deste arquivo. %s", message)
	}
	return nil, fmt.Errorf("Google Drive não liberou o arquivo: %s", message)
}

// ParseFolderChildrenFromIVD exposto para testes — portado de gdrive.rs:225
func (p GDriveProvider) ParseFolderChildrenFromIVD(ivd string) ([]map[string]interface{}, error) {
	return p.ParseFolderChildrenFromIVDWithPrefix(ivd, "")
}

func (p GDriveProvider) ParseFolderChildrenFromIVDWithPrefix(ivd, prefix string) ([]map[string]interface{}, error) {
	var value []interface{}
	if err := json.Unmarshal([]byte(ivd), &value); err != nil {
		return nil, err
	}
	if len(value) == 0 {
		return nil, fmt.Errorf("Google Drive não retornou a lista de arquivos da pasta")
	}
	entries, ok := value[0].([]interface{})
	if !ok {
		return nil, fmt.Errorf("Google Drive não retornou a lista de arquivos da pasta")
	}
	var out []map[string]interface{}
	for _, entry := range entries {
		arr, ok := entry.([]interface{})
		if !ok || len(arr) < 14 {
			continue
		}
		id, _ := arr[0].(string)
		filename, _ := arr[2].(string)
		mime, _ := arr[3].(string)
		var size uint64
		if v, ok := arr[13].(float64); ok {
			size = uint64(v)
		}
		if id == "" || filename == "" {
			continue
		}
		isFolder := mime == GDriveFolderMime
		path := filename
		if prefix != "" {
			path = strings.Trim(prefix, "/") + "/" + filename
		}
		sourceURL := ""
		if isFolder {
			sourceURL = fmt.Sprintf("https://drive.google.com/drive/folders/%s", id)
		} else {
			sourceURL = fmt.Sprintf("https://drive.google.com/file/d/%s/view", id)
		}
		out = append(out, map[string]interface{}{
			"id": id, "filename": filename, "mimeType": mime, "size": size, "isFolder": isFolder, "path": path, "sourceUrl": sourceURL,
		})
	}
	if len(out) == 0 {
		return nil, fmt.Errorf("Pasta do Google Drive vazia ou sem arquivos acessíveis")
	}
	return out, nil
}
