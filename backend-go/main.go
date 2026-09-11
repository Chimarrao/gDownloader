package main

import (
	"database/sql"
	"encoding/json"
	"fmt"
	"log"
	"net"
	"net/http"
	"os"
	"time"

	"github.com/gorilla/mux"
	"github.com/rs/cors"

	"gdownloader-go/internal/captcha"
	"gdownloader-go/internal/config"
	"gdownloader-go/internal/db"
	"gdownloader-go/internal/hashverify"
	"gdownloader-go/internal/health"
	"gdownloader-go/internal/history"
	"gdownloader-go/internal/integrity"
	"gdownloader-go/internal/links"
	"gdownloader-go/internal/migrations"
	"gdownloader-go/internal/mirrors"
	"gdownloader-go/internal/packages"
	"gdownloader-go/internal/providers"
	"gdownloader-go/internal/proxyintercept"
	"gdownloader-go/internal/stats"
	"gdownloader-go/internal/system"
	"gdownloader-go/internal/ws"
)

func main() {
	if len(os.Args) < 2 {
		log.Fatalf("uso: %s <caminho-do-banco>", os.Args[0])
	}
	dbPath := os.Args[1]

	database, err := db.Open(dbPath)
	if err != nil {
		log.Fatalf("falha ao abrir DB %s: %v", dbPath, err)
	}
	defer database.Close()

	// Roda migrações idempotentes (mesmas 22 do Rust) — permite Rust+Go compartilharem o arquivo
	if err := migrations.Run(database); err != nil {
		log.Fatalf("falha nas migrações: %v", err)
	}
	log.Printf("Go: migrações ok, DB=%s", dbPath)

	// Stats ticker — portado de lib.rs:114 (coleta a cada 1s e grava no ring buffer + broadcast)
	go func() {
		ticker := time.NewTicker(time.Second)
		defer ticker.Stop()
		for range ticker.C {
			// Em Go por enquanto sem scheduler de downloads em memória, registramos 0.
			// Quando o scheduler Go for implementado, somará speed_bps dos downloads ativos como no Rust.
			stats.DefaultManager.RecordStats(0, map[string]uint64{})
		}
	}()

	// Checkpoint periódico do WAL — portado de lib.rs:85 (a cada 2 min trunca WAL para não inflar)
	go func() {
		ticker := time.NewTicker(120 * time.Second)
		defer ticker.Stop()
		for range ticker.C {
			db.CheckpointWAL(database)
		}
	}()

	// Proxy intercept manager — portado de proxy_intercept.rs spawn_intercept_proxy_manager
	proxyintercept.SpawnInterceptProxyManager(database, dbPath)

	r := mux.NewRouter()

	// Health — portado de routes/health.rs:7
	r.HandleFunc("/health", health.Handler).Methods("GET")

	// Stats — portado de routes/stats.rs:4 + stats.rs (StatsTick, realtime stats)
	r.HandleFunc("/stats/realtime", stats.Handler).Methods("GET")

	// System — portado de routes/system.rs (disk usage)
	r.HandleFunc("/system/disk", system.DiskUsageHandler).Methods("GET")
	r.HandleFunc("/system/disks", system.ListDisksHandler).Methods("GET")

	// Mirrors — portado de routes/mirrors.rs + mirrors/mod.rs + searchers.rs (SSE mirrors search)
	r.HandleFunc("/mirrors/search", mirrors.Handler).Methods("GET")

	// Hash verify + Integrity — portado de hash_verify.rs + integrity.rs
	r.HandleFunc("/hash/verify", hashverify.Handler).Methods("POST")
	r.HandleFunc("/integrity/check", integrity.Handler).Methods("GET")

	// Links — portado de routes/links.rs (import-container .dlc/.ccf/.rsdf)
	r.HandleFunc("/links/import-container", links.Handler).Methods("POST")

	// Packages — portado de routes/packages.rs
	r.HandleFunc("/packages", func(w http.ResponseWriter, req *http.Request) {
		switch req.Method {
		case "GET":
			packages.ListPackages(w, req, database)
		case "POST":
			packages.CreatePackage(w, req, database)
		default:
			http.Error(w, `{"error":"método não permitido"}`, http.StatusMethodNotAllowed)
		}
	}).Methods("GET", "POST")
	r.HandleFunc("/packages/{id}", func(w http.ResponseWriter, req *http.Request) {
		packages.DeletePackage(w, req, database)
	}).Methods("DELETE")
	r.HandleFunc("/packages/{package_id}/assign/{download_id}", func(w http.ResponseWriter, req *http.Request) {
		packages.AssignDownloadToPackage(w, req, database)
	}).Methods("POST")
	r.HandleFunc("/packages/unassign/{download_id}", func(w http.ResponseWriter, req *http.Request) {
		packages.UnassignDownloadFromPackage(w, req, database)
	}).Methods("DELETE")

	// Providers — portado de routes/providers.rs
	r.HandleFunc("/providers", func(w http.ResponseWriter, req *http.Request) {
		providers.ListProviders(w, req, database)
	}).Methods("GET")
	r.HandleFunc("/detect", func(w http.ResponseWriter, req *http.Request) {
		providers.DetectProvider(w, req, database)
	}).Methods("GET")
	r.HandleFunc("/file-info", func(w http.ResponseWriter, req *http.Request) {
		providers.GetFileInfo(w, req, database)
	}).Methods("GET")
	r.HandleFunc("/file-info/cache", func(w http.ResponseWriter, req *http.Request) {
		switch req.Method {
		case "GET":
			providers.GetCachedFileInfo(w, req, database)
		case "DELETE":
			providers.ClearFileInfoCache(w, req, database)
		default:
			http.Error(w, `{"error":"método não permitido"}`, http.StatusMethodNotAllowed)
		}
	}).Methods("GET", "DELETE")
	r.HandleFunc("/file-info/cache/stats", func(w http.ResponseWriter, req *http.Request) {
		providers.FileInfoCacheStats(w, req, database)
	}).Methods("GET")

	// Proxy Intercept — portado de proxy_intercept.rs
	r.HandleFunc("/intercept", func(w http.ResponseWriter, req *http.Request) {
		proxyintercept.AddInterceptHandler(w, req, database, dbPath)
	}).Methods("POST")
	r.HandleFunc("/intercept/status", func(w http.ResponseWriter, req *http.Request) {
		proxyintercept.StatusHandler(w, req, database, dbPath)
	}).Methods("GET")
	r.HandleFunc("/intercept/history", func(w http.ResponseWriter, req *http.Request) {
		proxyintercept.HistoryHandler(w, req, database)
	}).Methods("GET")

	// WebSocket — portado de ws.rs
	r.HandleFunc("/ws", ws.Handler).Methods("GET")

	// Config — portado de routes/config.rs
	r.HandleFunc("/config/public", func(w http.ResponseWriter, req *http.Request) {
		switch req.Method {
		case "GET":
			config.GetPublicSettings(w, req, database)
		case "POST":
			config.UpdatePublicSettings(w, req, database)
		default:
			http.Error(w, `{"error":"método não permitido"}`, http.StatusMethodNotAllowed)
		}
	}).Methods("GET", "POST")
	r.HandleFunc("/config/secure", func(w http.ResponseWriter, req *http.Request) {
		switch req.Method {
		case "GET":
			config.GetSecureSettings(w, req, database)
		case "POST":
			config.UpdateSecureSettings(w, req, database)
		default:
			http.Error(w, `{"error":"método não permitido"}`, http.StatusMethodNotAllowed)
		}
	}).Methods("GET", "POST")
	r.HandleFunc("/config/legacy-migrations", func(w http.ResponseWriter, req *http.Request) {
		switch req.Method {
		case "GET":
			config.GetLegacyMigrations(w, req, database)
		case "POST":
			config.RecordLegacyMigration(w, req, database)
		default:
			http.Error(w, `{"error":"método não permitido"}`, http.StatusMethodNotAllowed)
		}
	}).Methods("GET", "POST")
	r.HandleFunc("/config/downloads", func(w http.ResponseWriter, req *http.Request) {
		config.UpdateDownloadConfig(w, req, database)
	}).Methods("POST")
	r.HandleFunc("/config/tor-runtime", func(w http.ResponseWriter, req *http.Request) {
		config.UpdateTorRuntime(w, req, database)
	}).Methods("POST")
	r.HandleFunc("/config/test-proxy", func(w http.ResponseWriter, req *http.Request) {
		config.TestProxy(w, req, database)
	}).Methods("GET")

	// Captcha — portado de routes/captcha.rs
	r.HandleFunc("/captcha", func(w http.ResponseWriter, req *http.Request) {
		captcha.CaptchaPage(w, req)
	}).Methods("GET")
	r.HandleFunc("/captcha/submit", func(w http.ResponseWriter, req *http.Request) {
		captcha.SubmitCaptcha(w, req, database)
	}).Methods("POST")

	// History — portado de routes/history.rs
	r.HandleFunc("/history", func(w http.ResponseWriter, req *http.Request) {
		switch req.Method {
		case "GET":
			history.ListHistory(w, req, database)
		case "POST":
			history.SaveHistory(w, req, database)
		case "DELETE":
			history.ClearHistory(w, req, database)
		default:
			http.Error(w, `{"error":"método não permitido"}`, http.StatusMethodNotAllowed)
		}
	}).Methods("GET", "POST", "DELETE")
	r.HandleFunc("/history/hosts", func(w http.ResponseWriter, req *http.Request) {
		history.ListHistoryHosts(w, req, database)
	}).Methods("GET")
	r.HandleFunc("/history/item", func(w http.ResponseWriter, req *http.Request) {
		history.UpsertHistoryItem(w, req, database)
	}).Methods("POST")
	r.HandleFunc("/history/{id}", func(w http.ResponseWriter, req *http.Request) {
		history.DeleteHistoryItem(w, req, database)
	}).Methods("DELETE")

	// Fallback para rotas não migradas — proxy para Rust (se disponível) ou 404
	// Para migração gradual, o Go responde 404 e o preload tenta Rust
	r.PathPrefix("/").HandlerFunc(func(w http.ResponseWriter, req *http.Request) {
		http.Error(w, `{"error":"rota não migrada para Go, use Rust"}`, http.StatusNotFound)
	})

	c := cors.New(cors.Options{
		AllowedOrigins:   []string{"*"},
		AllowedMethods:   []string{"GET", "POST", "PUT", "DELETE", "OPTIONS"},
		AllowedHeaders:   []string{"*"},
		AllowCredentials: true,
	})
	handler := c.Handler(r)

	// Liga em porta aleatória como Rust faz, e imprime READY:{"port":...}
	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		log.Fatalf("listen: %v", err)
	}
	port := ln.Addr().(*net.TCPAddr).Port
	// Mensagem compatível com backend-runtime.ts parseRustReadyPort
	fmt.Printf("READY:{\"port\":%d}\n", port)
	log.Printf("Go: ouvindo em 127.0.0.1:%d (DB %s)", port, dbPath)

	// Também sinaliza compatibilidade com Rust (PORT:... para fallback)
	fmt.Printf("PORT:%d\n", port)

	if err := http.Serve(ln, handler); err != nil {
		log.Fatalf("http serve: %v", err)
	}
}

// Pequeno helper para manter compatibilidade com db.Open que já faz _journal_mode=WAL
var _ = json.Marshal
var _ = sql.ErrNoRows
