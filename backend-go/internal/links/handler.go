package links

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"regexp"
	"strings"
	"time"
)

// Portado de backend/src/routes/links.rs — importação de containers DLC/CCF/RSDF
// Mantém JSON shapes idênticos ao Rust: {links:[{url,filename,size}], source}

const maxContainerBytes = 8 * 1024 * 1024

var remoteEndpoints = []string{
	"https://dlc.piratejd.io/decrypt",
	"https://dlc.piratejd.io/api/decrypt",
	"https://dlc.piratejd.io/api",
	"https://dlc.piratejd.io/",
}

type ImportedContainerLink struct {
	URL      string `json:"url"`
	Filename string `json:"filename"`
	Size     uint64 `json:"size"`
}

type ImportContainerResponse struct {
	Links  []ImportedContainerLink `json:"links"`
	Source string                  `json:"source"`
}

type apiError struct {
	Error string `json:"error"`
}

func writeJSON(w http.ResponseWriter, status int, v interface{}) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(v)
}

func writeError(w http.ResponseWriter, status int, msg string) {
	writeJSON(w, status, apiError{Error: msg})
}

// Handler POST /links/import-container — multipart file field
func Handler(w http.ResponseWriter, r *http.Request) {
	if err := r.ParseMultipartForm(maxContainerBytes + 1024); err != nil {
		writeError(w, http.StatusBadRequest, "Falha ao ler upload multipart: "+err.Error())
		return
	}
	var filename string
	var fileBytes []byte
	found := false
	for _, fhs := range r.MultipartForm.File {
		for _, fh := range fhs {
			filename = fh.Filename
			if filename == "" {
				filename = "container.dlc"
			}
			f, err := fh.Open()
			if err != nil {
				continue
			}
			b, err := io.ReadAll(io.LimitReader(f, maxContainerBytes+1))
			f.Close()
			if err != nil {
				writeError(w, http.StatusBadRequest, "Falha ao ler upload multipart: "+err.Error())
				return
			}
			if len(b) == 0 {
				continue
			}
			if len(b) > maxContainerBytes {
				writeError(w, http.StatusRequestEntityTooLarge, "Container muito grande. O limite atual é 8 MB.")
				return
			}
			fileBytes = b
			found = true
			break
		}
		if found {
			break
		}
	}
	// Fallback: try form value "file" as raw bytes? Already handled. Also check r.FormFile
	if !found {
		// Try single file field "file"
		f, fh, err := r.FormFile("file")
		if err == nil {
			defer f.Close()
			filename = fh.Filename
			if filename == "" {
				filename = "container.dlc"
			}
			b, err := io.ReadAll(io.LimitReader(f, maxContainerBytes+1))
			if err != nil {
				writeError(w, http.StatusBadRequest, "Falha ao ler upload multipart: "+err.Error())
				return
			}
			if len(b) == 0 {
				writeError(w, http.StatusBadRequest, "Envie um arquivo .dlc, .ccf ou .rsdf no campo multipart.")
				return
			}
			if len(b) > maxContainerBytes {
				writeError(w, http.StatusRequestEntityTooLarge, "Container muito grande. O limite atual é 8 MB.")
				return
			}
			fileBytes = b
			found = true
		}
	}
	if !found {
		writeError(w, http.StatusBadRequest, "Envie um arquivo .dlc, .ccf ou .rsdf no campo multipart.")
		return
	}
	if err := validateContainerFilename(filename); err != nil {
		writeError(w, http.StatusBadRequest, err.Error())
		return
	}
	if plain := extractPlainLinks(fileBytes); plain != nil {
		writeJSON(w, http.StatusOK, ImportContainerResponse{Links: *plain, Source: "plain-text"})
		return
	}
	links, err := decryptWithRemoteService(filename, fileBytes)
	if err != nil {
		if len(links) == 0 {
			// empty but decrypted? Rust returns bad gateway with message "nenhum link..."
			// err already contains reason; we unify
			if strings.Contains(err.Error(), "nenhum link") || strings.Contains(err.Error(), "não retornou links") {
				writeError(w, http.StatusBadGateway, "O container foi decifrado, mas nenhum link foi encontrado na resposta.")
				return
			}
		}
		writeError(w, http.StatusBadGateway, "Falha ao decifrar o container. O serviço remoto dlc.piratejd.io não respondeu ou rejeitou o arquivo: "+err.Error())
		return
	}
	if len(links) == 0 {
		writeError(w, http.StatusBadGateway, "O container foi decifrado, mas nenhum link foi encontrado na resposta.")
		return
	}
	writeJSON(w, http.StatusOK, ImportContainerResponse{Links: links, Source: "dlc.piratejd.io"})
}

func validateContainerFilename(filename string) error {
	lower := strings.ToLower(filename)
	if strings.HasSuffix(lower, ".dlc") || strings.HasSuffix(lower, ".ccf") || strings.HasSuffix(lower, ".rsdf") {
		return nil
	}
	return &validationError{"Formato não suportado. Envie um arquivo .dlc, .ccf ou .rsdf."}
}

type validationError struct{ msg string }

func (e *validationError) Error() string { return e.msg }

func extractPlainLinks(bytes_ []byte) *[]ImportedContainerLink {
	text := string(bytes_)
	// check valid utf8? In Go string conversion is okay; Rust uses from_utf8 ok.
	// We attempt to extract links; if none, return nil
	links := extractLinksFromText(text)
	if len(links) == 0 {
		return nil
	}
	return &links
}

var urlRegex = regexp.MustCompile(`https?://[^\s"'<>\\]+`)

func extractLinksFromText(text string) []ImportedContainerLink {
	matches := urlRegex.FindAllString(text, -1)
	var out []ImportedContainerLink
	for _, raw := range matches {
		cleaned := strings.TrimRight(raw, ")],;,.")
		item := linkFromURL(cleaned)
		if isSupportedURL(item.URL) {
			out = append(out, item)
		}
	}
	return dedupeLinks(out)
}

func dedupeLinks(links []ImportedContainerLink) []ImportedContainerLink {
	seen := map[string]bool{}
	var out []ImportedContainerLink
	for _, l := range links {
		if !seen[l.URL] {
			seen[l.URL] = true
			out = append(out, l)
		}
	}
	return out
}

func isSupportedURL(raw string) bool {
	return strings.HasPrefix(raw, "http://") || strings.HasPrefix(raw, "https://")
}

func linkFromURL(raw string) ImportedContainerLink {
	return ImportedContainerLink{URL: raw, Filename: filenameFromURL(raw), Size: 0}
}

func filenameFromURL(raw string) string {
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
	decoded, err := url.QueryUnescape(last)
	if err == nil {
		last = decoded
	}
	last = strings.TrimSpace(last)
	if last == "" {
		return "arquivo"
	}
	return last
}

func decryptWithRemoteService(filename string, fileBytes []byte) ([]ImportedContainerLink, error) {
	client := &http.Client{Timeout: 35 * time.Second}
	var lastErr string = "nenhum endpoint testado"
	for _, endpoint := range remoteEndpoints {
		boundary, payload := buildMultipartPayload(filename, fileBytes)
		req, err := http.NewRequest("POST", endpoint, bytes.NewReader(payload))
		if err != nil {
			lastErr = err.Error()
			continue
		}
		req.Header.Set("Content-Type", "multipart/form-data; boundary="+boundary)
		resp, err := client.Do(req)
		if err != nil {
			lastErr = err.Error()
			continue
		}
		data, _ := io.ReadAll(resp.Body)
		resp.Body.Close()
		if resp.StatusCode < 200 || resp.StatusCode >= 300 {
			lastErr = endpoint + " retornou HTTP " + resp.Status
			continue
		}
		links := normalizeRemoteResponse(string(data))
		if len(links) > 0 {
			return links, nil
		}
		lastErr = endpoint + " não retornou links reconhecíveis"
	}
	return nil, &validationError{lastErr}
}

func buildMultipartPayload(filename string, fileBytes []byte) (string, []byte) {
	// Minimal multipart builder without mime/multipart to avoid extra boundary handling
	boundary := "gdl-boundary-123456"
	var buf bytes.Buffer
	// file part
	buf.WriteString("--" + boundary + "\r\n")
	buf.WriteString(`Content-Disposition: form-data; name="file"; filename="` + escapeQuotes(filename) + `"` + "\r\n")
	buf.WriteString("Content-Type: application/octet-stream\r\n\r\n")
	buf.Write(fileBytes)
	buf.WriteString("\r\n")
	// filename field
	buf.WriteString("--" + boundary + "\r\n")
	buf.WriteString(`Content-Disposition: form-data; name="filename"` + "\r\n\r\n")
	buf.WriteString(filename + "\r\n")
	buf.WriteString("--" + boundary + "--\r\n")
	return boundary, buf.Bytes()
}

func escapeQuotes(s string) string {
	return strings.ReplaceAll(s, `"`, `\"`)
}

func newMultipartWriter(buf *bytes.Buffer, filename string, bytes_ []byte) interface{} {
	return nil
}

func normalizeRemoteResponse(text string) []ImportedContainerLink {
	trimmed := strings.TrimSpace(text)
	if trimmed == "" {
		return nil
	}
	var links []ImportedContainerLink
	var raw interface{}
	if err := json.Unmarshal([]byte(trimmed), &raw); err == nil {
		collectJSONLinks(raw, &links)
	}
	links = append(links, extractLinksFromText(trimmed)...)
	return dedupeLinks(links)
}

func collectJSONLinks(value interface{}, out *[]ImportedContainerLink) {
	switch v := value.(type) {
	case []interface{}:
		for _, item := range v {
			collectJSONLinks(item, out)
		}
	case map[string]interface{}:
		// check url fields
		var urlVal *string
		for _, key := range []string{"url", "link", "downloadUrl"} {
			if s, ok := v[key].(string); ok && isSupportedURL(s) {
				urlVal = &s
				break
			}
		}
		if urlVal != nil {
			var filename string
			for _, key := range []string{"filename", "name"} {
				if s, ok := v[key].(string); ok && s != "" {
					filename = s
					break
				}
			}
			if filename == "" {
				filename = filenameFromURL(*urlVal)
			}
			var size uint64
			for _, key := range []string{"size", "bytes"} {
				if s := jsonSize(v[key]); s != nil {
					size = *s
					break
				}
			}
			*out = append(*out, ImportedContainerLink{URL: *urlVal, Filename: filename, Size: size})
		}
		for _, nested := range v {
			collectJSONLinks(nested, out)
		}
	case string:
		if isSupportedURL(v) {
			*out = append(*out, linkFromURL(v))
		}
	}
}

func jsonSize(v interface{}) *uint64 {
	switch x := v.(type) {
	case float64:
		u := uint64(x)
		return &u
	case int:
		u := uint64(x)
		return &u
	case int64:
		u := uint64(x)
		return &u
	case string:
		var u uint64
		// parse string
		s := strings.TrimSpace(x)
		var parsed uint64
		_, err := fmt.Sscanf(s, "%d", &parsed)
		if err == nil {
			u = parsed
			return &u
		}
		if parsed2, err2 := parseUint(s); err2 == nil {
			u = parsed2
			return &u
		}
	case json.Number:
		if n, err := x.Int64(); err == nil {
			u := uint64(n)
			return &u
		}
	}
	return nil
}

func parseUint(s string) (uint64, error) {
	var v uint64
	for _, ch := range s {
		if ch < '0' || ch > '9' {
			return 0, &validationError{"not uint"}
		}
		v = v*10 + uint64(ch-'0')
	}
	return v, nil
}

var _ = bytes.NewBuffer
