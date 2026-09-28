package providers

import (
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strings"
)

// Portado de backend/src/providers/archive.rs — Internet Archive.

const archiveHost = "archive.org"

var archiveHosts = []string{"archive.org", "www.archive.org"}

type ArchiveProvider struct{}

type ArchiveTargetType string

const (
	ArchiveItem     ArchiveTargetType = "item"
	ArchiveFileKind ArchiveTargetType = "file"
)

type ArchiveTarget struct {
	Type       ArchiveTargetType
	Identifier string
	Filename   string
}

type ArchiveFile struct {
	Filename  string  `json:"filename"`
	Size      uint64  `json:"size"`
	MimeType  *string `json:"mimeType,omitempty"`
	SourceURL string  `json:"sourceUrl"`
}

func (p ArchiveProvider) Matches(rawURL string) bool {
	if !HostMatches(rawURL, archiveHosts) {
		return false
	}
	return p.ParseTarget(rawURL) != nil
}

func (p ArchiveProvider) ParseTarget(rawURL string) *ArchiveTarget {
	if !HostMatches(rawURL, archiveHosts) {
		return nil
	}
	segs := PathSegments(rawURL)
	switch {
	case len(segs) == 2 && segs[0] == "download" && segs[1] != "":
		return &ArchiveTarget{Type: ArchiveItem, Identifier: segs[1]}
	case len(segs) >= 3 && segs[0] == "download" && segs[1] != "" && segs[2] != "":
		decoded, _ := url.PathUnescape(segs[2])
		if decoded == "" {
			decoded = segs[2]
		}
		return &ArchiveTarget{Type: ArchiveFileKind, Identifier: segs[1], Filename: decoded}
	default:
		return nil
	}
}

func archiveFileURL(identifier, filename string) string {
	return fmt.Sprintf("https://archive.org/download/%s/%s", url.PathEscape(identifier), url.PathEscape(filename))
}

func (p ArchiveProvider) ItemFiles(client *http.Client, identifier string) ([]ArchiveFile, error) {
	resp, err := client.Get(fmt.Sprintf("https://archive.org/metadata/%s", url.PathEscape(identifier)))
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	if resp.StatusCode >= 400 {
		return nil, fmt.Errorf("Internet Archive HTTP %d", resp.StatusCode)
	}
	body, _ := io.ReadAll(io.LimitReader(resp.Body, 4*1024*1024))
	var metadata map[string]interface{}
	if err := json.Unmarshal(body, &metadata); err != nil {
		return nil, err
	}
	filesRaw, ok := metadata["files"].([]interface{})
	if !ok {
		return nil, fmt.Errorf("Internet Archive não retornou arquivos para este item")
	}
	var entries []ArchiveFile
	var withSource []struct {
		source string
		file   ArchiveFile
	}
	for _, entry := range filesRaw {
		m, ok := entry.(map[string]interface{})
		if !ok {
			continue
		}
		name, _ := m["name"].(string)
		name = strings.TrimSpace(name)
		if name == "" || strings.HasSuffix(name, "/") || strings.HasPrefix(name, "__ia_") || name == fmt.Sprintf("%s_meta.xml", identifier) || name == fmt.Sprintf("%s_meta.sqlite", identifier) {
			continue
		}
		var size uint64
		switch v := m["size"].(type) {
		case float64:
			size = uint64(v)
		case string:
			fmt.Sscanf(v, "%d", &size)
		case int:
			size = uint64(v)
		}
		if size == 0 {
			continue
		}
		source, _ := m["source"].(string)
		var mime *string
		if f, ok := m["format"].(string); ok && f != "" {
			mime = &f
		}
		file := ArchiveFile{
			Filename:  SanitizeFilename(name, "arquivo_archive"),
			Size:      size,
			MimeType:  mime,
			SourceURL: archiveFileURL(identifier, name),
		}
		withSource = append(withSource, struct {
			source string
			file   ArchiveFile
		}{source: source, file: file})
		_ = entries
	}
	hasOriginal := false
	for _, e := range withSource {
		if e.source == "original" {
			hasOriginal = true
			break
		}
	}
	for _, e := range withSource {
		if hasOriginal && e.source != "original" {
			continue
		}
		entries = append(entries, e.file)
	}
	return entries, nil
}

func (p ArchiveProvider) FetchFileInfo(client *http.Client, rawURL string) (string, uint64, bool, []ArchiveFile, error) {
	target := p.ParseTarget(rawURL)
	if target == nil {
		return "", 0, false, nil, fmt.Errorf("URL do Internet Archive inválida")
	}
	files, err := p.ItemFiles(client, target.Identifier)
	if err != nil {
		return "", 0, false, nil, err
	}
	if target.Type == ArchiveFileKind {
		for _, f := range files {
			if f.Filename == target.Filename {
				return f.Filename, f.Size, false, []ArchiveFile{f}, nil
			}
		}
		return "", 0, false, nil, fmt.Errorf("Arquivo não encontrado no item do Internet Archive")
	}
	if len(files) == 0 {
		return "", 0, false, nil, fmt.Errorf("Item do Internet Archive não contém arquivos originais disponíveis")
	}
	total := uint64(0)
	for _, f := range files {
		total += f.Size
	}
	return SanitizeFilename(target.Identifier, "internet_archive"), total, true, files, nil
}

// Ensure import not unused
var _ = url.PathEscape
