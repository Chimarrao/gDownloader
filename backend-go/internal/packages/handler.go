package packages

import (
	"database/sql"
	"encoding/json"
	"net/http"
	"time"

	"github.com/google/uuid"
	"github.com/gorilla/mux"
	"gdownloader-go/internal/db"
	"gdownloader-go/internal/models"
)

type createPackageRequest struct {
	Name            string  `json:"name"`
	Color           *string `json:"color"`
	Comment         *string `json:"comment"`
	DestDirOverride *string `json:"destDirOverride"`
	Priority        *int32  `json:"priority"`
}

func writeJSON(w http.ResponseWriter, status int, v interface{}) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(v)
}

func writeError(w http.ResponseWriter, status int, msg string) {
	writeJSON(w, status, map[string]string{"error": msg})
}

// ListPackages GET /packages
func ListPackages(w http.ResponseWriter, r *http.Request, database *sql.DB) {
	items, err := db.ListPackages(database)
	if err != nil {
		writeError(w, http.StatusInternalServerError, err.Error())
		return
	}
	writeJSON(w, http.StatusOK, items)
}

// CreatePackage POST /packages
func CreatePackage(w http.ResponseWriter, r *http.Request, database *sql.DB) {
	var req createPackageRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		writeError(w, http.StatusBadRequest, "JSON inválido")
		return
	}
	if req.Name == "" {
		writeError(w, http.StatusBadRequest, "nome obrigatório")
		return
	}
	id := uuid.New().String()
	now := uint64(time.Now().Unix())
	color := "#7c6fff"
	if req.Color != nil && *req.Color != "" {
		color = *req.Color
	}
	var priority int32
	if req.Priority != nil {
		priority = *req.Priority
	}
	pkg := models.Package{
		ID:              id,
		Name:            req.Name,
		Color:           color,
		Comment:         req.Comment,
		DestDirOverride: req.DestDirOverride,
		Priority:        priority,
		CreatedAt:       now,
	}
	if err := db.InsertPackage(database, pkg); err != nil {
		writeError(w, http.StatusInternalServerError, err.Error())
		return
	}
	writeJSON(w, http.StatusOK, pkg)
}

// DeletePackage DELETE /packages/{id}
func DeletePackage(w http.ResponseWriter, r *http.Request, database *sql.DB) {
	vars := mux.Vars(r)
	id := vars["id"]
	if id == "" {
		id = r.URL.Query().Get("id")
	}
	if id == "" {
		// fallback extract from path
		writeError(w, http.StatusBadRequest, "id obrigatório")
		return
	}
	if err := db.DeletePackage(database, id); err != nil {
		writeError(w, http.StatusInternalServerError, err.Error())
		return
	}
	w.WriteHeader(http.StatusNoContent)
}

// AssignDownloadToPackage POST /packages/{package_id}/assign/{download_id}
func AssignDownloadToPackage(w http.ResponseWriter, r *http.Request, database *sql.DB) {
	vars := mux.Vars(r)
	pkgID := vars["package_id"]
	dlID := vars["download_id"]
	if pkgID == "" || dlID == "" {
		writeError(w, http.StatusBadRequest, "parâmetros obrigatórios")
		return
	}
	if err := db.AssignDownloadToPackage(database, pkgID, dlID); err != nil {
		writeError(w, http.StatusInternalServerError, err.Error())
		return
	}
	w.WriteHeader(http.StatusNoContent)
}

// UnassignDownloadFromPackage DELETE /packages/unassign/{download_id}
func UnassignDownloadFromPackage(w http.ResponseWriter, r *http.Request, database *sql.DB) {
	vars := mux.Vars(r)
	dlID := vars["download_id"]
	if dlID == "" {
		writeError(w, http.StatusBadRequest, "download_id obrigatório")
		return
	}
	if err := db.UnassignDownloadFromPackage(database, dlID); err != nil {
		writeError(w, http.StatusInternalServerError, err.Error())
		return
	}
	w.WriteHeader(http.StatusNoContent)
}
