package hashverify

import (
	"encoding/json"
	"net/http"

	"gdownloader-go/internal/models"
)

// Handler POST /hash/verify {path, expected: {algorithm, value}} — portado de hash_verify.rs
func Handler(w http.ResponseWriter, r *http.Request) {
	var req struct {
		Path     string             `json:"path"`
		Expected models.ExpectedHash `json:"expected"`
	}
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, `{"error":"JSON inválido"}`, http.StatusBadRequest)
		return
	}
	if req.Path == "" || req.Expected.Value == "" {
		http.Error(w, `{"error":"path e expected são obrigatórios"}`, http.StatusBadRequest)
		return
	}
	// Para resposta SSE-like streaming usamos header e enviamos progress via chunked? Simplificamos para resposta final síncrona
	result, err := VerifyFile(req.Path, req.Expected, nil)
	if err != nil {
		http.Error(w, `{"error":"`+err.Error()+`"}`, http.StatusInternalServerError)
		return
	}
	w.Header().Set("Content-Type", "application/json")
	_ = json.NewEncoder(w).Encode(result)
}
