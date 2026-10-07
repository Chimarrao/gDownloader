// Package pcloudengine é o segundo motor de download em Go (ver onedriveengine
// pro primeiro) — suporte a links públicos do pCloud
// (https://u.pcloud.link/publink/show?code=<code>).
//
// Fluxo, validado manualmente via curl antes de escrever este arquivo:
//  1. GET na própria página do link público já devolve a árvore INTEIRA da
//     pasta (recursiva, com subpastas aninhadas) embutida no HTML como um
//     objeto JS `var publinkData = {...}` — não precisa de nenhuma API de
//     listagem separada, nem de cookie/sessão.
//  2. Cada arquivo tem `fileid` e `size`; o link de download de verdade vem
//     da API pública `https://api.pcloud.com/getpublinkdownload?code=<code>&fileid=<id>`,
//     que devolve `{hosts: [...], path: "..."}` — a URL final é
//     `https://<hosts[0]><path>`.
//  3. Esse link final suporta Range requests (206 Partial Content, Accept-Ranges),
//     confirmado com um download parcial real de teste.
package pcloudengine

import (
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"sync"
	"sync/atomic"
	"time"
)

const userAgent = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36"

// Matches reconhece links públicos do pCloud (u.pcloud.link ou my.pcloud.com,
// ambos usados pra link de compartilhamento) no formato .../publink/show?code=<code>.
func Matches(rawURL string) bool {
	u, err := url.Parse(rawURL)
	if err != nil {
		return false
	}
	host := strings.ToLower(u.Hostname())
	if !strings.Contains(host, "pcloud.link") && !strings.Contains(host, "pcloud.com") {
		return false
	}
	return strings.Contains(u.Path, "/publink/show") && u.Query().Get("code") != ""
}

func extractCode(rawURL string) (string, error) {
	u, err := url.Parse(rawURL)
	if err != nil {
		return "", err
	}
	code := u.Query().Get("code")
	if code == "" {
		return "", fmt.Errorf("link do pCloud sem código de compartilhamento")
	}
	return code, nil
}

type ChildInfo struct {
	Filename  string `json:"filename"`
	Size      uint64 `json:"size"`
	IsFolder  bool   `json:"isFolder"`
	Path      string `json:"path"`
	SourceURL string `json:"sourceUrl"` // aqui: o fileid como string
}

// pcloudItem espelha o shape de cada entrada em publinkData.metadata.contents
// (e recursivamente dentro de si mesma via Contents).
type pcloudItem struct {
	Name     string       `json:"name"`
	IsFolder bool         `json:"isfolder"`
	FileID   int64        `json:"fileid"`
	Size     uint64       `json:"size"`
	Contents []pcloudItem `json:"contents"`
}

type publinkData struct {
	Result   int    `json:"result"`
	Metadata struct {
		Name     string       `json:"name"`
		IsFolder bool         `json:"isfolder"`
		Contents []pcloudItem `json:"contents"`
	} `json:"metadata"`
}

var publinkDataRe = regexp.MustCompile(`(?s)var publinkData\s*=\s*(\{.*?\});`)

func fetchPublinkData(code string) (*publinkData, error) {
	pageURL := fmt.Sprintf("https://u.pcloud.link/publink/show?code=%s", url.QueryEscape(code))
	req, err := http.NewRequest("GET", pageURL, nil)
	if err != nil {
		return nil, err
	}
	req.Header.Set("User-Agent", userAgent)
	client := &http.Client{Timeout: 30 * time.Second}
	resp, err := client.Do(req)
	if err != nil {
		return nil, fmt.Errorf("falha ao acessar o link do pCloud: %w", err)
	}
	defer resp.Body.Close()
	body, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, err
	}
	match := publinkDataRe.FindSubmatch(body)
	if match == nil {
		return nil, fmt.Errorf("não foi possível ler os dados da pasta do pCloud (link inválido ou expirado)")
	}
	var data publinkData
	if err := json.Unmarshal(match[1], &data); err != nil {
		return nil, fmt.Errorf("resposta inesperada do pCloud: %w", err)
	}
	if data.Result != 0 {
		return nil, fmt.Errorf("pCloud recusou o link (código de erro %d)", data.Result)
	}
	return &data, nil
}

func flattenItems(items []pcloudItem, prefix string) []ChildInfo {
	var out []ChildInfo
	for _, item := range items {
		relPath := item.Name
		if prefix != "" {
			relPath = prefix + "/" + item.Name
		}
		if item.IsFolder {
			out = append(out, flattenItems(item.Contents, relPath)...)
			continue
		}
		out = append(out, ChildInfo{
			Filename:  item.Name,
			Size:      item.Size,
			IsFolder:  false,
			Path:      relPath,
			SourceURL: fmt.Sprintf("%d", item.FileID),
		})
	}
	return out
}

// GetFileInfo resolve nome da pasta raiz + lista completa e achatada de
// arquivos (com tamanho total), usada pela prévia no Capturador de Links.
func GetFileInfo(rawURL string) (name string, totalSize uint64, children []ChildInfo, err error) {
	code, err := extractCode(rawURL)
	if err != nil {
		return "", 0, nil, err
	}
	data, err := fetchPublinkData(code)
	if err != nil {
		return "", 0, nil, err
	}
	children = flattenItems(data.Metadata.Contents, "")
	if len(children) == 0 {
		return "", 0, nil, fmt.Errorf("pasta do pCloud vazia")
	}
	var total uint64
	for _, c := range children {
		total += c.Size
	}
	name = data.Metadata.Name
	if name == "" {
		name = "pCloud"
	}
	return name, total, children, nil
}

// resolveDownloadURL consulta a API pública do pCloud pra obter a URL real
// (com token temporário) de um arquivo específico do link compartilhado.
func resolveDownloadURL(code string, fileID string) (string, error) {
	api := fmt.Sprintf("https://api.pcloud.com/getpublinkdownload?code=%s&fileid=%s", url.QueryEscape(code), url.QueryEscape(fileID))
	req, err := http.NewRequest("GET", api, nil)
	if err != nil {
		return "", err
	}
	req.Header.Set("User-Agent", userAgent)
	client := &http.Client{Timeout: 30 * time.Second}
	resp, err := client.Do(req)
	if err != nil {
		return "", fmt.Errorf("falha ao resolver link de download do pCloud: %w", err)
	}
	defer resp.Body.Close()
	var out struct {
		Result int      `json:"result"`
		Path   string   `json:"path"`
		Hosts  []string `json:"hosts"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		return "", fmt.Errorf("resposta inválida do pCloud: %w", err)
	}
	if out.Result != 0 || len(out.Hosts) == 0 {
		return "", fmt.Errorf("pCloud recusou gerar o link de download (código %d)", out.Result)
	}
	return "https://" + out.Hosts[0] + out.Path, nil
}

// --- Motor de download (mesmo padrão do onedriveengine: jobs em background,
// consultados por polling pelo Rust) ---

type Job struct {
	mu              sync.Mutex
	Status          string
	BytesDownloaded int64
	TotalBytes      int64
	Error           string
	CurrentPath     string
	CurrentFilename string
	CurrentBytes    int64
	CurrentTotal    int64
}

type JobSnapshot struct {
	Status          string
	BytesDownloaded int64
	TotalBytes      int64
	Error           string
	CurrentPath     string
	CurrentFilename string
	CurrentBytes    int64
	CurrentTotal    int64
}

func (j *Job) snapshot() JobSnapshot {
	j.mu.Lock()
	defer j.mu.Unlock()
	return JobSnapshot{
		Status:          j.Status,
		BytesDownloaded: j.BytesDownloaded,
		TotalBytes:      j.TotalBytes,
		Error:           j.Error,
		CurrentPath:     j.CurrentPath,
		CurrentFilename: j.CurrentFilename,
		CurrentBytes:    j.CurrentBytes,
		CurrentTotal:    j.CurrentTotal,
	}
}

func (j *Job) fail(err error) {
	j.mu.Lock()
	defer j.mu.Unlock()
	j.Status = "error"
	j.Error = err.Error()
}

func (j *Job) addBytes(n int64) {
	j.mu.Lock()
	defer j.mu.Unlock()
	j.BytesDownloaded += n
}

func (j *Job) setTotal(total int64) {
	j.mu.Lock()
	defer j.mu.Unlock()
	j.TotalBytes = total
}

func (j *Job) finish() {
	j.mu.Lock()
	defer j.mu.Unlock()
	j.Status = "done"
}

func (j *Job) setCurrentFile(path, filename string, total int64) {
	j.mu.Lock()
	defer j.mu.Unlock()
	j.CurrentPath = path
	j.CurrentFilename = filename
	j.CurrentBytes = 0
	j.CurrentTotal = total
}

func (j *Job) addCurrentBytes(n int64) {
	j.mu.Lock()
	defer j.mu.Unlock()
	j.CurrentBytes += n
}

var (
	jobsMu sync.Mutex
	jobs   = map[string]*Job{}
	jobSeq uint64
)

func GetJob(id string) *Job {
	jobsMu.Lock()
	defer jobsMu.Unlock()
	return jobs[id]
}

func StartDownload(rawURL, destPath string, selectedChildren []string) string {
	id := fmt.Sprintf("pc-%d-%d", time.Now().UnixNano(), atomic.AddUint64(&jobSeq, 1))
	job := &Job{Status: "running"}
	jobsMu.Lock()
	jobs[id] = job
	jobsMu.Unlock()

	selected := map[string]bool{}
	for _, s := range selectedChildren {
		selected[s] = true
	}

	go runDownload(job, rawURL, destPath, selected)
	return id
}

func runDownload(job *Job, rawURL, destPath string, selected map[string]bool) {
	code, err := extractCode(rawURL)
	if err != nil {
		job.fail(err)
		return
	}
	data, err := fetchPublinkData(code)
	if err != nil {
		job.fail(err)
		return
	}
	children := flattenItems(data.Metadata.Contents, "")

	var filtered []ChildInfo
	if len(selected) > 0 {
		for _, c := range children {
			if selected[c.SourceURL] {
				filtered = append(filtered, c)
			}
		}
	} else {
		filtered = children
	}
	if len(filtered) == 0 {
		job.fail(fmt.Errorf("pasta do pCloud vazia ou sem arquivos selecionados"))
		return
	}

	var total int64
	for _, c := range filtered {
		total += int64(c.Size)
	}
	job.setTotal(total)

	if err := os.MkdirAll(destPath, 0o755); err != nil {
		job.fail(fmt.Errorf("falha ao criar a pasta de destino: %w", err))
		return
	}

	for _, c := range filtered {
		if err := downloadOneFile(code, destPath, c, job); err != nil {
			job.fail(err)
			return
		}
	}

	job.finish()
}

func downloadOneFile(code, destPath string, c ChildInfo, job *Job) error {
	target := filepath.Join(destPath, filepath.FromSlash(c.Path))
	if err := os.MkdirAll(filepath.Dir(target), 0o755); err != nil {
		return fmt.Errorf("falha ao criar subpasta: %w", err)
	}

	var existing int64
	if fi, err := os.Stat(target); err == nil {
		existing = fi.Size()
	}
	job.setCurrentFile(c.Path, c.Filename, int64(c.Size))
	if c.Size > 0 && uint64(existing) >= c.Size {
		job.addBytes(existing)
		job.addCurrentBytes(existing)
		return nil
	}

	downloadURL, err := resolveDownloadURL(code, c.SourceURL)
	if err != nil {
		return err
	}

	req, err := http.NewRequest("GET", downloadURL, nil)
	if err != nil {
		return err
	}
	req.Header.Set("User-Agent", userAgent)
	resumed := false
	if existing > 0 {
		req.Header.Set("Range", fmt.Sprintf("bytes=%d-", existing))
	}
	client := &http.Client{Timeout: 0}
	resp, err := client.Do(req)
	if err != nil {
		return fmt.Errorf("falha ao baixar %s: %w", c.Filename, err)
	}
	defer resp.Body.Close()

	if existing > 0 && resp.StatusCode == http.StatusPartialContent {
		resumed = true
	} else if existing > 0 && (resp.StatusCode == http.StatusBadRequest || resp.StatusCode == http.StatusRequestedRangeNotSatisfiable) {
		existing = 0
	} else if resp.StatusCode >= 400 {
		return fmt.Errorf("pCloud HTTP %d ao baixar %s", resp.StatusCode, c.Filename)
	}

	var f *os.File
	if resumed {
		f, err = os.OpenFile(target, os.O_WRONLY|os.O_APPEND, 0o644)
	} else {
		existing = 0
		f, err = os.Create(target)
	}
	if err != nil {
		return fmt.Errorf("falha ao abrir arquivo de destino: %w", err)
	}
	defer f.Close()
	if resumed {
		job.addBytes(existing)
		job.addCurrentBytes(existing)
	}

	buf := make([]byte, 256*1024)
	for {
		n, readErr := resp.Body.Read(buf)
		if n > 0 {
			if _, werr := f.Write(buf[:n]); werr != nil {
				return fmt.Errorf("falha ao escrever em disco: %w", werr)
			}
			job.addBytes(int64(n))
			job.addCurrentBytes(int64(n))
		}
		if readErr == io.EOF {
			break
		}
		if readErr != nil {
			return fmt.Errorf("conexão interrompida baixando %s: %w", c.Filename, readErr)
		}
	}
	return nil
}

// --- HTTP handlers ---

func writeJSON(w http.ResponseWriter, status int, v interface{}) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(v)
}

func writeErr(w http.ResponseWriter, status int, msg string) {
	writeJSON(w, status, map[string]string{"error": msg})
}

func FileInfoHandler(w http.ResponseWriter, r *http.Request) {
	rawURL := r.URL.Query().Get("url")
	if strings.TrimSpace(rawURL) == "" {
		writeErr(w, http.StatusBadRequest, "url obrigatório")
		return
	}
	name, size, children, err := GetFileInfo(rawURL)
	if err != nil {
		writeErr(w, http.StatusBadGateway, err.Error())
		return
	}
	writeJSON(w, http.StatusOK, map[string]interface{}{
		"name":     name,
		"size":     size,
		"isFolder": true,
		"children": children,
	})
}

type startDownloadRequest struct {
	URL              string   `json:"url"`
	DestPath         string   `json:"destPath"`
	SelectedChildren []string `json:"selectedChildren"`
}

func StartDownloadHandler(w http.ResponseWriter, r *http.Request) {
	var req startDownloadRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		writeErr(w, http.StatusBadRequest, "JSON inválido")
		return
	}
	if strings.TrimSpace(req.URL) == "" || strings.TrimSpace(req.DestPath) == "" {
		writeErr(w, http.StatusBadRequest, "url e destPath são obrigatórios")
		return
	}
	id := StartDownload(req.URL, req.DestPath, req.SelectedChildren)
	writeJSON(w, http.StatusOK, map[string]string{"jobId": id})
}

func JobStatusHandler(w http.ResponseWriter, r *http.Request) {
	id := strings.TrimPrefix(r.URL.Path, "/pcloud/jobs/")
	job := GetJob(id)
	if job == nil {
		writeErr(w, http.StatusNotFound, "job não encontrado")
		return
	}
	snap := job.snapshot()
	writeJSON(w, http.StatusOK, map[string]interface{}{
		"status":          snap.Status,
		"bytesDownloaded": snap.BytesDownloaded,
		"totalBytes":      snap.TotalBytes,
		"error":           snap.Error,
		"currentPath":     snap.CurrentPath,
		"currentFilename": snap.CurrentFilename,
		"currentBytes":    snap.CurrentBytes,
		"currentTotal":    snap.CurrentTotal,
	})
}

// RegisterRoutes registra as rotas deste motor no mux compartilhado (mesma
// porta fixa 127.0.0.1:48653 usada por todos os motores de nuvem em Go).
func RegisterRoutes(mux *http.ServeMux) {
	mux.HandleFunc("/pcloud/file-info", FileInfoHandler)
	mux.HandleFunc("/pcloud/jobs", func(w http.ResponseWriter, r *http.Request) {
		if r.Method == http.MethodPost {
			StartDownloadHandler(w, r)
			return
		}
		writeErr(w, http.StatusMethodNotAllowed, "método não suportado")
	})
	mux.HandleFunc("/pcloud/jobs/", JobStatusHandler)
}
