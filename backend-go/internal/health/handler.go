package health

import (
	"encoding/json"
	"net/http"
)

// Portado de backend/src/routes/health.rs:7 — GET /health {status, version}
func Handler(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(http.StatusOK)
	// version vem de go.mod; para paridade com Rust env!("CARGO_PKG_VERSION") usamos "0.1.0"
	_ = json.NewEncoder(w).Encode(map[string]string{
		"status":  "ok",
		"version": "0.1.0",
	})
}
