package providers

import (
	"encoding/json"
	"fmt"
	"net/http"
	"regexp"
	"strings"
)

// Portado de backend/src/providers/mediafire.rs

type MediaFireProvider struct{}

func (p MediaFireProvider) Matches(rawURL string) bool {
	return HostMatches(rawURL, []string{"mediafire.com", "www.mediafire.com"})
}

func (p MediaFireProvider) IsFolderURL(rawURL string) bool {
	return strings.Contains(rawURL, "/folder/")
}

func (p MediaFireProvider) ExtractFolderKey(rawURL string) *string {
	pos := strings.Index(rawURL, "/folder/")
	if pos < 0 {
		return nil
	}
	after := rawURL[pos+8:]
	parts := strings.Split(after, "/")
	if len(parts) == 0 {
		return nil
	}
	key := strings.Split(parts[0], "?")[0]
	key = strings.Split(key, "#")[0]
	key = strings.TrimSpace(key)
	if key == "" {
		return nil
	}
	return &key
}

func (p MediaFireProvider) ExtractSelectedSubfolderKey(rawURL string) *string {
	parts := strings.Split(rawURL, "#")
	if len(parts) < 2 {
		return nil
	}
	fragment := strings.TrimSpace(parts[1])
	if fragment == "" {
		return nil
	}
	return &fragment
}

func (p MediaFireProvider) ExtractFilenameFromURL(rawURL string) *string {
	pos := strings.Index(rawURL, "/file/")
	if pos < 0 {
		return nil
	}
	after := rawURL[pos+6:]
	segments := strings.Split(after, "/")
	var filtered []string
	for _, s := range segments {
		if s != "" {
			filtered = append(filtered, s)
		}
	}
	if len(filtered) < 2 {
		return nil
	}
	filename := strings.TrimSpace(filtered[1])
	if filename == "" {
		return nil
	}
	return &filename
}

var downloadButtonRe = regexp.MustCompile(`id="downloadButton"[^>]*href="([^"]+)"`)
var anchorRe = regexp.MustCompile(`<a[^>]+href="([^"]+)"`)

func (p MediaFireProvider) ExtractDirectLink(html string) *string {
	if m := downloadButtonRe.FindStringSubmatch(html); len(m) > 1 {
		if strings.HasPrefix(m[1], "http") {
			return &m[1]
		}
	}
	matches := anchorRe.FindAllStringSubmatch(html, -1)
	for _, m := range matches {
		href := m[1]
		if !strings.HasPrefix(href, "http") {
			continue
		}
		u := ParseURL(href)
		if u == nil {
			continue
		}
		host := u.Hostname()
		isMF := host == "mediafire.com" || strings.HasSuffix(host, ".mediafire.com")
		if isMF && (strings.Contains(href, "/get/") || strings.Contains(href, "/download/")) {
			return &href
		}
	}
	return nil
}

func (p MediaFireProvider) ResolveDirectDownloadURL(client *http.Client, pageURL string) (string, error) {
	resp, err := client.Get(pageURL)
	if err != nil {
		return "", err
	}
	defer resp.Body.Close()
	buf := make([]byte, 2*1024*1024)
	n, _ := resp.Body.Read(buf)
	html := string(buf[:n])
	if link := p.ExtractDirectLink(html); link != nil {
		return *link, nil
	}
	return "", fmt.Errorf("Link de download não encontrado na página do MediaFire")
}

func (p MediaFireProvider) FetchFolderInfo(client *http.Client, folderKey string) (map[string]interface{}, error) {
	u := fmt.Sprintf("https://www.mediafire.com/api/1.4/folder/get_info.php?folder_key=%s&response_format=json", folderKey)
	resp, err := client.Get(u)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	var data map[string]interface{}
	if err := json.NewDecoder(resp.Body).Decode(&data); err != nil {
		return nil, err
	}
	return data, nil
}

func (p MediaFireProvider) FetchFolderFiles(client *http.Client, folderKey string) ([]map[string]interface{}, error) {
	var files []map[string]interface{}
	chunk := 1
	for {
		u := fmt.Sprintf("https://www.mediafire.com/api/1.4/folder/get_content.php?folder_key=%s&content_type=files&response_format=json&chunk=%d", folderKey, chunk)
		resp, err := client.Get(u)
		if err != nil {
			return nil, err
		}
		var data map[string]interface{}
		if err := json.NewDecoder(resp.Body).Decode(&data); err != nil {
			resp.Body.Close()
			return nil, err
		}
		resp.Body.Close()
		respMap, _ := data["response"].(map[string]interface{})
		folderContent, _ := respMap["folder_content"].(map[string]interface{})
		if arr, ok := folderContent["files"].([]interface{}); ok {
			for _, f := range arr {
				if m, ok := f.(map[string]interface{}); ok {
					files = append(files, m)
				}
			}
		}
		more, _ := folderContent["more_chunks"].(string)
		if more != "yes" {
			break
		}
		chunk++
	}
	return files, nil
}

func (p MediaFireProvider) ResolveEffectiveFolderKey(client *http.Client, rawURL string) (string, error) {
	rootKey := p.ExtractFolderKey(rawURL)
	if rootKey == nil {
		return "", fmt.Errorf("URL de pasta do MediaFire inválida: %s", rawURL)
	}
	selectedKey := p.ExtractSelectedSubfolderKey(rawURL)
	if selectedKey == nil {
		return *rootKey, nil
	}
	// Verify if selectedKey is direct child of root
	u := fmt.Sprintf("https://www.mediafire.com/api/1.4/folder/get_content.php?folder_key=%s&content_type=folders&response_format=json", *rootKey)
	resp, err := client.Get(u)
	if err != nil {
		return *rootKey, nil
	}
	defer resp.Body.Close()
	var data map[string]interface{}
	if err := json.NewDecoder(resp.Body).Decode(&data); err != nil {
		return *rootKey, nil
	}
	respMap, _ := data["response"].(map[string]interface{})
	folderContent, _ := respMap["folder_content"].(map[string]interface{})
	if folders, ok := folderContent["folders"].([]interface{}); ok {
		for _, f := range folders {
			if m, ok := f.(map[string]interface{}); ok {
				if fk, ok := m["folderkey"].(string); ok && fk == *selectedKey {
					return *selectedKey, nil
				}
			}
		}
	}
	return *rootKey, nil
}
