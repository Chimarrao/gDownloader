package integrity

import (
	"encoding/json"
	"net/http"
)

// Handler GET /integrity/check?path=...&expectedSize=... — portado de integrity.rs
func Handler(w http.ResponseWriter, r *http.Request) {
	path := r.URL.Query().Get("path")
	if path == "" {
		http.Error(w, `{"error":"path obrigatório"}`, http.StatusBadRequest)
		return
	}
	var expectedSize uint64
	if s := r.URL.Query().Get("expectedSize"); s != "" {
		// parse as uint64
		var v uint64
		for _, ch := range s {
			if ch < '0' || ch > '9' {
				http.Error(w, `{"error":"expectedSize inválido"}`, http.StatusBadRequest)
				return
			}
			v = v*10 + uint64(ch-'0')
		}
		expectedSize = v
	}
	res := CheckFile(path, expectedSize)
	w.Header().Set("Content-Type", "application/json")
	if res.IsOk() {
		_ = json.NewEncoder(w).Encode(map[string]interface{}{"ok": true})
	} else {
		_ = json.NewEncoder(w).Encode(map[string]interface{}{"ok": false, "reason": res.Reason})
	}
}
