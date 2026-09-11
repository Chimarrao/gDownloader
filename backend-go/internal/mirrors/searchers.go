package mirrors

import (
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"regexp"
	"strings"
	"time"
)

// Portado de backend/src/mirrors/searchers.rs

var (
	urlRe            = regexp.MustCompile(`https?://[^\s"'<>]+`)
	ddgResultRe      = regexp.MustCompile(`uddg=(https?%3A[^&]+)`)
	bingLinkRe       = regexp.MustCompile(`<a[^>]+href="(https?://[^"]+)"`)
	yandexURLRe      = regexp.MustCompile(`"url":"(https?://[^"]+)"`)
	yandexDataURLRe  = regexp.MustCompile(`data-url="(https?://[^"]+)"`)
	googleURLRe      = regexp.MustCompile(`/url\?q=(https?://[^&]+)`)
	mediafireLinkRe  = regexp.MustCompile(`href="(https://www\.mediafire\.com/file/[^"]+)"`)
	filesearchingRe  = regexp.MustCompile(`href="((ftp|https?)://[^"]+)"`)
)

func BuildClient() *http.Client {
	return &http.Client{
		Timeout: 14 * time.Second,
		CheckRedirect: func(req *http.Request, via []*http.Request) error {
			if len(via) >= 5 {
				return http.ErrUseLastResponse
			}
			return nil
		},
	}
}

func q(s string) string {
	return url.QueryEscape(s)
}

func get(client *http.Client, rawURL string, referer *string) *string {
	for attempt := 0; attempt < 2; attempt++ {
		req, _ := http.NewRequest("GET", rawURL, nil)
		req.Header.Set("Accept", "text/html,*/*;q=0.9")
		req.Header.Set("Accept-Language", "pt-BR,pt;q=0.9,en;q=0.8")
		req.Header.Set("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/136 Safari/537.36")
		if referer != nil {
			req.Header.Set("Referer", *referer)
		}
		resp, err := client.Do(req)
		if err != nil {
			if attempt == 0 {
				time.Sleep(time.Second)
			}
			continue
		}
		if resp.StatusCode >= 200 && resp.StatusCode < 300 {
			body, _ := io.ReadAll(io.LimitReader(resp.Body, 4*1024*1024))
			resp.Body.Close()
			s := string(body)
			return &s
		}
		resp.Body.Close()
		if attempt == 0 {
			time.Sleep(time.Second)
		}
	}
	return nil
}

func getJSON(client *http.Client, rawURL string) map[string]interface{} {
	for attempt := 0; attempt < 2; attempt++ {
		req, _ := http.NewRequest("GET", rawURL, nil)
		req.Header.Set("Accept", "application/json")
		resp, err := client.Do(req)
		if err != nil {
			if attempt == 0 {
				time.Sleep(time.Second)
			}
			continue
		}
		if resp.StatusCode >= 200 && resp.StatusCode < 300 {
			var data map[string]interface{}
			if err := json.NewDecoder(resp.Body).Decode(&data); err == nil {
				resp.Body.Close()
				return data
			}
		}
		resp.Body.Close()
		if attempt == 0 {
			time.Sleep(time.Second)
		}
	}
	return nil
}

func BuildQueries(slug string, uploader *string, ext string) []string {
	var queries []string
	words := strings.Fields(slug)
	var titleWords []string
	for _, w := range words {
		if len(w) >= 3 {
			if _, err := parseUint(w); err != nil {
				titleWords = append(titleWords, w)
				if len(titleWords) >= 4 {
					break
				}
			}
		}
	}
	shortTitle := strings.Join(titleWords, " ")
	if uploader != nil {
		queries = append(queries, fmt.Sprintf("\"%s\" \"%s\"", slug, *uploader))
		if shortTitle != "" && !strings.EqualFold(shortTitle, slug) {
			queries = append(queries, fmt.Sprintf("\"%s\" \"%s\"", shortTitle, *uploader))
		}
		if ext != "" {
			queries = append(queries, fmt.Sprintf("\"%s\" \"%s\" filetype:%s", shortTitle, *uploader, ext))
		}
	} else {
		queries = append(queries, fmt.Sprintf("\"%s\"", slug))
		if shortTitle != "" && !strings.EqualFold(shortTitle, slug) {
			queries = append(queries, fmt.Sprintf("\"%s\"", shortTitle))
		}
		if ext != "" {
			queries = append(queries, fmt.Sprintf("\"%s\" filetype:%s", shortTitle, ext))
		}
	}
	return queries
}

func SearchDuckDuckGo(client *http.Client, slug string, uploader *string) []MirrorResult {
	var results []MirrorResult
	queries := BuildQueries(slug, uploader, "")
	limit := 2
	if len(queries) < limit {
		limit = len(queries)
	}
	for _, qs := range queries[:limit] {
		u := fmt.Sprintf("https://html.duckduckgo.com/html/?q=%s", q(qs))
		ref := "https://duckduckgo.com/"
		html := get(client, u, &ref)
		if html == nil {
			continue
		}
		for _, cap := range ddgResultRe.FindAllStringSubmatch(*html, -1) {
			decoded, _ := url.QueryUnescape(cap[1])
			if !strings.Contains(decoded, "duckduckgo") {
				results = append(results, MakeResult("duckduckgo", decoded, slug))
			}
		}
		time.Sleep(900 * time.Millisecond)
	}
	return results
}

func SearchBing(client *http.Client, slug string, uploader *string, ext string) []MirrorResult {
	var results []MirrorResult
	queries := BuildQueries(slug, uploader, ext)
	limit := 2
	if len(queries) < limit {
		limit = len(queries)
	}
	for _, qs := range queries[:limit] {
		u := fmt.Sprintf("https://www.bing.com/search?q=%s&count=30&mkt=pt-BR", q(qs))
		html := get(client, u, nil)
		if html == nil {
			continue
		}
		for _, cap := range bingLinkRe.FindAllStringSubmatch(*html, -1) {
			href := cap[1]
			lower := strings.ToLower(href)
			if strings.Contains(lower, "bing.com") || strings.Contains(lower, "microsoft.com") || strings.Contains(lower, "msn.com") || strings.Contains(lower, "live.com") {
				continue
			}
			h := DetectHoster(href)
			words := strings.Fields(slug)
			if len(words) > 3 {
				words = words[:3]
			}
			matched := false
			if h != nil {
				matched = true
			} else {
				for _, w := range words {
					if strings.Contains(lower, strings.ToLower(w)) {
						matched = true
						break
					}
				}
			}
			if matched {
				results = append(results, MakeResult("bing", href, slug))
			}
		}
		time.Sleep(900 * time.Millisecond)
	}
	return results
}

func SearchYandex(client *http.Client, slug string, uploader *string) []MirrorResult {
	var results []MirrorResult
	queries := BuildQueries(slug, uploader, "")
	limit := 2
	if len(queries) < limit {
		limit = len(queries)
	}
	for _, qs := range queries[:limit] {
		u := fmt.Sprintf("https://yandex.com/search/?text=%s&lr=10393", q(qs))
		html := get(client, u, nil)
		if html == nil {
			continue
		}
		for _, re := range []*regexp.Regexp{yandexURLRe, yandexDataURLRe} {
			for _, cap := range re.FindAllStringSubmatch(*html, -1) {
				href := cap[1]
				lower := strings.ToLower(href)
				if strings.Contains(lower, "yandex.") {
					continue
				}
				if DetectHoster(href) != nil || func() bool {
					for _, w := range strings.Fields(slug)[:min(2, len(strings.Fields(slug)))] {
						if strings.Contains(lower, strings.ToLower(w)) {
							return true
						}
					}
					return false
				}() {
					results = append(results, MakeResult("yandex", href, slug))
				}
			}
		}
		time.Sleep(900 * time.Millisecond)
	}
	return results
}

func min(a, b int) int {
	if a < b {
		return a
	}
	return b
}

func googleDork(client *http.Client, dork, tag string) []MirrorResult {
	var results []MirrorResult
	u := fmt.Sprintf("https://www.google.com/search?q=%s&num=20&hl=pt-BR", q(dork))
	ref := "https://www.google.com/"
	html := get(client, u, &ref)
	if html == nil {
		return results
	}
	for _, cap := range googleURLRe.FindAllStringSubmatch(*html, -1) {
		decoded, _ := url.QueryUnescape(cap[1])
		if !strings.Contains(decoded, "google") {
			results = append(results, MakeResult(tag, decoded, dork))
		}
	}
	time.Sleep(2200 * time.Millisecond)
	return results
}

func SearchGoogleOpendir(client *http.Client, slug, ext string) []MirrorResult {
	var results []MirrorResult
	dorks := []string{
		fmt.Sprintf("intitle:\"index of\" \"%s\"", slug),
		fmt.Sprintf("\"parent directory\" \"%s\"", slug),
	}
	if ext != "" {
		dorks = append(dorks, fmt.Sprintf("intitle:\"index of\" \"%s\" .%s", slug, ext))
	} else {
		dorks = append(dorks, fmt.Sprintf("intitle:\"index of\" \"%s\"", slug))
	}
	for _, dork := range dorks {
		results = append(results, googleDork(client, dork, "google-opendir")...)
	}
	return results
}

func SearchGoogleHosters(client *http.Client, slug string, uploader *string) []MirrorResult {
	var results []MirrorResult
	hosts := Hosters
	if len(hosts) > 14 {
		hosts = hosts[:14]
	}
	var parts []string
	for _, h := range hosts {
		parts = append(parts, "site:"+h)
	}
	hosterQ := strings.Join(parts, " OR ")
	var titleShort string
	if uploader != nil {
		words := strings.Fields(slug)
		if len(words) > 3 {
			words = words[:3]
		}
		titleShort = fmt.Sprintf("\"%s\" \"%s\"", strings.Join(words, " "), *uploader)
	} else {
		titleShort = fmt.Sprintf("\"%s\"", slug)
	}
	for _, dork := range []string{
		fmt.Sprintf("%s (%s)", titleShort, hosterQ),
		fmt.Sprintf("\"%s\" download link", slug),
	} {
		results = append(results, googleDork(client, dork, "google-hosters")...)
	}
	return results
}

func SearchPixeldrain(client *http.Client, slug string) []MirrorResult {
	var results []MirrorResult
	u := fmt.Sprintf("https://pixeldrain.com/api/search?q=%s&count=20", q(slug))
	data := getJSON(client, u)
	if data == nil {
		return results
	}
	for _, key := range []string{"files", "lists"} {
		arr, ok := data[key].([]interface{})
		if !ok {
			continue
		}
		for _, item := range arr {
			m, ok := item.(map[string]interface{})
			if !ok {
				continue
			}
			name, _ := m["name"].(string)
			if name == "" {
				name, _ = m["title"].(string)
			}
			words := strings.Fields(slug)
			if len(words) > 3 {
				words = words[:3]
			}
			matched := false
			for _, w := range words {
				if strings.Contains(strings.ToLower(name), strings.ToLower(w)) {
					matched = true
					break
				}
			}
			if !matched {
				continue
			}
			if fid, ok := m["id"].(string); ok {
				link := fmt.Sprintf("https://pixeldrain.com/u/%s", fid)
				results = append(results, MakeResult("pixeldrain.com", link, name))
			}
		}
	}
	return results
}

func SearchGofile(client *http.Client, slug string) []MirrorResult {
	var results []MirrorResult
	u := fmt.Sprintf("https://api.gofile.io/search?query=%s", q(slug))
	data := getJSON(client, u)
	if data == nil {
		return results
	}
	d, _ := data["data"].(map[string]interface{})
	items, _ := d["items"].([]interface{})
	for _, item := range items {
		m, _ := item.(map[string]interface{})
		name, _ := m["name"].(string)
		link, _ := m["link"].(string)
		words := strings.Fields(slug)
		if len(words) > 3 {
			words = words[:3]
		}
		matched := false
		for _, w := range words {
			if strings.Contains(strings.ToLower(name), strings.ToLower(w)) {
				matched = true
				break
			}
		}
		if link != "" && matched {
			results = append(results, MakeResult("gofile.io", link, name))
		}
	}
	return results
}

func SearchArchiveOrg(client *http.Client, slug string) []MirrorResult {
	var results []MirrorResult
	u := fmt.Sprintf("https://archive.org/advancedsearch.php?q=%s&fl[]=identifier&fl[]=title&rows=10&output=json", q(slug))
	data := getJSON(client, u)
	if data == nil {
		return results
	}
	resp, _ := data["response"].(map[string]interface{})
	docs, _ := resp["docs"].([]interface{})
	words := strings.Fields(slug)
	if len(words) > 3 {
		words = words[:3]
	}
	for _, doc := range docs {
		m, _ := doc.(map[string]interface{})
		id, _ := m["identifier"].(string)
		title, _ := m["title"].(string)
		matched := false
		for _, w := range words {
			if strings.Contains(strings.ToLower(id), strings.ToLower(w)) || strings.Contains(strings.ToLower(title), strings.ToLower(w)) {
				matched = true
				break
			}
		}
		if matched && id != "" {
			link := fmt.Sprintf("https://archive.org/download/%s", id)
			results = append(results, MakeResult("archive.org", link, title))
		}
	}
	return results
}

func SearchMediafire(client *http.Client, slug string) []MirrorResult {
	var results []MirrorResult
	u := fmt.Sprintf("https://www.mediafire.com/search/?q=%s&type=file", q(slug))
	ref := "https://www.mediafire.com/"
	html := get(client, u, &ref)
	if html == nil {
		return results
	}
	for _, cap := range mediafireLinkRe.FindAllStringSubmatch(*html, -1) {
		results = append(results, MakeResult("mediafire.com", cap[1], slug))
	}
	return results
}

func dorkHoster(client *http.Client, slug string, uploader *string, site, source string) []MirrorResult {
	var results []MirrorResult
	var title string
	if uploader != nil {
		words := strings.Fields(slug)
		if len(words) > 3 {
			words = words[:3]
		}
		title = fmt.Sprintf("\"%s\" \"%s\"", strings.Join(words, " "), *uploader)
	} else {
		title = fmt.Sprintf("\"%s\"", slug)
	}
	dork := fmt.Sprintf("site:%s %s", site, title)
	u := fmt.Sprintf("https://html.duckduckgo.com/html/?q=%s", q(dork))
	ref := "https://duckduckgo.com/"
	if html := get(client, u, &ref); html != nil {
		re := regexp.MustCompile(`uddg=(https?%3A[^&]+)`)
		for _, cap := range re.FindAllStringSubmatch(*html, -1) {
			decoded, _ := url.QueryUnescape(cap[1])
			if strings.Contains(decoded, site) {
				results = append(results, MakeResult(source, decoded, slug))
			}
		}
	}
	results = append(results, googleDork(client, dork, source)...)
	return results
}

func Search1Fichier(client *http.Client, slug string, uploader *string) []MirrorResult {
	return dorkHoster(client, slug, uploader, "1fichier.com", "1fichier.com")
}
func SearchRapidgator(client *http.Client, slug string, uploader *string) []MirrorResult {
	return dorkHoster(client, slug, uploader, "rapidgator.net", "rapidgator.net")
}
func SearchMega(client *http.Client, slug string, uploader *string) []MirrorResult {
	return dorkHoster(client, slug, uploader, "mega.nz", "mega.nz")
}

func SearchFilesearching(client *http.Client, slug string) []MirrorResult {
	var results []MirrorResult
	u := fmt.Sprintf("https://filesearching.com/search.php?q=%s", q(slug))
	html := get(client, u, nil)
	if html == nil {
		return results
	}
	words := strings.Fields(slug)
	if len(words) > 3 {
		words = words[:3]
	}
	for _, cap := range filesearchingRe.FindAllStringSubmatch(*html, -1) {
		href := cap[1]
		lower := strings.ToLower(href)
		if strings.Contains(lower, "filesearching.com") {
			continue
		}
		for _, w := range words {
			if strings.Contains(lower, strings.ToLower(w)) {
				results = append(results, MakeResult("filesearching.com", href, slug))
				break
			}
		}
	}
	return results
}
