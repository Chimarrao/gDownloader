// Package onedriveengine é o PRIMEIRO motor de download que roda de verdade no
// Go (bytes de rede -> disco, com retomada), lado a lado com o motor do Rust —
// início da migração gradual pedida pelo usuário (novas implementações em Go).
//
// Fluxo OneDrive-for-Business / SharePoint "my site": o link que o usuário
// cola (formato https://<tenant>-my.sharepoint.com/:f:/g/personal/<user>/<token>)
// é um link de compartilhamento público/anônimo. Visitá-lo concede um cookie
// FedAuth anônimo (escopo só naquela pasta) via Set-Cookie, e redireciona pra
// URL de navegação (.../_layouts/15/onedrive.aspx?id=<caminho-servidor>) — o
// caminho server-relative da pasta raiz vem desse "id=". Com esse cookie já no
// client, a API REST do SharePoint (_api/web/GetFolderByServerRelativeUrl)
// lista arquivos/subpastas recursivamente, e o endpoint de download
// (_layouts/15/download.aspx?SourceUrl=...) baixa com suporte a Range (resume).
//
// Tudo isso foi validado manualmente via curl antes de escrever este arquivo:
// confirmado que a URL de navegação crua (sem primeiro visitar o link :f:)
// SEMPRE redireciona pra tela de login — só funciona se o cookie anônimo foi
// obtido primeiro visitando o link de compartilhamento de verdade.
package onedriveengine

import (
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/http/cookiejar"
	"net/url"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"time"
)

const userAgent = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36"

// ChildInfo é um arquivo (nunca pasta) já achatado da árvore recursiva, com
// Path relativo à raiz do compartilhamento (preserva subpastas pra UI montar
// a árvore, mesma convenção de DownloadChild.path no resto do app) e
// SourceURL = caminho server-relative completo (estável, usado tanto pra
// selected_children quanto pra montar a URL de download depois).
type ChildInfo struct {
	Filename  string `json:"filename"`
	Size      uint64 `json:"size"`
	IsFolder  bool   `json:"isFolder"`
	Path      string `json:"path"`
	SourceURL string `json:"sourceUrl"`
}

// Matches reconhece especificamente o formato de link de COMPARTILHAMENTO
// (/:f:/...), não a URL de navegação crua (.../_layouts/15/onedrive.aspx) —
// essa última não funciona como ponto de entrada porque exige o cookie
// anônimo que só o link de compartilhamento concede.
func Matches(rawURL string) bool {
	u, err := url.Parse(rawURL)
	if err != nil {
		return false
	}
	host := strings.ToLower(u.Hostname())
	if !strings.Contains(host, "sharepoint.com") {
		return false
	}
	return strings.Contains(u.Path, "/:f:/")
}

func newClient() (*http.Client, error) {
	jar, err := cookiejar.New(nil)
	if err != nil {
		return nil, err
	}
	return &http.Client{Jar: jar, Timeout: 120 * time.Second}, nil
}

// bootstrap visita o link de compartilhamento (concede o FedAuth anônimo no
// cookie jar do client) e extrai, da URL final pós-redirect, o site base
// (https://host/personal/<user>) e o caminho server-relative da pasta raiz
// (query param "id=", já vem url-decoded pelo parser do Go).
func bootstrap(client *http.Client, shareURL string) (siteBase string, rootPath string, err error) {
	req, err := http.NewRequest("GET", shareURL, nil)
	if err != nil {
		return "", "", err
	}
	req.Header.Set("User-Agent", userAgent)
	resp, err := client.Do(req)
	if err != nil {
		return "", "", fmt.Errorf("falha ao abrir o link do OneDrive: %w", err)
	}
	defer resp.Body.Close()

	final := resp.Request.URL
	id := final.Query().Get("id")
	if id == "" {
		return "", "", fmt.Errorf("não foi possível identificar a pasta compartilhada — o link pode exigir login (use o link de compartilhamento público, não a URL de navegação do OneDrive)")
	}
	segs := strings.Split(strings.Trim(final.Path, "/"), "/")
	if len(segs) < 2 || segs[0] != "personal" {
		return "", "", fmt.Errorf("formato de site OneDrive-for-Business inesperado")
	}
	siteBase = fmt.Sprintf("%s://%s/personal/%s", final.Scheme, final.Host, segs[1])
	return siteBase, id, nil
}

// Length vem como STRING no JSON do OData verbose clássico do SharePoint
// (serializa Edm.Int64 como string pra não perder precisão em JS) — bug real
// encontrado ao testar: unmarshal direto pra uint64 falhava com "cannot
// unmarshal string into ... Length of type uint64".
type spListResponse struct {
	D struct {
		Results []struct {
			Name              string `json:"Name"`
			ServerRelativeUrl string `json:"ServerRelativeUrl"`
			Length            string `json:"Length"`
		} `json:"results"`
	} `json:"d"`
}

func parseSPLength(s string) uint64 {
	n, _ := strconv.ParseUint(s, 10, 64)
	return n
}

func apiGet(client *http.Client, siteBase, endpoint string) (*spListResponse, error) {
	req, err := http.NewRequest("GET", siteBase+endpoint, nil)
	if err != nil {
		return nil, err
	}
	req.Header.Set("Accept", "application/json;odata=verbose")
	req.Header.Set("User-Agent", userAgent)
	resp, err := client.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	if resp.StatusCode >= 400 {
		return nil, fmt.Errorf("SharePoint API HTTP %d", resp.StatusCode)
	}
	var out spListResponse
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		return nil, fmt.Errorf("resposta inesperada da API do SharePoint: %w", err)
	}
	return &out, nil
}

// listRecursive percorre a árvore inteira (arquivos + subpastas) a partir de
// relPath, achatando tudo em arquivos com Path relativo a rootBase.
func listRecursive(client *http.Client, siteBase, relPath, rootBase string) ([]ChildInfo, error) {
	// PathEscape (não QueryEscape): o valor vai dentro de um segmento de PATH
	// ("GetFolderByServerRelativeUrl('<aqui>')"), onde espaço precisa virar
	// %20 — QueryEscape usa '+' pra espaço (convenção de query string), que o
	// parser de URL do SharePoint não interpreta como espaço nesse contexto
	// (bug real encontrado ao testar: retornava 0 arquivos sem erro nenhum).
	enc := url.PathEscape(relPath)
	var out []ChildInfo

	filesResp, err := apiGet(client, siteBase, fmt.Sprintf("/_api/web/GetFolderByServerRelativeUrl('%s')/Files", enc))
	if err != nil {
		return nil, err
	}
	for _, f := range filesResp.D.Results {
		rel := strings.TrimPrefix(f.ServerRelativeUrl, rootBase+"/")
		out = append(out, ChildInfo{
			Filename:  f.Name,
			Size:      parseSPLength(f.Length),
			IsFolder:  false,
			Path:      rel,
			SourceURL: f.ServerRelativeUrl,
		})
	}

	foldersResp, err := apiGet(client, siteBase, fmt.Sprintf("/_api/web/GetFolderByServerRelativeUrl('%s')/Folders", enc))
	if err != nil {
		return nil, err
	}
	for _, sub := range foldersResp.D.Results {
		// SharePoint sempre inclui uma subpasta de sistema "Forms" (metadados
		// de lista) em bibliotecas de documentos — nunca é conteúdo real.
		if strings.HasSuffix(sub.ServerRelativeUrl, "/Forms") {
			continue
		}
		children, err := listRecursive(client, siteBase, sub.ServerRelativeUrl, rootBase)
		if err != nil {
			return nil, err
		}
		out = append(out, children...)
	}
	return out, nil
}

// GetFileInfo resolve nome da pasta raiz + lista completa e achatada de
// arquivos (com tamanho total), usada pela prévia no Capturador de Links.
func GetFileInfo(rawURL string) (name string, totalSize uint64, children []ChildInfo, err error) {
	client, err := newClient()
	if err != nil {
		return "", 0, nil, err
	}
	siteBase, rootPath, err := bootstrap(client, rawURL)
	if err != nil {
		return "", 0, nil, err
	}
	children, err = listRecursive(client, siteBase, rootPath, rootPath)
	if err != nil {
		return "", 0, nil, err
	}
	for _, c := range children {
		totalSize += c.Size
	}
	segs := strings.Split(strings.TrimRight(rootPath, "/"), "/")
	name = "OneDrive"
	if len(segs) > 0 && segs[len(segs)-1] != "" {
		name = segs[len(segs)-1]
	}
	return name, totalSize, children, nil
}

// --- Motor de download (jobs em background, consultados por polling) ---

type Job struct {
	mu              sync.Mutex
	Status          string // running|done|error
	BytesDownloaded int64
	TotalBytes      int64
	Error           string
	// Arquivo atualmente sendo baixado dentro do job — a tela de downloads
	// (pastas com filhos) calcula velocidade/ETA/percentual somando o
	// progresso POR ARQUIVO reportado, não o agregado do job inteiro. Sem
	// isso a velocidade ficava travada em "0 KB/s" mesmo com bytes reais
	// sendo escritos em disco.
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

var (
	jobsMu sync.Mutex
	jobs   = map[string]*Job{}
)

func GetJob(id string) *Job {
	jobsMu.Lock()
	defer jobsMu.Unlock()
	return jobs[id]
}

// StartDownload inicia o download em background (goroutine própria) e
// devolve um ID de job pra consulta por polling — o motor de verdade (rede +
// disco) roda inteiramente aqui no Go; quem chama só acompanha o progresso.
// jobSeq garante ID único mesmo quando dois downloads começam no mesmo
// instante: time.Now().UnixNano() sozinho colidiu na prática (dois POSTs
// processados em paralelo geraram o MESMO ID, e o segundo job sobrescreveu o
// primeiro no map — os dois downloads passaram a compartilhar o progresso
// de um só, com tamanho/velocidade errados pro outro).
var jobSeq uint64

func StartDownload(rawURL, destPath string, selectedChildren []string) string {
	id := fmt.Sprintf("od-%d-%d", time.Now().UnixNano(), atomic.AddUint64(&jobSeq, 1))
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
	client, err := newClient()
	if err != nil {
		job.fail(err)
		return
	}
	siteBase, rootPath, err := bootstrap(client, rawURL)
	if err != nil {
		job.fail(err)
		return
	}
	children, err := listRecursive(client, siteBase, rootPath, rootPath)
	if err != nil {
		job.fail(err)
		return
	}

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
		job.fail(fmt.Errorf("pasta do OneDrive vazia ou sem arquivos selecionados"))
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
		if err := downloadOneFile(client, siteBase, destPath, c, job); err != nil {
			job.fail(err)
			return
		}
	}

	job.finish()
}

func downloadOneFile(client *http.Client, siteBase, destPath string, c ChildInfo, job *Job) error {
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

	downloadURL := fmt.Sprintf("%s/_layouts/15/download.aspx?SourceUrl=%s", siteBase, url.PathEscape(c.SourceURL))
	req, err := http.NewRequest("GET", downloadURL, nil)
	if err != nil {
		return err
	}
	req.Header.Set("User-Agent", userAgent)
	resumed := false
	if existing > 0 {
		req.Header.Set("Range", fmt.Sprintf("bytes=%d-", existing))
	}
	resp, err := client.Do(req)
	if err != nil {
		return fmt.Errorf("falha ao baixar %s: %w", c.Filename, err)
	}
	defer resp.Body.Close()

	if existing > 0 && resp.StatusCode == http.StatusPartialContent {
		resumed = true
	} else if existing > 0 && (resp.StatusCode == http.StatusBadRequest || resp.StatusCode == http.StatusRequestedRangeNotSatisfiable) {
		// Host não aceitou retomar a partir do byte existente: descarta e baixa de novo.
		existing = 0
	} else if resp.StatusCode >= 400 {
		return fmt.Errorf("OneDrive HTTP %d ao baixar %s", resp.StatusCode, c.Filename)
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
	// Bug real encontrado ao testar: sem isto, o progresso reportado (job.BytesDownloaded)
	// zerava a cada resume — o arquivo em disco continuava correto (append), mas a UI
	// mostraria o download "voltando" pra 0% a cada retomada.
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

// --- HTTP handlers (servidos num listener de porta fixa só pro Rust chamar) ---

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
	id := strings.TrimPrefix(r.URL.Path, "/onedrive/jobs/")
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

// StartEngineServer sobe um listener HTTP SEPARADO numa porta fixa, só pro
// backend Rust alcançar (loopback, sem necessidade de descoberta de porta
// dinâmica entre os dois processos — ambos já escutam em 127.0.0.1 apenas).
const EnginePort = 48653

// RegisterRoutes registra as rotas deste motor num mux compartilhado — o
// listener de verdade (porta fixa) é subido uma única vez em main.go, com
// TODOS os motores de nuvem (OneDrive, pCloud, ...) registrados no mesmo mux.
func RegisterRoutes(mux *http.ServeMux) {
	mux.HandleFunc("/onedrive/file-info", FileInfoHandler)
	mux.HandleFunc("/onedrive/jobs", func(w http.ResponseWriter, r *http.Request) {
		if r.Method == http.MethodPost {
			StartDownloadHandler(w, r)
			return
		}
		writeErr(w, http.StatusMethodNotAllowed, "método não suportado")
	})
	mux.HandleFunc("/onedrive/jobs/", JobStatusHandler)
}
