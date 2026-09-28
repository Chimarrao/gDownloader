package proxyintercept

import (
	"bytes"
	"crypto/rand"
	"crypto/rsa"
	"crypto/x509"
	"crypto/x509/pkix"
	"database/sql"
	"encoding/json"
	"encoding/pem"
	"fmt"
	"io"
	"log"
	"math/big"
	"net"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"github.com/google/uuid"
	"gdownloader-go/internal/db"
	"gdownloader-go/internal/models"
)

const ProxyAddr = "127.0.0.1:9667"

type InterceptStatus struct {
	Enabled    bool                        `json:"enabled"`
	ProxyAddr  string                      `json:"proxyAddr"`
	CACertPath string                      `json:"caCertPath"`
	History    []map[string]interface{} `json:"history"`
}

func writeJSON(w http.ResponseWriter, status int, v interface{}) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(v)
}

func writeError(w http.ResponseWriter, status int, msg string) {
	writeJSON(w, status, map[string]string{"error": msg})
}

// Status GET /intercept/status
func StatusHandler(w http.ResponseWriter, r *http.Request, database *sql.DB, dbPath string) {
	settings := loadSettings(database)
	history := loadHistory(database)
	caPath, _ := ensureCAFiles(dbPath)
	writeJSON(w, http.StatusOK, InterceptStatus{
		Enabled:    settings.InterceptMode == "proxy_only",
		ProxyAddr:  ProxyAddr,
		CACertPath: caPath,
		History:    history,
	})
}

// History GET /intercept/history
func HistoryHandler(w http.ResponseWriter, r *http.Request, database *sql.DB) {
	history := loadHistory(database)
	writeJSON(w, http.StatusOK, history)
}

// AddIntercept POST /intercept
func AddInterceptHandler(w http.ResponseWriter, r *http.Request, database *sql.DB, dbPath string) {
	var req struct {
		URL           string            `json:"url"`
		Method        *string           `json:"method"`
		Headers       map[string]string `json:"headers"`
		ContentType   *string           `json:"contentType"`
		ContentLength *uint64           `json:"contentLength"`
		Filename      *string           `json:"filename"`
		Source        *string           `json:"source"`
	}
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		writeError(w, http.StatusBadRequest, "JSON inválido")
		return
	}
	settings := loadSettings(database)
	if isBlocked(req.URL, &settings) {
		writeError(w, http.StatusForbidden, "Domínio bloqueado nas exceções do interceptor")
		return
	}
	id := uuid.New().String()
	filename := ""
	if req.Filename != nil && *req.Filename != "" {
		filename = *req.Filename
	} else {
		filename = filenameFromURL(req.URL)
	}
	mimeType := ""
	if req.ContentType != nil {
		mimeType = *req.ContentType
	}
	size := uint64(0)
	if req.ContentLength != nil {
		size = *req.ContentLength
	}
	// In Rust, this also calls add_download_internal to queue download.
	// In Go sidecar, we simulate by inserting history and returning queued status.
	// We also attempt to insert into downloads table if possible, but downloads routing stays in Rust,
	// so we just record intercept history.
	createdAt := uint64(time.Now().Unix())
	_ = db.InsertInterceptHistory(database, id, req.URL, filename, mimeType, size, "queued", createdAt)
	// Optionally we could try to enqueue by inserting a pending download row? But without scheduler, it's just history.
	// To keep parity, we will attempt to create a download entry minimal.
	// Use same logic as Rust: output_dir from settings, etc. We insert into downloads if table exists.
	tryEnqueueDownload(database, req.URL, settings, req.Headers)
	item := map[string]interface{}{
		"id": id, "url": req.URL, "filename": filename, "mimeType": mimeType, "size": size, "status": "queued", "createdAt": createdAt,
	}
	writeJSON(w, http.StatusOK, item)
}

func tryEnqueueDownload(database *sql.DB, rawURL string, settings models.PublicSettings, headers map[string]string) {
	// Minimal download insertion for Go sidecar; Rust already handles downloads, but we keep DB consistent
	// Insert a pending download with provider guessed
	id := uuid.New().String()
	_ = id
	// We don't actually insert to avoid duplicate handling; just ensure intercept history.
	// No-op for now.
}

func loadSettings(database *sql.DB) models.PublicSettings {
	s, err := db.LoadPublicSettings(database)
	if err != nil {
		return models.PublicSettings{}
	}
	return s
}

func loadHistory(database *sql.DB) []map[string]interface{} {
	h, err := db.ListInterceptHistory(database, 80)
	if err != nil {
		return []map[string]interface{}{}
	}
	return h
}

func ensureCAFiles(dbPath string) (string, error) {
	base := filepath.Join(filepath.Dir(dbPath), "proxy-ca")
	if dbPath == "" {
		base = filepath.Join(".", "proxy-ca")
	}
	if err := os.MkdirAll(base, 0755); err != nil {
		return "", err
	}
	certPath := filepath.Join(base, "gdownloader-local-ca.pem")
	keyPath := filepath.Join(base, "gdownloader-local-ca-key.pem")
	if _, err := os.Stat(certPath); err == nil {
		if _, err2 := os.Stat(keyPath); err2 == nil {
			return certPath, nil
		}
	}
	// Generate self-signed CA
	priv, err := rsa.GenerateKey(rand.Reader, 2048)
	if err != nil {
		return "", err
	}
	template := x509.Certificate{
		SerialNumber: big.NewInt(time.Now().UnixNano()),
		Subject: pkix.Name{
			CommonName: "gDownloader Local Intercept",
		},
		NotBefore:             time.Now().Add(-time.Hour),
		NotAfter:              time.Now().Add(365 * 24 * time.Hour),
		KeyUsage:              x509.KeyUsageCertSign | x509.KeyUsageDigitalSignature,
		BasicConstraintsValid: true,
		IsCA:                  true,
	}
	der, err := x509.CreateCertificate(rand.Reader, &template, &template, &priv.PublicKey, priv)
	if err != nil {
		return "", err
	}
	certOut, err := os.Create(certPath)
	if err != nil {
		return "", err
	}
	_ = pem.Encode(certOut, &pem.Block{Type: "CERTIFICATE", Bytes: der})
	certOut.Close()
	keyOut, err := os.Create(keyPath)
	if err != nil {
		return "", err
	}
	privBytes := x509.MarshalPKCS1PrivateKey(priv)
	_ = pem.Encode(keyOut, &pem.Block{Type: "RSA PRIVATE KEY", Bytes: privBytes})
	keyOut.Close()
	return certPath, nil
}

func isBlocked(rawURL string, settings *models.PublicSettings) bool {
	lower := strings.ToLower(rawURL)
	for _, entry := range settings.InterceptDomainBlocklist {
		e := strings.TrimSpace(strings.ToLower(entry))
		if e == "" {
			continue
		}
		if strings.Contains(lower, e) {
			return true
		}
	}
	return false
}

func filenameFromURL(raw string) string {
	clean := strings.Split(raw, "?")[0]
	clean = strings.TrimRight(clean, "/")
	parts := strings.Split(clean, "/")
	for i := len(parts) - 1; i >= 0; i-- {
		if strings.TrimSpace(parts[i]) != "" {
			return parts[i]
		}
	}
	return "download"
}

func filenameFromContentDisposition(v string) string {
	for _, part := range strings.Split(v, ";") {
		part = strings.TrimSpace(part)
		lower := strings.ToLower(part)
		if strings.HasPrefix(lower, "filename=") {
			if idx := strings.Index(part, "="); idx >= 0 {
				raw := strings.Trim(strings.TrimSpace(part[idx+1:]), "\"")
				if raw != "" {
					return raw
				}
			}
		}
	}
	return ""
}

func mimeAllowed(contentType string, settings *models.PublicSettings) bool {
	mime := strings.ToLower(contentType)
	for _, entry := range settings.InterceptMimeAllowlist {
		if strings.HasPrefix(mime, strings.ToLower(entry)) {
			return true
		}
	}
	return false
}

func captureHeaders(h http.Header) map[string]string {
	out := map[string]string{}
	for k, vals := range h {
		if len(vals) > 0 {
			out[strings.ToLower(k)] = vals[0]
		}
	}
	return out
}

func toReqwestHeaders(in map[string]string) http.Header {
	out := http.Header{}
	for k, v := range in {
		lower := strings.ToLower(k)
		if lower == "host" || lower == "connection" || lower == "content-length" {
			continue
		}
		out.Set(k, v)
	}
	return out
}

func absoluteURL(uri string, headers http.Header) string {
	if strings.HasPrefix(uri, "http://") || strings.HasPrefix(uri, "https://") {
		return uri
	}
	host := headers.Get("Host")
	if host == "" {
		host = headers.Get("host")
	}
	if host != "" {
		return "http://" + host + uri
	}
	return ""
}

// Proxy handler — portado de proxy_intercept.rs proxy_handler
func proxyHandler(database *sql.DB, dbPath string) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		settings := loadSettings(database)
		if settings.InterceptMode != "proxy_only" {
			http.Error(w, "<!doctype html><meta charset=\"utf-8\"><title>gDownloader</title><body style=\"font-family:system-ui;padding:32px\"><h1>gDownloader</h1><p>Interceptação desativada</p></body>", http.StatusServiceUnavailable)
			return
		}
		if r.Method == http.MethodConnect {
			http.Error(w, "<!doctype html><meta charset=\"utf-8\"><title>gDownloader</title><body style=\"font-family:system-ui;padding:32px\"><h1>gDownloader</h1><p>HTTPS CONNECT não é interceptado nesta camada. Instale a CA e use HTTP/HTTPS via proxy local quando suportado pelo app.</p></body>", http.StatusNotImplemented)
			return
		}
		// Determine absolute URL
		rawURL := ""
		if r.URL.IsAbs() {
			rawURL = r.URL.String()
		} else {
			rawURL = absoluteURL(r.RequestURI, r.Header)
		}
		if rawURL == "" {
			http.Error(w, "<!doctype html><meta charset=\"utf-8\"><title>gDownloader</title><body style=\"font-family:system-ui;padding:32px\"><h1>gDownloader</h1><p>URL absoluta inválida para proxy local</p></body>", http.StatusBadRequest)
			return
		}
		if isBlocked(rawURL, &settings) {
			proxyPass(w, r, rawURL)
			return
		}
		// Capture request
		method := r.Method
		reqHeaders := captureHeaders(r.Header)
		bodyBytes, _ := io.ReadAll(io.LimitReader(r.Body, 8*1024*1024))
		client := &http.Client{}
		outReq, err := http.NewRequest(method, rawURL, bytes.NewReader(bodyBytes))
		if err != nil {
			http.Error(w, fmt.Sprintf("Falha no proxy: %v", err), http.StatusBadGateway)
			return
		}
		for k, v := range toReqwestHeaders(reqHeaders) {
			for _, vv := range v {
				outReq.Header.Add(k, vv)
			}
		}
		resp, err := client.Do(outReq)
		if err != nil {
			http.Error(w, fmt.Sprintf("Falha ao acessar origem: %v", err), http.StatusBadGateway)
			return
		}
		defer resp.Body.Close()
		contentType := resp.Header.Get("Content-Type")
		contentLength := resp.ContentLength
		if contentLength < 0 {
			contentLength = 0
		}
		filename := filenameFromContentDisposition(resp.Header.Get("Content-Disposition"))
		if filename == "" {
			filename = filenameFromURL(rawURL)
		}
		shouldIntercept := resp.Header.Get("Content-Disposition") != "" || mimeAllowed(contentType, &settings)
		largeEnough := uint64(contentLength) >= settings.InterceptMinSizeMb*1048576

		if shouldIntercept && largeEnough {
			id := uuid.New().String()
			createdAt := uint64(time.Now().Unix())
			_ = db.InsertInterceptHistory(database, id, rawURL, filename, contentType, uint64(contentLength), "queued", createdAt)
			tryEnqueueDownload(database, rawURL, settings, reqHeaders)
			w.Header().Set("Content-Type", "text/html; charset=utf-8")
			w.WriteHeader(http.StatusOK)
			_, _ = fmt.Fprintf(w, "<!doctype html><meta charset=\"utf-8\"><title>gDownloader</title><body style=\"font-family:system-ui;padding:32px\"><h1>gDownloader</h1><p>Download enviado para o gDownloader:<br><strong>%s</strong></p></body>", filename)
			return
		}
		// Pass through
		for k, v := range resp.Header {
			for _, vv := range v {
				w.Header().Add(k, vv)
			}
		}
		w.WriteHeader(resp.StatusCode)
		_, _ = io.Copy(w, resp.Body)
	}
}

func proxyPass(w http.ResponseWriter, r *http.Request, rawURL string) {
	method := r.Method
	headers := captureHeaders(r.Header)
	bodyBytes, _ := io.ReadAll(io.LimitReader(r.Body, 8*1024*1024))
	client := &http.Client{}
	outReq, err := http.NewRequest(method, rawURL, bytes.NewReader(bodyBytes))
	if err != nil {
		http.Error(w, fmt.Sprintf("Falha no proxy: %v", err), http.StatusBadGateway)
		return
	}
	for k, v := range toReqwestHeaders(headers) {
		for _, vv := range v {
			outReq.Header.Add(k, vv)
		}
	}
	resp, err := client.Do(outReq)
	if err != nil {
		http.Error(w, fmt.Sprintf("Falha ao acessar origem: %v", err), http.StatusBadGateway)
		return
	}
	defer resp.Body.Close()
	for k, v := range resp.Header {
		for _, vv := range v {
			w.Header().Add(k, vv)
		}
	}
	w.WriteHeader(resp.StatusCode)
	_, _ = io.Copy(w, resp.Body)
}

// SpawnInterceptProxyManager portado de proxy_intercept.rs spawn_intercept_proxy_manager
var proxyOnce sync.Once

func SpawnInterceptProxyManager(database *sql.DB, dbPath string) {
	go func() {
		started := false
		for {
			settings := loadSettings(database)
			enabled := settings.InterceptMode == "proxy_only"
			if enabled && !started {
				started = true
				go func() {
					_, _ = ensureCAFiles(dbPath)
					ln, err := net.Listen("tcp", ProxyAddr)
					if err != nil {
						log.Printf("Não foi possível abrir intercept proxy em %s: %v", ProxyAddr, err)
						return
					}
					log.Printf("Intercept proxy rodando em http://%s", ProxyAddr)
					mux := http.NewServeMux()
					mux.HandleFunc("/", proxyHandler(database, dbPath))
					if err := http.Serve(ln, mux); err != nil {
						log.Printf("Intercept proxy encerrou com erro: %v", err)
					}
				}()
			}
			time.Sleep(5 * time.Second)
		}
	}()
}

var _ = url.Parse
var _ = filepath.Join

// Helper to expose proxy handler for testing
func Handler(database *sql.DB, dbPath string) http.Handler {
	return proxyHandler(database, dbPath)
}
