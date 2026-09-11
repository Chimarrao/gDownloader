package providers

import (
	"database/sql"
	"encoding/json"
	"fmt"
	"net/http"
	"net/url"
	"strings"

	"gdownloader-go/internal/db"
	"gdownloader-go/internal/models"
)

type urlQuery struct {
	URL string `json:"url"`
}

func writeJSON(w http.ResponseWriter, status int, v interface{}) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(v)
}

func writeError(w http.ResponseWriter, status int, msg string) {
	writeJSON(w, status, map[string]string{"error": msg})
}

// Provider detection helpers — portado de providers/mod.rs detect_provider
// Simple host-based detection to mirror Rust priority.

func detectProviderName(rawURL string) string {
	u := ParseURL(rawURL)
	if u == nil {
		return ""
	}
	host := strings.ToLower(u.Hostname())
	// Map host to provider name similar to Rust order
	checks := []struct {
		hosts []string
		name  string
	}{
		{[]string{"mega.nz", "mega.co.nz"}, "Mega"},
		{[]string{"mediafire.com", "www.mediafire.com"}, "MediaFire"},
		{[]string{"app.drime.cloud"}, "Drime"},
		{[]string{"1fichier.com"}, "1Fichier"},
		{[]string{"terabox.com", "www.terabox.com", "1024tera.com", "1024terabox.com"}, "Terabox"},
		{[]string{"transfer.it", "www.transfer.it"}, "Transfer.it"},
		{[]string{"send.now", "www.send.now"}, "Send.now"},
		{[]string{"archive.org", "www.archive.org"}, "Internet Archive"},
		{[]string{"youtube.com", "www.youtube.com", "youtu.be", "m.youtube.com"}, "YouTube"},
		{[]string{"sharepoint.com"}, "OneDrive"},
		{[]string{"drive.google.com"}, "Google Drive"},
		{[]string{"gofile.io", "www.gofile.io"}, "Gofile"},
		{[]string{"pixeldrain.com", "www.pixeldrain.com"}, "PixelDrain"},
		{[]string{"rapidgator.net", "www.rapidgator.net"}, "Rapidgator"},
		{[]string{"brupload.net", "www.brupload.net"}, "BRUpload"},
		{[]string{"brfiles.com", "www.brfiles.com"}, "BRFiles"},
		{[]string{"moondl.com", "www.moondl.com"}, "MoonDL"},
		{[]string{"akirabox.to", "www.akirabox.to"}, "AkiraBox"},
		{[]string{"katfile.com", "katfile.ws"}, "Katfile"},
		{[]string{"onedrive.live.com", "1drv.ms"}, "OneDrive"},
	}
	for _, c := range checks {
		for _, h := range c.hosts {
			if host == h || strings.HasSuffix(host, "."+h) {
				return c.name
			}
		}
	}
	if strings.HasPrefix(rawURL, "http://") || strings.HasPrefix(rawURL, "https://") {
		return "Direct HTTP"
	}
	return ""
}

func accountStateForProvider(providerName string, secure models.SecureSettings) *ProviderAccountState {
	if providerName == "Terabox" && secure.TeraboxAccount != nil {
		acc := secure.TeraboxAccount
		connected := len(acc.Cookies) > 0 || acc.VerifiedAt != nil
		return &ProviderAccountState{Connected: connected, VerifiedAt: acc.VerifiedAt}
	}
	return nil
}

// Additional types needed for descriptor enrichment — mirrors Rust provider_descriptor
type ProviderAccountState struct {
	Connected  bool    `json:"connected"`
	VerifiedAt *string `json:"verifiedAt,omitempty"`
}

func descriptorByName(name string) ProviderDescriptor {
	descs := AllProviderDescriptors()
	for _, d := range descs {
		if d.Name == name {
			return d
		}
	}
	return ProviderDescriptor{ID: "unknown", Name: "Unknown", Color: "#64748b"}
}

// ListProviders GET /providers
func ListProviders(w http.ResponseWriter, r *http.Request, database *sql.DB) {
	secure, _ := db.LoadSecureSettings(database)
	descs := AllProviderDescriptors()
	type enriched struct {
		ID           string                `json:"id"`
		Name         string                `json:"name"`
		Icon         string                `json:"icon"`
		Color        string                `json:"color"`
		Capabilities ProviderCapabilities  `json:"capabilities"`
		AccountState *ProviderAccountState `json:"accountState,omitempty"`
	}
	var out []enriched
	for _, d := range descs {
		// Normalize ID using providerIDFromName
		id := providerIDFromName(d.Name)
		acc := accountStateForProvider(d.Name, secure)
		out = append(out, enriched{
			ID: id, Name: d.Name, Icon: id, Color: d.Color, Capabilities: d.Capabilities, AccountState: acc,
		})
	}
	writeJSON(w, http.StatusOK, out)
}

// DetectProvider GET /detect?url=...
func DetectProvider(w http.ResponseWriter, r *http.Request, database *sql.DB) {
	rawURL := r.URL.Query().Get("url")
	if strings.TrimSpace(rawURL) == "" {
		writeError(w, http.StatusBadRequest, "url obrigatório")
		return
	}
	name := detectProviderName(rawURL)
	if name == "" {
		writeError(w, http.StatusBadRequest, "URL não reconhecida por nenhum provider suportado")
		return
	}
	secure, _ := db.LoadSecureSettings(database)
	desc := descriptorByName(name)
	// Enrich account state
	acc := accountStateForProvider(desc.Name, secure)
	// Ensure ID uses providerIDFromName mapping
	id := providerIDFromName(desc.Name)
	writeJSON(w, http.StatusOK, map[string]interface{}{
		"id":           id,
		"name":         desc.Name,
		"icon":         id,
		"color":        desc.Color,
		"capabilities": desc.Capabilities,
		"accountState": acc,
	})
}

// GetFileInfo GET /file-info?url=...
func GetFileInfo(w http.ResponseWriter, r *http.Request, database *sql.DB) {
	rawURL := r.URL.Query().Get("url")
	if strings.TrimSpace(rawURL) == "" {
		writeError(w, http.StatusBadRequest, "url obrigatório")
		return
	}
	name := detectProviderName(rawURL)
	if name == "" {
		writeError(w, http.StatusBadRequest, "URL não reconhecida")
		return
	}
	// Try to load cached channel thumbnail to pass as context (as Rust does)
	var cachedThumb *string
	if cached, err := db.LoadCachedFileInfo(database, rawURL); err == nil && cached != nil {
		cachedThumb = cached.ChannelThumbnailURL
	}
	_ = cachedThumb // for future use with youtube provider

	// Attempt provider-specific fetch where available, otherwise generic HEAD
	var info models.FileInfo
	var err error
	switch name {
	case "Gofile":
		info, err = fetchGofileInfo(rawURL)
	case "PixelDrain":
		info, err = fetchPixelDrainInfo(rawURL)
	case "MediaFire":
		info, err = fetchMediaFireInfo(rawURL)
	case "Google Drive":
		info, err = fetchGDriveInfo(rawURL)
	default:
		info, err = fetchDirectHTTPInfo(rawURL)
	}
	if err != nil {
		// Prettify similar to Rust downloads::prettify_download_error
		msg := err.Error()
		// Keep as is; Rust would map REMOVED etc but we keep simple
		writeError(w, http.StatusBadRequest, msg)
		return
	}
	// Save to cache
	providerID := providerIDFromName(name)
	_ = db.SaveCachedFileInfo(database, rawURL, providerID, info.Filename, info.Size, info.DurationSecs, info.MimeType, info.IsFolder, info.Children, info.ThumbnailURL, info.ChannelName, info.ChannelThumbnailURL)
	writeJSON(w, http.StatusOK, map[string]interface{}{
		"name":                info.Filename,
		"size":                info.Size,
		"durationSecs":        info.DurationSecs,
		"mimeType":            info.MimeType,
		"isFolder":            info.IsFolder,
		"children":            info.Children,
		"thumbnailUrl":        info.ThumbnailURL,
		"channelName":         info.ChannelName,
		"channelThumbnailUrl": info.ChannelThumbnailURL,
	})
}

// GetCachedFileInfo GET /file-info/cache?url=...
func GetCachedFileInfo(w http.ResponseWriter, r *http.Request, database *sql.DB) {
	rawURL := r.URL.Query().Get("url")
	if strings.TrimSpace(rawURL) == "" {
		writeError(w, http.StatusBadRequest, "url obrigatório")
		return
	}
	cached, err := db.LoadCachedFileInfo(database, rawURL)
	if err != nil {
		writeError(w, http.StatusInternalServerError, "Falha ao consultar o cache local: "+err.Error())
		return
	}
	if cached == nil {
		writeError(w, http.StatusNotFound, "Nenhum cache local encontrado para este link")
		return
	}
	// Map to Rust CachedFileInfo shape which is same as models.CachedFileInfo but with camelCase JSON from db.rs? Our model already has json tags fix.
	// Return same as Rust: direct struct
	writeJSON(w, http.StatusOK, cached)
}

// FileInfoCacheStats GET /file-info/cache/stats
func FileInfoCacheStats(w http.ResponseWriter, r *http.Request, database *sql.DB) {
	entries, bytes, err := db.FileInfoCacheStats(database)
	if err != nil {
		writeError(w, http.StatusInternalServerError, "Falha ao medir o cache local: "+err.Error())
		return
	}
	writeJSON(w, http.StatusOK, map[string]interface{}{"entries": entries, "bytes": bytes})
}

// ClearFileInfoCache DELETE /file-info/cache
func ClearFileInfoCache(w http.ResponseWriter, r *http.Request, database *sql.DB) {
	removed, err := db.ClearFileInfoCache(database)
	if err != nil {
		writeError(w, http.StatusInternalServerError, "Falha ao limpar o cache local: "+err.Error())
		return
	}
	writeJSON(w, http.StatusOK, map[string]interface{}{"removed": removed})
}

// --- FileInfo fetch helpers ---

func fetchDirectHTTPInfo(rawURL string) (models.FileInfo, error) {
	client := &http.Client{}
	// Try HEAD first
	req, err := http.NewRequest("HEAD", rawURL, nil)
	if err != nil {
		return models.FileInfo{}, err
	}
	req.Header.Set("User-Agent", DefaultUserAgent)
	resp, err := client.Do(req)
	if err != nil || (resp != nil && resp.StatusCode >= 400 && resp.StatusCode != http.StatusPartialContent) {
		if resp != nil {
			_ = resp.Body.Close()
		}
		// fallback to GET with range 0-0
		req2, _ := http.NewRequest("GET", rawURL, nil)
		req2.Header.Set("User-Agent", DefaultUserAgent)
		req2.Header.Set("Range", "bytes=0-0")
		resp2, err2 := client.Do(req2)
		if err2 != nil {
			return models.FileInfo{}, err2
		}
		resp = resp2
		if resp.StatusCode >= 400 && resp.StatusCode != http.StatusPartialContent {
			// Try plain GET without range as last resort (some servers ignore Range)
			_ = resp.Body.Close()
			req3, _ := http.NewRequest("GET", rawURL, nil)
			req3.Header.Set("User-Agent", DefaultUserAgent)
			resp3, err3 := client.Do(req3)
			if err3 != nil {
				return models.FileInfo{}, err3
			}
			resp = resp3
		}
	}
	if resp == nil {
		return models.FileInfo{}, fmt.Errorf("sem resposta")
	}
	defer resp.Body.Close()
	if resp.StatusCode >= 400 && resp.StatusCode != http.StatusPartialContent {
		return models.FileInfo{}, fmt.Errorf("HTTP %d", resp.StatusCode)
	}
	size := uint64(0)
	if cl := resp.Header.Get("Content-Length"); cl != "" {
		fmt.Sscanf(cl, "%d", &size)
	}
	if cr := resp.Header.Get("Content-Range"); cr != "" {
		// parse "bytes 0-0/12345"
		parts := strings.Split(cr, "/")
		if len(parts) == 2 {
			fmt.Sscanf(parts[1], "%d", &size)
		}
	}
	// Filename from Content-Disposition or URL
	filename := handlerFilenameFromContentDisposition(resp.Header.Get("Content-Disposition"))
	if filename == "" {
		filename = handlerFilenameFromURL(rawURL)
	}
	// Fallback if disposition filename contains path
	filename = SanitizeFilename(filename, "arquivo")
	var mime *string
	if ct := resp.Header.Get("Content-Type"); ct != "" {
		mime = &ct
	}
	return models.FileInfo{
		Filename: filename,
		Size:     size,
		MimeType: mime,
		IsFolder: false,
	}, nil
}

func handlerFilenameFromContentDisposition(v string) string {
	for _, part := range strings.Split(v, ";") {
		part = strings.TrimSpace(part)
		lower := strings.ToLower(part)
		if strings.HasPrefix(lower, "filename=") {
			if idx := strings.Index(part, "="); idx >= 0 {
				raw := strings.Trim(strings.TrimSpace(part[idx+1:]), "\"")
				if raw != "" {
					if decoded, err := url.QueryUnescape(raw); err == nil {
						raw = decoded
					}
					return raw
				}
			}
		}
		if strings.HasPrefix(lower, "filename*=") {
			if idx := strings.Index(part, "="); idx >= 0 {
				raw := strings.Trim(strings.TrimSpace(part[idx+1:]), "\"")
				if strings.HasPrefix(raw, "UTF-8''") {
					raw = raw[7:]
				}
				if decoded, err := url.QueryUnescape(raw); err == nil {
					return decoded
				}
				return raw
			}
		}
	}
	return ""
}

func handlerFilenameFromURL(raw string) string {
	parsed, err := url.Parse(raw)
	if err != nil {
		return "arquivo"
	}
	segments := strings.Split(strings.Trim(parsed.Path, "/"), "/")
	var last string
	for _, s := range segments {
		if s != "" {
			last = s
		}
	}
	if last == "" {
		return "arquivo"
	}
	if decoded, err := url.QueryUnescape(last); err == nil {
		last = decoded
	}
	if strings.TrimSpace(last) == "" {
		return "arquivo"
	}
	return last
}

func fetchGofileInfo(rawURL string) (models.FileInfo, error) {
	// Use Go provider if available
	p := GofileProvider{}
	client := &http.Client{}
	name, size, isFolder, files, err := p.GetFileInfo(client, rawURL)
	if err != nil {
		return models.FileInfo{}, err
	}
	var children *[]models.FileChildInfo
	if isFolder {
		var list []models.FileChildInfo
		for _, f := range files {
			mime := ""
			if f.MimeType != nil {
				mime = *f.MimeType
			}
			var mimePtr *string
			if mime != "" {
				mimePtr = &mime
			}
			src := p.ChildSourceURL(*p.ContentID(rawURL), f.ID)
			list = append(list, models.FileChildInfo{
				Filename:  f.Name,
				Size:      f.Size,
				MimeType:  mimePtr,
				IsFolder:  false,
				SourceURL: &src,
			})
		}
		children = &list
	}
	return models.FileInfo{Filename: name, Size: size, IsFolder: isFolder, Children: children}, nil
}

func fetchPixelDrainInfo(rawURL string) (models.FileInfo, error) {
	p := PixelDrainProvider{}
	target := p.ParseTarget(rawURL)
	if target == nil {
		return fetchDirectHTTPInfo(rawURL)
	}
	client := &http.Client{}
	if target.Type == PixelDrainFile {
		data, err := p.FetchFileInfo(client, target.ID)
		if err != nil {
			return models.FileInfo{}, err
		}
		name, _ := data["name"].(string)
		if name == "" {
			name, _ = data["title"].(string)
		}
		var size uint64
		switch v := data["size"].(type) {
		case float64:
			size = uint64(v)
		case int64:
			size = uint64(v)
		}
		return models.FileInfo{Filename: SanitizeFilename(name, "arquivo"), Size: size, IsFolder: false}, nil
	}
	// list
	data, err := p.FetchListInfo(client, target.ID)
	if err != nil {
		return models.FileInfo{}, err
	}
	// data contains files
	var total uint64
	var files []models.FileChildInfo
	if children, ok := data["files"].([]interface{}); ok {
		for _, it := range children {
			if m, ok := it.(map[string]interface{}); ok {
				name, _ := m["name"].(string)
				var sz uint64
				switch v := m["size"].(type) {
				case float64:
					sz = uint64(v)
				}
				total += sz
				files = append(files, models.FileChildInfo{Filename: name, Size: sz})
			}
		}
	}
	title, _ := data["title"].(string)
	if title == "" {
		title = target.ID
	}
	var ch *[]models.FileChildInfo
	if len(files) > 0 {
		ch = &files
	}
	return models.FileInfo{Filename: SanitizeFilename(title, "arquivo"), Size: total, IsFolder: true, Children: ch}, nil
}

func fetchMediaFireInfo(rawURL string) (models.FileInfo, error) {
	// Fallback to generic HEAD for now; mediafire provider in Go doesn't have GetFileInfo yet but we can try simple scrape
	// Try to use existing MediaFire provider if it has matching logic? For now generic.
	return fetchDirectHTTPInfo(rawURL)
}

func fetchGDriveInfo(rawURL string) (models.FileInfo, error) {
	// Use GDrive provider parsing
	p := GDriveProvider{}
	if id := p.ExtractID(rawURL); id != nil {
		// Try HEAD on download URL
		dlURL := p.DownloadURL(*id)
		info, err := fetchDirectHTTPInfo(dlURL)
		if err == nil && info.Filename != "" && info.Filename != "arquivo" {
			return info, nil
		}
		// fallback to parsing title from HTML?
		client := &http.Client{}
		req, _ := http.NewRequest("GET", rawURL, nil)
		req.Header.Set("User-Agent", DefaultUserAgent)
		resp, err := client.Do(req)
		if err == nil {
			defer resp.Body.Close()
			// generic fallback
			if info.Filename == "arquivo" {
				info.Filename = handlerFilenameFromURL(rawURL)
			}
			return info, nil
		}
		return info, nil
	}
	if fid := p.ExtractFolderID(rawURL); fid != nil {
		return models.FileInfo{Filename: SanitizeFilename(*fid, "pasta"), IsFolder: true, Size: 0}, nil
	}
	return fetchDirectHTTPInfo(rawURL)
}

var _ = fmt.Sprintf
