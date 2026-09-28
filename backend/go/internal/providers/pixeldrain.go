package providers

import (
	"encoding/json"
	"fmt"
	"net/http"
	"strings"
)

// Portado de backend/src/providers/pixeldrain.rs

type PixelDrainProvider struct{}

type PixelDrainTargetType string

const (
	PixelDrainFile PixelDrainTargetType = "file"
	PixelDrainList PixelDrainTargetType = "list"
)

type PixelDrainTarget struct {
	Type          PixelDrainTargetType
	ID            string
	SelectedIndex int
}

func (p PixelDrainProvider) Matches(rawURL string) bool {
	return HostMatches(rawURL, []string{"pixeldrain.com", "www.pixeldrain.com"})
}

func (p PixelDrainProvider) ParseTarget(rawURL string) *PixelDrainTarget {
	if !HostMatches(rawURL, []string{"pixeldrain.com", "www.pixeldrain.com"}) {
		return nil
	}
	segments := PathSegments(rawURL)
	if len(segments) >= 2 {
		first := segments[0]
		id := segments[1]
		if first == "u" && id != "" {
			return &PixelDrainTarget{Type: PixelDrainFile, ID: id}
		}
		if first == "l" && id != "" {
			idx := p.ExtractSelectedIndex(rawURL)
			return &PixelDrainTarget{Type: PixelDrainList, ID: id, SelectedIndex: idx}
		}
	}
	return nil
}

func (p PixelDrainProvider) ExtractSelectedIndex(rawURL string) int {
	u := ParseURL(rawURL)
	if u == nil {
		return 0
	}
	fragment := u.Fragment
	for _, part := range strings.Split(fragment, "&") {
		if strings.HasPrefix(part, "item=") {
			var v int
			if _, err := fmt.Sscanf(part[5:], "%d", &v); err == nil {
				return v
			}
		}
	}
	return 0
}

func (p PixelDrainProvider) FetchFileInfo(client *http.Client, id string) (map[string]interface{}, error) {
	u := fmt.Sprintf("https://pixeldrain.com/api/file/%s/info", id)
	resp, err := client.Get(u)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	if resp.StatusCode >= 400 {
		return nil, fmt.Errorf("pixeldrain file info %d", resp.StatusCode)
	}
	var data map[string]interface{}
	if err := json.NewDecoder(resp.Body).Decode(&data); err != nil {
		return nil, err
	}
	return data, nil
}

func (p PixelDrainProvider) FetchListInfo(client *http.Client, id string) (map[string]interface{}, error) {
	u := fmt.Sprintf("https://pixeldrain.com/api/list/%s", id)
	resp, err := client.Get(u)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	if resp.StatusCode >= 400 {
		return nil, fmt.Errorf("pixeldrain list info %d", resp.StatusCode)
	}
	var data map[string]interface{}
	if err := json.NewDecoder(resp.Body).Decode(&data); err != nil {
		return nil, err
	}
	return data, nil
}
