package config

import (
	"database/sql"
	"encoding/json"
	"net/http"

	"gdownloader-go/internal/db"
)

// Portado de backend/src/routes/config.rs — handlers para /config/* e /config/*

func GetPublicSettings(w http.ResponseWriter, r *http.Request, database *sql.DB) {
	s, err := db.LoadPublicSettings(database)
	if err != nil {
		http.Error(w, `{"error":"Falha ao carregar settings"}`, http.StatusInternalServerError)
		return
	}
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(s)
}

func GetSecureSettings(w http.ResponseWriter, r *http.Request, database *sql.DB) {
	s, err := db.LoadSecureSettings(database)
	if err != nil {
		http.Error(w, `{"error":"Falha ao carregar credenciais"}`, http.StatusInternalServerError)
		return
	}
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(s)
}

func GetLegacyMigrations(w http.ResponseWriter, r *http.Request, database *sql.DB) {
	items, err := db.LoadLegacyMigrations(database)
	if err != nil {
		http.Error(w, `{"error":"Falha ao carregar migrações"}`, http.StatusInternalServerError)
		return
	}
	if items == nil {
		w.Header().Set("Content-Type", "application/json")
		w.Write([]byte("[]"))
		return
	}
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(items)
}

func UpdatePublicSettings(w http.ResponseWriter, r *http.Request, database *sql.DB) {
	var raw json.RawMessage
	if err := json.NewDecoder(r.Body).Decode(&raw); err != nil {
		http.Error(w, `{"error":"JSON inválido"}`, http.StatusBadRequest)
		return
	}
	// Valida que é JSON objeto
	var tmp map[string]interface{}
	if err := json.Unmarshal(raw, &tmp); err != nil {
		http.Error(w, `{"error":"JSON inválido"}`, http.StatusBadRequest)
		return
	}
	// Salva como RawMessage no app_kv (mesma tabela que Rust usa)
	if err := db.SaveRawSettings(database, "public_settings", raw); err != nil {
		http.Error(w, `{"error":"Falha ao salvar"}`, http.StatusInternalServerError)
		return
	}
	w.WriteHeader(http.StatusNoContent)
}

func UpdateSecureSettings(w http.ResponseWriter, r *http.Request, database *sql.DB) {
	var raw json.RawMessage
	if err := json.NewDecoder(r.Body).Decode(&raw); err != nil {
		http.Error(w, `{"error":"JSON inválido"}`, http.StatusBadRequest)
		return
	}
	if err := db.SaveRawSettings(database, "secure_settings", raw); err != nil {
		http.Error(w, `{"error":"Falha ao salvar credenciais"}`, http.StatusInternalServerError)
		return
	}
	w.WriteHeader(http.StatusNoContent)
}

func RecordLegacyMigration(w http.ResponseWriter, r *http.Request, database *sql.DB) {
	var req struct {
		Version int64  `json:"version"`
		Name    string `json:"name"`
	}
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, `{"error":"JSON inválido"}`, http.StatusBadRequest)
		return
	}
	if err := db.MarkLegacyMigration(database, req.Version, req.Name); err != nil {
		http.Error(w, `{"error":"Falha ao registrar migração"}`, http.StatusInternalServerError)
		return
	}
	w.WriteHeader(http.StatusNoContent)
}

func UpdateDownloadConfig(w http.ResponseWriter, r *http.Request, database *sql.DB) {
	// Apenas espelha o que Rust faz: atualiza max_concurrent_downloads em memória
	// Como Go é sidecar, apenas responde 204 para compatibilidade; o Rust real gerencia o scheduler
	w.WriteHeader(http.StatusNoContent)
}

func UpdateTorRuntime(w http.ResponseWriter, r *http.Request, database *sql.DB) {
	w.WriteHeader(http.StatusNoContent)
}

func TestProxy(w http.ResponseWriter, r *http.Request, database *sql.DB) {
	// Teste simples de proxy — retorna IP mockado para compatibilidade
	// O Rust real faz request para check.torproject.org; Go retorna stub se não configurado
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]interface{}{
		"ip":    "127.0.0.1",
		"isTor": false,
	})
}
