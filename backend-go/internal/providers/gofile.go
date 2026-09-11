package providers

import (
	"crypto/sha256"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strings"
	"time"
)

// Portado de backend/src/providers/gofile.rs

const gofileUA = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36"
const gofileWTSalt = "5d4f7g8sd45fsd"

type GofileProvider struct{}

type GofileFile struct {
	ID       string  `json:"id"`
	Name     string  `json:"name"`
	Size     uint64  `json:"size"`
	Link     string  `json:"link"`
	MimeType *string `json:"mimeType,omitempty"`
}

func (p GofileProvider) Matches(rawURL string) bool {
	return HostMatches(rawURL, []string{"gofile.io", "www.gofile.io"})
}

func (p GofileProvider) ContentID(rawURL string) *string {
	segs := PathSegments(rawURL)
	if len(segs) >= 2 && segs[0] == "d" && segs[1] != "" {
		return &segs[1]
	}
	return nil
}

func gofileClient() *http.Client {
	return &http.Client{
		Timeout: 30 * time.Second,
	}
}

func (p GofileProvider) CreateToken(client *http.Client) (string, error) {
	if client == nil {
		client = gofileClient()
	}
	// Need to set UA
	req, _ := http.NewRequest("POST", "https://api.gofile.io/accounts", nil)
	req.Header.Set("User-Agent", gofileUA)
	resp, err := client.Do(req)
	if err != nil {
		return "", err
	}
	defer resp.Body.Close()
	if resp.StatusCode >= 400 {
		return "", fmt.Errorf("Gofile: falha ao criar sessão de convidado HTTP %d", resp.StatusCode)
	}
	body, _ := io.ReadAll(resp.Body)
	var data map[string]interface{}
	if err := json.Unmarshal(body, &data); err != nil {
		return "", err
	}
	if token, ok := data["data"].(map[string]interface{}); ok {
		if t, ok := token["token"].(string); ok && t != "" {
			return t, nil
		}
	}
	return "", fmt.Errorf("Gofile: falha ao criar a sessão de convidado")
}

func (p GofileProvider) WebsiteToken(accountToken string) string {
	window := time.Now().Unix() / 14400
	payload := fmt.Sprintf("%s::en-US::%s::%d::%s", gofileUA, accountToken, window, gofileWTSalt)
	hash := sha256.Sum256([]byte(payload))
	return fmt.Sprintf("%x", hash)
}

func (p GofileProvider) FetchContents(client *http.Client, token, contentID string) (map[string]interface{}, error) {
	if client == nil {
		client = gofileClient()
	}
	wt := p.WebsiteToken(token)
	url := fmt.Sprintf("https://api.gofile.io/contents/%s?contentFilter=&page=1&pageSize=1000&sortField=name&sortDirection=1", contentID)
	req, _ := http.NewRequest("GET", url, nil)
	req.Header.Set("Authorization", "Bearer "+token)
	req.Header.Set("X-Website-Token", wt)
	req.Header.Set("X-BL", "en-US")
	req.Header.Set("User-Agent", gofileUA)
	resp, err := client.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	body, _ := io.ReadAll(resp.Body)
	var data map[string]interface{}
	if err := json.Unmarshal(body, &data); err != nil {
		return nil, err
	}
	switch data["status"] {
	case "ok":
		if d, ok := data["data"].(map[string]interface{}); ok {
			return d, nil
		}
		return nil, fmt.Errorf("Gofile: resposta ok sem data")
	case "error-rateLimit":
		return nil, fmt.Errorf("RATE_LIMIT:120:Gofile limitou as requisições. Retry automático agendado.")
	case "error-notPremium":
		return nil, fmt.Errorf("Gofile rejeitou a sessão de convidado (token do site expirado). Tente novamente mais tarde.")
	default:
		if s, ok := data["status"].(string); ok {
			return nil, fmt.Errorf("Gofile respondeu: %s", s)
		}
		return nil, fmt.Errorf("Gofile: resposta inesperada da API")
	}
}

func (p GofileProvider) CollectFiles(data map[string]interface{}) []GofileFile {
	var files []GofileFile
	pushFile := func(node map[string]interface{}) {
		if node["type"] != "file" {
			return
		}
		id, _ := node["id"].(string)
		name, _ := node["name"].(string)
		link, _ := node["link"].(string)
		if id == "" || name == "" || link == "" {
			return
		}
		var size uint64
		switch v := node["size"].(type) {
		case float64:
			size = uint64(v)
		case int:
			size = uint64(v)
		case int64:
			size = uint64(v)
		case json.Number:
			if n, err := v.Int64(); err == nil {
				size = uint64(n)
			}
		}
		var mime *string
		if m, ok := node["mimetype"].(string); ok && m != "" {
			mime = &m
		}
		files = append(files, GofileFile{ID: id, Name: name, Size: size, Link: link, MimeType: mime})
	}
	if children, ok := data["children"].(map[string]interface{}); ok {
		for _, child := range children {
			if m, ok := child.(map[string]interface{}); ok {
				pushFile(m)
			}
		}
	} else if data["type"] == "file" {
		pushFile(data)
	}
	return files
}

func (p GofileProvider) ChildSourceURL(contentID, fileID string) string {
	return fmt.Sprintf("https://gofile.io/d/%s?file=%s", contentID, fileID)
}

func (p GofileProvider) GetFileInfo(client *http.Client, rawURL string) (string, uint64, bool, []GofileFile, error) {
	contentID := p.ContentID(rawURL)
	if contentID == nil {
		return "", 0, false, nil, fmt.Errorf("URL do Gofile inválida: %s", rawURL)
	}
	if client == nil {
		client = gofileClient()
		client.Transport = &gofileUATransport{base: http.DefaultTransport}
	}
	token, err := p.CreateToken(client)
	if err != nil {
		return "", 0, false, nil, err
	}
	data, err := p.FetchContents(client, token, *contentID)
	if err != nil {
		return "", 0, false, nil, err
	}
	files := p.CollectFiles(data)
	if len(files) == 0 {
		return "", 0, false, nil, fmt.Errorf("Gofile: nenhum arquivo encontrado neste link")
	}
	if len(files) == 1 {
		return files[0].Name, files[0].Size, false, files, nil
	}
	// folder
	total := uint64(0)
	for _, f := range files {
		total += f.Size
	}
	folderName := *contentID
	if n, ok := data["name"].(string); ok && n != "" {
		folderName = n
	}
	return SanitizeFilename(folderName, "gofile"), total, true, files, nil
}

// gofileUATransport ensures User-Agent is set
type gofileUATransport struct{ base http.RoundTripper }

func (t *gofileUATransport) RoundTrip(req *http.Request) (*http.Response, error) {
	if req.Header.Get("User-Agent") == "" {
		req.Header.Set("User-Agent", gofileUA)
	}
	base := t.base
	if base == nil {
		base = http.DefaultTransport
	}
	return base.RoundTrip(req)
}

var _ = strings.Contains
