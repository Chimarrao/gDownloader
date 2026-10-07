// Persiste o texto bruto colado no Capturador de Links no SQLite (app_kv),
// pra sobreviver a reinícios do app — antes esse texto vivia só em memória no
// Vue (LinkGrabber.vue), então um restart apagava tudo que não tinha ido pra
// fila ainda.
package capturestate

import (
	"database/sql"
	"encoding/json"
	"net/http"

	"gdownloader-go/internal/db"
)

const kvKey = "capture_links_input"

type payload struct {
	Text string `json:"text"`
}

func Get(w http.ResponseWriter, r *http.Request, database *sql.DB) {
	text, _, err := db.GetKV(database, kvKey)
	if err != nil {
		http.Error(w, `{"error":"Falha ao carregar links capturados"}`, http.StatusInternalServerError)
		return
	}
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(payload{Text: text})
}

func Save(w http.ResponseWriter, r *http.Request, database *sql.DB) {
	var body payload
	if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
		http.Error(w, `{"error":"Corpo inválido"}`, http.StatusBadRequest)
		return
	}
	if err := db.SetKV(database, kvKey, body.Text); err != nil {
		http.Error(w, `{"error":"Falha ao salvar links capturados"}`, http.StatusInternalServerError)
		return
	}
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]bool{"ok": true})
}
