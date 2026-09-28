package providers

import (
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strings"
)

// Portado de backend/src/providers/drime.rs — Drime cloud share API.

type DrimeProvider struct{}

type DrimeFileInfo struct {
	Filename string  `json:"filename"`
	Size     uint64  `json:"size"`
	MimeType *string `json:"mimeType,omitempty"`
	SourceURL *string `json:"sourceUrl,omitempty"`
}

func (p DrimeProvider) Matches(rawURL string) bool {
	if !HostMatches(rawURL, []string{"app.drime.cloud"}) {
		return false
	}
	segs := PathSegments(rawURL)
	if len(segs) < 3 {
		return false
	}
	return segs[0] == "drive" && segs[1] == "s"
}

func (p DrimeProvider) ExtractShareHash(rawURL string) *string {
	pos := strings.Index(rawURL, "/drive/s/")
	if pos < 0 {
		return nil
	}
	after := rawURL[pos+9:]
	hash := strings.Split(strings.Split(strings.Split(after, "?")[0], "#")[0], "/")[0]
	hash = strings.TrimSpace(hash)
	if hash == "" {
		return nil
	}
	return &hash
}

func (p DrimeProvider) FetchSharePage(client *http.Client, shareHash string, page int) (map[string]interface{}, error) {
	url := fmt.Sprintf("https://app.drime.cloud/api/v1/shareable-links/%s?withEntries=true&page=%d&order=updated_at:desc", shareHash, page)
	req, _ := http.NewRequest("GET", url, nil)
	req.Header.Set("Accept", "application/json")
	req.Header.Set("X-Requested-With", "XMLHttpRequest")
	resp, err := client.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	if resp.StatusCode >= 400 {
		return nil, fmt.Errorf("Drime HTTP %d", resp.StatusCode)
	}
	var data map[string]interface{}
	if err := json.NewDecoder(resp.Body).Decode(&data); err != nil {
		return nil, err
	}
	return data, nil
}

func (p DrimeProvider) FetchAllFolderChildren(client *http.Client, shareHash string) (map[string]interface{}, []map[string]interface{}, error) {
	firstPage, err := p.FetchSharePage(client, shareHash, 1)
	if err != nil {
		return nil, nil, err
	}
	var lastPage int = 1
	if fc, ok := firstPage["folderChildren"].(map[string]interface{}); ok {
		if lp, ok := fc["last_page"].(float64); ok {
			lastPage = int(lp)
		}
	}
	var children []map[string]interface{}
	if fc, ok := firstPage["folderChildren"].(map[string]interface{}); ok {
		if arr, ok := fc["data"].([]interface{}); ok {
			for _, item := range arr {
				if m, ok := item.(map[string]interface{}); ok {
					children = append(children, m)
				}
			}
		}
	}
	for page := 2; page <= lastPage; page++ {
		j, err := p.FetchSharePage(client, shareHash, page)
		if err != nil {
			continue
		}
		if fc, ok := j["folderChildren"].(map[string]interface{}); ok {
			if arr, ok := fc["data"].([]interface{}); ok {
				for _, item := range arr {
					if m, ok := item.(map[string]interface{}); ok {
						children = append(children, m)
					}
				}
			}
		}
	}
	return firstPage, children, nil
}

func drimeChildToInfo(child map[string]interface{}) DrimeFileInfo {
	name, _ := child["name"].(string)
	if name == "" {
		name = "arquivo_drime"
	}
	var size uint64
	switch v := child["file_size"].(type) {
	case float64:
		size = uint64(v)
	case int:
		size = uint64(v)
	}
	var mime *string
	if m, ok := child["mime"].(string); ok && m != "" {
		mime = &m
	}
	var sourceURL *string
	if id, ok := child["id"].(float64); ok {
		u := fmt.Sprintf("https://app.drime.cloud/api/v1/file-entries/%d", int64(id))
		sourceURL = &u
	}
	return DrimeFileInfo{Filename: name, Size: size, MimeType: mime, SourceURL: sourceURL}
}

func (p DrimeProvider) GetFileInfo(client *http.Client, rawURL string) (string, uint64, bool, []DrimeFileInfo, error) {
	shareHash := p.ExtractShareHash(rawURL)
	if shareHash == nil {
		return "", 0, false, nil, fmt.Errorf("URL do Drime inválida: %s", rawURL)
	}
	page, children, err := p.FetchAllFolderChildren(client, *shareHash)
	if err != nil {
		return "", 0, false, nil, err
	}
	link, _ := page["link"].(map[string]interface{})
	entry, _ := link["entry"].(map[string]interface{})
	entryType, _ := entry["type"].(string)
	if entryType == "folder" {
		var mapped []DrimeFileInfo
		for _, child := range children {
			if t, _ := child["type"].(string); t == "folder" {
				continue
			}
			mapped = append(mapped, drimeChildToInfo(child))
		}
		total := uint64(0)
		for _, c := range mapped {
			total += c.Size
		}
		name, _ := entry["name"].(string)
		if name == "" {
			name = "pasta_drime"
		}
		return SanitizeFilename(name, "pasta_drime"), total, true, mapped, nil
	}
	// single file
	name, _ := entry["name"].(string)
	if name == "" {
		name = "arquivo_drime"
	}
	var size uint64
	switch v := entry["file_size"].(type) {
	case float64:
		size = uint64(v)
	}
	var mime *string
	if m, ok := entry["mime"].(string); ok {
		mime = &m
	}
	_ = mime
	return SanitizeFilename(name, "arquivo_drime"), size, false, nil, nil
}

func (p DrimeProvider) OpenDownloadResponse(client *http.Client, entryID, shareID int64, existingBytes uint64) (*http.Response, bool, error) {
	endpoint := fmt.Sprintf("https://app.drime.cloud/api/v1/file-entries/%d?shareable_link=%d", entryID, shareID)
	req, _ := http.NewRequest("GET", endpoint, nil)
	if existingBytes > 0 {
		req.Header.Set("Range", fmt.Sprintf("bytes=%d-", existingBytes))
	}
	resp, err := client.Do(req)
	if err != nil {
		return nil, false, err
	}
	if existingBytes > 0 && resp.StatusCode == 206 {
		return resp, true, nil
	}
	if existingBytes > 0 && (resp.StatusCode == 400 || resp.StatusCode == 416) {
		resp.Body.Close()
		req2, _ := http.NewRequest("GET", endpoint, nil)
		resp2, err := client.Do(req2)
		if err != nil {
			return nil, false, err
		}
		if resp2.StatusCode >= 400 {
			return nil, false, fmt.Errorf("Drime HTTP %d", resp2.StatusCode)
		}
		return resp2, false, nil
	}
	if resp.StatusCode >= 400 {
		return nil, false, fmt.Errorf("Drime HTTP %d", resp.StatusCode)
	}
	return resp, false, nil
}

// Ensure imports used
var _ = io.ReadAll
var _ = json.Marshal
