package captcha

import (
	"database/sql"
	"encoding/json"
	"fmt"
	"net/http"
)

// Portado de backend/src/routes/captcha.rs — serve página de captcha e submit

func CaptchaPage(w http.ResponseWriter, r *http.Request) {
	q := r.URL.Query()
	captchaType := q.Get("type")
	sitekey := q.Get("sitekey")
	pageURL := q.Get("pageurl")
	id := q.Get("id")

	var scriptURL, widgetHTML string
	if captchaType == "hcaptcha" {
		scriptURL = "https://js.hcaptcha.com/1/api.js"
		widgetHTML = fmt.Sprintf(`<div class="h-captcha" data-sitekey="%s" data-callback="onSolved"></div>`, sitekey)
		_ = pageURL
	} else {
		scriptURL = "https://www.google.com/recaptcha/api.js"
		widgetHTML = fmt.Sprintf(`<div class="g-recaptcha" data-sitekey="%s" data-callback="onSolved"></div>`, sitekey)
		_ = id
	}

	html := fmt.Sprintf(`<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8">
  <style>
    * { margin: 0; padding: 0; box-sizing: border-box; }
    body { background: transparent; display: flex; justify-content: center; padding: 8px; }
  </style>
</head>
<body>
  %s
  <script>
    var DOWNLOAD_ID = "%s";
    function onSolved(token) {
      window.parent.postMessage({ type: 'captcha-token', id: DOWNLOAD_ID, token: token }, '*');
    }
  </script>
  <script src="%s" async defer></script>
</body>
</html>`, widgetHTML, id, scriptURL)

	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	w.Write([]byte(html))
}

func SubmitCaptcha(w http.ResponseWriter, r *http.Request, database *sql.DB) {
	var body struct {
		ID    string `json:"id"`
		Token string `json:"token"`
	}
	if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
		http.Error(w, `{"error":"JSON inválido"}`, http.StatusBadRequest)
		return
	}
	if body.ID == "" || body.Token == "" {
		http.Error(w, `{"error":"id e token obrigatórios"}`, http.StatusBadRequest)
		return
	}
	// Atualiza downloads: seta captcha_token e volta para pending
	_, err := database.Exec(`
		UPDATE downloads SET captcha_token = ?, status = 'pending', captcha_type = NULL, captcha_sitekey = NULL, captcha_page_url = NULL, error = NULL, retry_at = NULL WHERE id = ?`,
		body.Token, body.ID)
	if err != nil {
		http.Error(w, `{"error":"Falha ao atualizar download"}`, http.StatusInternalServerError)
		return
	}
	// Também atualiza app_kv se necessário (compatibilidade)
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]bool{"ok": true})
}
