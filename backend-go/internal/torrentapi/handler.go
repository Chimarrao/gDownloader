// Package torrentapi expõe o torrentengine via HTTP REST, no mesmo estilo dos
// outros módulos do sidecar Go (ver internal/packages para o padrão seguido).
package torrentapi

import (
	"encoding/json"
	"net/http"

	"github.com/gorilla/mux"

	"gdownloader-go/internal/torrentengine"
)

func writeJSON(w http.ResponseWriter, status int, v interface{}) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(v)
}

func writeError(w http.ResponseWriter, status int, msg string) {
	writeJSON(w, status, map[string]string{"error": msg})
}

type addRequest struct {
	Source      string `json:"source"`
	DestDir     string `json:"destDir"`
	TorRequired bool   `json:"torRequired"`
}

// Add POST /torrents  {source, destDir, torRequired}
func Add(w http.ResponseWriter, r *http.Request, e *torrentengine.Engine) {
	var req addRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		writeError(w, http.StatusBadRequest, "JSON inválido")
		return
	}
	status, err := e.Add(req.Source, req.DestDir, req.TorRequired)
	if err != nil {
		writeError(w, http.StatusBadRequest, err.Error())
		return
	}
	writeJSON(w, http.StatusOK, status)
}

// List GET /torrents
func List(w http.ResponseWriter, r *http.Request, e *torrentengine.Engine) {
	writeJSON(w, http.StatusOK, e.List())
}

// Get GET /torrents/{id}
func Get(w http.ResponseWriter, r *http.Request, e *torrentengine.Engine) {
	id := mux.Vars(r)["id"]
	status, err := e.Get(id)
	if err != nil {
		writeError(w, http.StatusNotFound, err.Error())
		return
	}
	writeJSON(w, http.StatusOK, status)
}

// Pause POST /torrents/{id}/pause
func Pause(w http.ResponseWriter, r *http.Request, e *torrentengine.Engine) {
	id := mux.Vars(r)["id"]
	if err := e.Pause(id); err != nil {
		writeError(w, http.StatusNotFound, err.Error())
		return
	}
	w.WriteHeader(http.StatusNoContent)
}

// Resume POST /torrents/{id}/resume
func Resume(w http.ResponseWriter, r *http.Request, e *torrentengine.Engine) {
	id := mux.Vars(r)["id"]
	if err := e.Resume(id); err != nil {
		writeError(w, http.StatusNotFound, err.Error())
		return
	}
	w.WriteHeader(http.StatusNoContent)
}

// Recheck POST /torrents/{id}/recheck
func Recheck(w http.ResponseWriter, r *http.Request, e *torrentengine.Engine) {
	id := mux.Vars(r)["id"]
	if err := e.Recheck(id); err != nil {
		writeError(w, http.StatusNotFound, err.Error())
		return
	}
	w.WriteHeader(http.StatusNoContent)
}

// Remove DELETE /torrents/{id}?deleteFiles=1
func Remove(w http.ResponseWriter, r *http.Request, e *torrentengine.Engine) {
	id := mux.Vars(r)["id"]
	deleteFiles := r.URL.Query().Get("deleteFiles") == "1" || r.URL.Query().Get("deleteFiles") == "true"
	if err := e.Remove(id, deleteFiles); err != nil {
		writeError(w, http.StatusNotFound, err.Error())
		return
	}
	w.WriteHeader(http.StatusNoContent)
}

type selectFilesRequest struct {
	Indices []int `json:"indices"`
}

// SelectFiles POST /torrents/{id}/select-files  {indices: [0,2,3]}
func SelectFiles(w http.ResponseWriter, r *http.Request, e *torrentengine.Engine) {
	id := mux.Vars(r)["id"]
	var req selectFilesRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		writeError(w, http.StatusBadRequest, "JSON inválido")
		return
	}
	if err := e.SelectFiles(id, req.Indices); err != nil {
		writeError(w, http.StatusNotFound, err.Error())
		return
	}
	w.WriteHeader(http.StatusNoContent)
}

// Peers GET /torrents/{id}/peers
func Peers(w http.ResponseWriter, r *http.Request, e *torrentengine.Engine) {
	id := mux.Vars(r)["id"]
	peers, err := e.Peers(id)
	if err != nil {
		writeError(w, http.StatusNotFound, err.Error())
		return
	}
	if peers == nil {
		peers = []torrentengine.PeerStatus{}
	}
	writeJSON(w, http.StatusOK, peers)
}
