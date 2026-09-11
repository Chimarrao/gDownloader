package history

import (
	"database/sql"
	"encoding/json"
	"net/http"
	"strconv"

	"gdownloader-go/internal/db"
)

func ListHistory(w http.ResponseWriter, r *http.Request, database *sql.DB) {
	q := r.URL.Query()
	qq := q.Get("q")
	host := q.Get("host")
	from := q.Get("from")
	to := q.Get("to")
	page, _ := strconv.Atoi(q.Get("page"))
	pageSize, _ := strconv.Atoi(q.Get("page_size"))
	if pageSize == 0 {
		pageSize = 80
	}
	var qp, hp, fp, tp *string
	if qq != "" {
		qp = &qq
	}
	if host != "" {
		hp = &host
	}
	if from != "" {
		fp = &from
	}
	if to != "" {
		tp = &to
	}
	items, err := db.SearchHistory(database, qp, hp, fp, tp, page, pageSize)
	if err != nil {
		http.Error(w, `{"error":"Falha ao carregar histórico"}`, http.StatusInternalServerError)
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

func ListHistoryHosts(w http.ResponseWriter, r *http.Request, database *sql.DB) {
	hosts, err := db.ListHistoryHosts(database)
	if err != nil {
		http.Error(w, `{"error":"Falha ao carregar hosts"}`, http.StatusInternalServerError)
		return
	}
	if hosts == nil {
		hosts = []string{}
	}
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(hosts)
}

func SaveHistory(w http.ResponseWriter, r *http.Request, database *sql.DB) {
	var raw json.RawMessage
	if err := json.NewDecoder(r.Body).Decode(&raw); err != nil {
		http.Error(w, `{"error":"JSON inválido"}`, http.StatusBadRequest)
		return
	}
	if err := db.ReplaceHistoryRaw(database, raw); err != nil {
		http.Error(w, `{"error":"Falha ao salvar histórico"}`, http.StatusInternalServerError)
		return
	}
	w.WriteHeader(http.StatusNoContent)
}

func UpsertHistoryItem(w http.ResponseWriter, r *http.Request, database *sql.DB) {
	var raw json.RawMessage
	if err := json.NewDecoder(r.Body).Decode(&raw); err != nil {
		http.Error(w, `{"error":"JSON inválido"}`, http.StatusBadRequest)
		return
	}
	if err := db.UpsertHistoryItemRaw(database, raw); err != nil {
		http.Error(w, `{"error":"Falha ao salvar item"}`, http.StatusInternalServerError)
		return
	}
	w.WriteHeader(http.StatusNoContent)
}

func DeleteHistoryItem(w http.ResponseWriter, r *http.Request, database *sql.DB) {
	id := r.PathValue("id")
	if id == "" {
		// Fallback para mux Vars
		id = r.URL.Path[len("/history/"):]
	}
	if err := db.DeleteHistoryItem(database, id); err != nil {
		http.Error(w, `{"error":"Falha ao remover"}`, http.StatusInternalServerError)
		return
	}
	w.WriteHeader(http.StatusNoContent)
}

func ClearHistory(w http.ResponseWriter, r *http.Request, database *sql.DB) {
	if err := db.ClearHistory(database); err != nil {
		http.Error(w, `{"error":"Falha ao limpar"}`, http.StatusInternalServerError)
		return
	}
	w.WriteHeader(http.StatusNoContent)
}
