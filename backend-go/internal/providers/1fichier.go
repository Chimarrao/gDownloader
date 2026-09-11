package providers

import (
	"database/sql"
	"fmt"
	"io"
	"net/http"
	"regexp"
	"strings"
	"time"

	_ "modernc.org/sqlite"
)

// Portado de backend/src/providers/1fichier.rs — 1Fichier free download + folder + resolved link cache.

const fichierCacheTTLSeconds = 24 * 60 * 60

type FichierProvider struct{}

func (p FichierProvider) Matches(rawURL string) bool {
	u := ParseURL(rawURL)
	if u == nil {
		return false
	}
	host := strings.ToLower(u.Hostname())
	return host == "1fichier.com" || strings.HasSuffix(host, ".1fichier.com")
}

func (p FichierProvider) ExtractDownloadPage(rawURL string) *string {
	if !p.Matches(rawURL) {
		return nil
	}
	normalized := strings.TrimSpace(rawURL)
	if normalized == "" {
		return nil
	}
	return &normalized
}

func fichierExtractBetween(haystack, start, end string) *string {
	startIdx := strings.Index(haystack, start)
	if startIdx < 0 {
		return nil
	}
	startIdx += len(start)
	rest := haystack[startIdx:]
	endIdx := strings.Index(rest, end)
	if endIdx < 0 {
		return nil
	}
	v := strings.TrimSpace(rest[:endIdx])
	return &v
}

func fichierDecodeBasicHTMLEntities(value string) string {
	replacer := strings.NewReplacer(
		"&amp;", "&",
		"&quot;", "\"",
		"&#039;", "'",
		"&lt;", "<",
		"&gt;", ">",
		"&nbsp;", " ",
	)
	return replacer.Replace(value)
}

func fichierHTMLToText(value string) string {
	re := regexp.MustCompile(`(?is)<[^>]*>`)
	without := re.ReplaceAllString(value, "")
	decoded := fichierDecodeBasicHTMLEntities(without)
	fields := strings.Fields(decoded)
	return strings.Join(fields, " ")
}

func (p FichierProvider) ExtractFolderName(html string) *string {
	re := regexp.MustCompile(`(?is)<div\b[^>]*\bclass\s*=\s*["'][^"']*\bbh3\b[^"']*["'][^>]*>.*?<span\b[^>]*>(.*?)</span>`)
	if m := re.FindStringSubmatch(html); len(m) > 1 {
		name := fichierHTMLToText(m[1])
		if name != "" {
			return &name
		}
	}
	if v := fichierExtractBetween(html, `<div class="bh3 alc">`, `</div>`); v != nil {
		text := fichierHTMLToText(*v)
		if text != "" {
			return &text
		}
	}
	return nil
}

func (p FichierProvider) ExtractFilenameAndSize(html, fallbackName string) (string, uint64) {
	var filename string
	if v := fichierExtractBetween(html, `<span style="font-weight:bold">`, `</span>`); v != nil {
		decoded := fichierDecodeBasicHTMLEntities(*v)
		if strings.TrimSpace(decoded) != "" {
			filename = decoded
		}
	}
	if filename == "" {
		if v := fichierExtractBetween(html, "<title>", "</title>"); v != nil {
			cleaned := strings.ReplaceAll(*v, " - 1fichier.com", "")
			cleaned = strings.TrimSpace(cleaned)
			if cleaned != "" {
				filename = cleaned
			}
		}
	}
	if filename == "" {
		filename = fallbackName
	}
	var humanSize string
	if v := fichierExtractBetween(html, `<span style="font-size:0.9em;font-style:italic">`, `</span>`); v != nil {
		humanSize = *v
	}
	size := fichierParseHumanSize(humanSize)
	return filename, size
}

func fichierParseHumanSize(value string) uint64 {
	trimmed := strings.TrimSpace(value)
	if trimmed == "" {
		return 0
	}
	parts := strings.Fields(trimmed)
	if len(parts) == 0 {
		return 0
	}
	numStr := strings.ReplaceAll(parts[0], ",", ".")
	var num float64
	fmt.Sscanf(numStr, "%f", &num)
	unit := ""
	if len(parts) > 1 {
		unit = strings.ToUpper(parts[1])
	}
	var mult float64 = 1
	switch unit {
	case "B":
		mult = 1
	case "KB":
		mult = 1024
	case "MB":
		mult = 1024 * 1024
	case "GB":
		mult = 1024 * 1024 * 1024
	case "TB":
		mult = 1024 * 1024 * 1024 * 1024
	}
	return uint64(num * mult)
}

func (p FichierProvider) ExtractWaitSeconds(html string) *uint64 {
	marker := "var ct = "
	if pos := strings.Index(html, marker); pos >= 0 {
		rest := html[pos+len(marker):]
		var digits strings.Builder
		for _, ch := range rest {
			if ch >= '0' && ch <= '9' {
				digits.WriteRune(ch)
			} else {
				break
			}
		}
		if digits.Len() > 0 {
			var v uint64
			fmt.Sscanf(digits.String(), "%d", &v)
			return &v
		}
	}
	lower := strings.ToLower(html)
	if pos := strings.Index(lower, "wait "); pos >= 0 {
		rest := lower[pos+5:]
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
	if secs := ExtractWaitSecondsFromText(html); secs != nil {
		return secs
	}
	return nil
}

func (p FichierProvider) HasFreeSlotError(html string) bool {
	return strings.Contains(html, "All free guest slots are currently in use") ||
		strings.Contains(html, "Sign in instantly to continue your download")
}

func (p FichierProvider) HasRestrictedAccessError(html string) bool {
	return strings.Contains(html, "professional infrastructure detected") ||
		strings.Contains(html, "Accès restreint") ||
		strings.Contains(html, "professional network infrastructures")
}

func (p FichierProvider) IsFolderPage(rawURL, html string) bool {
	return strings.Contains(rawURL, "/dir/") || strings.Contains(html, "liste des fichiers") || strings.Contains(html, "file list")
}

type FichierChildInfo struct {
	Filename  string
	Size      uint64
	SourceURL string
	Path      string
}

func (p FichierProvider) ExtractFolderChildren(html string) []FichierChildInfo {
	re := regexp.MustCompile(`(?is)<td\b[^>]*\bclass\s*=\s*["'][^"']*\bfile-obj\b[^"']*["'][^>]*>\s*<a\b[^>]*\bhref\s*=\s*["'](https://1fichier\.com/\?[^"']+)["'][^>]*>(.*?)</a>\s*</td>\s*<td\b[^>]*>\s*([^<]+)`)
	if re == nil {
		return nil
	}
	var out []FichierChildInfo
	for _, m := range re.FindAllStringSubmatch(html, -1) {
		if len(m) < 4 {
			continue
		}
		url := strings.TrimSpace(m[1])
		filename := SanitizeFilename(fichierHTMLToText(strings.TrimSpace(m[2])), "arquivo_1fichier")
		size := fichierParseHumanSize(fichierDecodeBasicHTMLEntities(strings.TrimSpace(m[3])))
		if filename == "" || url == "" {
			continue
		}
		out = append(out, FichierChildInfo{
			Filename:  filename,
			Size:      size,
			SourceURL: url,
			Path:      filename,
		})
	}
	return out
}

func (p FichierProvider) ExtractDirectLink(html string) *string {
	patterns := []string{
		`(?is)<a\b[^>]*class="[^"]*btn-orange[^"]*"[^>]*href="(https?://[^"]+)"`,
		`(?is)<a\b[^>]*href="(https?://[^"]+)"[^>]*class="[^"]*btn-orange[^"]*"`,
	}
	for _, pat := range patterns {
		re := regexp.MustCompile(pat)
		if m := re.FindStringSubmatch(html); len(m) > 1 {
			href := m[1]
			if fichierIsPlausibleDirectLink(href) {
				return &href
			}
		}
	}
	cursor := html
	for {
		pos := strings.Index(cursor, `href="`)
		if pos < 0 {
			break
		}
		rest := cursor[pos+6:]
		end := strings.Index(rest, `"`)
		if end < 0 {
			break
		}
		href := rest[:end]
		if strings.HasPrefix(href, "http") && fichierIsPlausibleDirectLink(href) {
			return &href
		}
		cursor = rest[end+1:]
	}
	return nil
}

func fichierIsPlausibleDirectLink(href string) bool {
	lower := strings.ToLower(href)
	badHosts := []string{"img.1fichier.com", "twitter.com", "facebook.com", "dstorage.fr"}
	for _, h := range badHosts {
		if strings.Contains(lower, h) {
			return false
		}
	}
	badPaths := []string{"/login", "/register", "/hlp", "/tarifs", "/cgu", "/abus", "/network", "/contact", "/revendeurs", "/api", "/console"}
	for _, p := range badPaths {
		if strings.Contains(lower, p) {
			return false
		}
	}
	badExt := []string{".ico", ".png", ".jpg", ".jpeg", ".gif", ".svg", ".css", ".js", ".html"}
	pathOnly := strings.Split(strings.Split(lower, "?")[0], "#")[0]
	for _, ext := range badExt {
		if strings.HasSuffix(pathOnly, ext) {
			return false
		}
	}
	rest := lower
	if idx := strings.Index(lower, "://"); idx >= 0 {
		rest = lower[idx+3:]
	}
	hasPath := false
	if idx := strings.Index(rest, "/"); idx >= 0 {
		path := strings.Trim(rest[idx:], "/")
		hasPath = path != ""
	}
	return hasPath || strings.Contains(rest, "?")
}

func (p FichierProvider) IsBinaryResponse(resp *http.Response) bool {
	if resp.Header.Get("Content-Disposition") != "" {
		return true
	}
	ct := strings.ToLower(resp.Header.Get("Content-Type"))
	if ct == "" {
		return false
	}
	return !strings.Contains(ct, "text/html")
}

// Resolved link cache helpers — portado de 1fichier.rs usando modernc.org/sqlite via database/sql
func (p FichierProvider) LoadCachedDirectLink(db *sql.DB, pageURL string) *string {
	if db == nil {
		return nil
	}
	var directURL string
	var expiresAt int64
	err := db.QueryRow(`SELECT direct_url, expires_at FROM resolved_download_link_cache WHERE provider_id='fichier' AND source_url=?`, pageURL).Scan(&directURL, &expiresAt)
	if err != nil {
		return nil
	}
	if time.Now().Unix() > expiresAt {
		return nil
	}
	return &directURL
}

func (p FichierProvider) SaveCachedDirectLink(db *sql.DB, pageURL, directURL string) {
	if db == nil {
		return
	}
	now := time.Now().Unix()
	expires := now + fichierCacheTTLSeconds
	_, _ = db.Exec(`INSERT INTO resolved_download_link_cache(provider_id,source_url,direct_url,referer_url,created_at,expires_at,last_used_at) VALUES('fichier',?,?,?,?,?,?) ON CONFLICT(provider_id,source_url) DO UPDATE SET direct_url=excluded.direct_url, referer_url=excluded.referer_url, expires_at=excluded.expires_at, last_used_at=excluded.last_used_at`,
		pageURL, directURL, pageURL, now, expires, now)
}

func (p FichierProvider) TouchCachedDirectLink(db *sql.DB, pageURL string) {
	if db == nil {
		return
	}
	_, _ = db.Exec(`UPDATE resolved_download_link_cache SET last_used_at=? WHERE provider_id='fichier' AND source_url=?`, time.Now().Unix(), pageURL)
}

func (p FichierProvider) RemoveCachedDirectLink(db *sql.DB, pageURL string) {
	if db == nil {
		return
	}
	_, _ = db.Exec(`DELETE FROM resolved_download_link_cache WHERE provider_id='fichier' AND source_url=?`, pageURL)
}

func (p FichierProvider) CachedLinkIsInvalid(errMsg string) bool {
	lower := strings.ToLower(errMsg)
	return strings.Contains(lower, "link temporário não retornou arquivo binário") ||
		strings.Contains(lower, "401 unauthorized") ||
		strings.Contains(lower, "403 forbidden") ||
		strings.Contains(lower, "404 not found") ||
		strings.Contains(lower, "410 gone")
}

func (p FichierProvider) ProbeFileInfo(client *http.Client, pageURL string) (string, uint64, bool, []FichierChildInfo, error) {
	resp, err := client.Get(pageURL)
	if err != nil {
		return "", 0, false, nil, err
	}
	defer resp.Body.Close()
	body, _ := io.ReadAll(io.LimitReader(resp.Body, 2*1024*1024))
	html := string(body)
	if p.HasRestrictedAccessError(html) {
		return "", 0, false, nil, fmt.Errorf("1Fichier bloqueou este IP para download gratuito (VPN/proxy/Tor detectado)")
	}
	if wait := p.ExtractWaitSeconds(html); wait != nil && *wait > 90 {
		return "", 0, false, nil, fmt.Errorf("RATE_LIMIT:%d:1Fichier: aguarde %d minutos", *wait, *wait/60)
	}
	if p.IsFolderPage(pageURL, html) {
		children := p.ExtractFolderChildren(html)
		var folderName string
		if n := p.ExtractFolderName(html); n != nil {
			folderName = *n
		} else if v := fichierExtractBetween(html, "<title>", "</title>"); v != nil {
			folderName = *v
		} else {
			folderName = "pasta_1fichier"
		}
		return SanitizeFilename(folderName, "pasta_1fichier"), 0, true, children, nil
	}
	fallback := "arquivo_1fichier"
	filename, size := p.ExtractFilenameAndSize(html, fallback)
	return SanitizeFilename(filename, fallback), size, false, nil, nil
}
