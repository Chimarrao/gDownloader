package mirrors

import (
	"encoding/json"
	"fmt"
	"net/http"
	"strings"
	"time"
)

// Portado de backend/src/routes/mirrors.rs — SSE streaming de busca de mirrors

type searcherDescriptor struct {
	Name        string
	Key         string
	DelayMs     int
	TimeoutSecs int
}

var searchers = []searcherDescriptor{
	{"DuckDuckGo", "duckduckgo", 1100, 45},
	{"Bing", "bing", 1100, 45},
	{"Yandex", "yandex", 1100, 45},
	{"Google (open dir)", "google_opendir", 1100, 45},
	{"Google (hosters)", "google_hosters", 1100, 45},
	{"Pixeldrain API", "pixeldrain", 1100, 45},
	{"Gofile API", "gofile", 1100, 45},
	{"Archive.org", "archive_org", 1100, 45},
	{"MediaFire", "mediafire", 1100, 45},
	{"1Fichier (dork)", "1fichier", 1100, 45},
	{"Rapidgator (dork)", "rapidgator", 1100, 45},
	{"Mega (dork)", "mega", 1100, 45},
	{"FileSearching", "filesearching", 1100, 45},
}

func isSearchEngineURL(rawURL string) bool {
	lower := strings.ToLower(rawURL)
	return strings.Contains(lower, "google.com") ||
		strings.Contains(lower, "bing.com") ||
		strings.Contains(lower, "duckduckgo.com") ||
		strings.Contains(lower, "search.yahoo.com") ||
		strings.Contains(lower, "yandex.")
}

// Handler GET /mirrors/search?filename=... — SSE
func Handler(w http.ResponseWriter, r *http.Request) {
	filename := r.URL.Query().Get("filename")
	if strings.TrimSpace(filename) == "" {
		http.Error(w, `{"error":"filename obrigatório"}`, http.StatusBadRequest)
		return
	}

	w.Header().Set("Content-Type", "text/event-stream")
	w.Header().Set("Cache-Control", "no-cache")
	w.Header().Set("Connection", "keep-alive")
	w.Header().Set("X-Accel-Buffering", "no")

	flusher, ok := w.(http.Flusher)
	if !ok {
		http.Error(w, "SSE não suportado", http.StatusInternalServerError)
		return
	}

	slug, ext := NormalizeFilename(filename)
	uploader := ExtractUploaderTag(slug)
	client := BuildClient()

	total := len(searchers)
	seenURLs := map[string]bool{}
	var allResults []MirrorResult

	startedAt := time.Now()

	sendEvent(w, eventStart(filename, total))
	sendEvent(w, eventLog(fmt.Sprintf("════════════════════════════════════════════════\n  Arquivo: %s\n  Busca:   %s%s\n════════════════════════════════════════════════", filename, slug, func() string {
		if uploader != nil {
			return fmt.Sprintf("  [uploader: %s]", *uploader)
		}
		return ""
	}())))

	flusher.Flush()

	for idx, desc := range searchers {
		pad := fmt.Sprintf("[%2d/%d]", idx+1, total)
		sendEvent(w, eventProgress(idx+1, total, desc.Name, "running", 0, len(seenURLs), 0, 0, 0, nil))
		sendEvent(w, eventLog(fmt.Sprintf("%s %s...", pad, desc.Name)))
		flusher.Flush()

		var rawResults []MirrorResult
		var errReason *string

		// Run with timeout
		done := make(chan []MirrorResult, 1)
		go func(key string) {
			var res []MirrorResult
			switch key {
			case "duckduckgo":
				res = SearchDuckDuckGo(client, slug, uploader)
			case "bing":
				res = SearchBing(client, slug, uploader, ext)
			case "yandex":
				res = SearchYandex(client, slug, uploader)
			case "google_opendir":
				res = SearchGoogleOpendir(client, slug, ext)
			case "google_hosters":
				res = SearchGoogleHosters(client, slug, uploader)
			case "pixeldrain":
				res = SearchPixeldrain(client, slug)
			case "gofile":
				res = SearchGofile(client, slug)
			case "archive_org":
				res = SearchArchiveOrg(client, slug)
			case "mediafire":
				res = SearchMediafire(client, slug)
			case "1fichier":
				res = Search1Fichier(client, slug, uploader)
			case "rapidgator":
				res = SearchRapidgator(client, slug, uploader)
			case "mega":
				res = SearchMega(client, slug, uploader)
			case "filesearching":
				res = SearchFilesearching(client, slug)
			}
			done <- res
		}(desc.Key)

		select {
		case res := <-done:
			rawResults = res
		case <-time.After(time.Duration(desc.TimeoutSecs) * time.Second):
			reason := "tempo limite excedido, pulando para o próximo"
			errReason = &reason
			sendEvent(w, eventLog(fmt.Sprintf("%s %s — %s", pad, desc.Name, *errReason)))
			sendEvent(w, eventProgress(idx+1, total, desc.Name, "completed", 0, len(seenURLs), 0, 0, 0, errReason))
			flusher.Flush()
			time.Sleep(time.Duration(desc.DelayMs) * time.Millisecond)
			continue
		}

		rawCount := len(rawResults)
		// Filter and score
		var filtered []MirrorResult
		for _, res := range rawResults {
			if isSearchEngineURL(res.URL) {
				continue
			}
			if !IsRelevantResult(&res, slug, uploader) {
				continue
			}
			res.Score = ScoreResult(&res, slug, uploader)
			if res.Score < 8 {
				continue
			}
			filtered = append(filtered, res)
		}
		rejectedCount := rawCount - len(filtered)
		var newResults []MirrorResult
		for _, res := range filtered {
			if !seenURLs[res.URL] {
				newResults = append(newResults, res)
			}
		}

		if len(newResults) == 0 {
			sendEvent(w, eventLog(fmt.Sprintf("%s %s — nada encontrado", pad, desc.Name)))
		} else {
			sendEvent(w, eventLog(fmt.Sprintf("%s %s ✓ %d resultado(s)", pad, desc.Name, len(newResults))))
			for _, res := range newResults {
				seenURLs[res.URL] = true
				hosterTag := ""
				if res.Hoster != nil {
					hosterTag = fmt.Sprintf(" [%s]", *res.Hoster)
				}
				magTag := ""
				if strings.HasPrefix(res.URL, "magnet:?") {
					magTag = " [torrent]"
				}
				sendEvent(w, eventLog(fmt.Sprintf("    → %s%s%s", res.URL, hosterTag, magTag)))
				sendEvent(w, eventResult(res.Source, res.URL, res.Hoster, res.Score))
				allResults = append(allResults, res)
			}
			sendEvent(w, eventProgress(idx+1, total, desc.Name, "completed", len(newResults), len(seenURLs), rawCount, rejectedCount, 0, nil))
			flusher.Flush()
			time.Sleep(time.Duration(desc.DelayMs) * time.Millisecond)
			continue
		}

		sendEvent(w, eventProgress(idx+1, total, desc.Name, "completed", len(newResults), len(seenURLs), rawCount, rejectedCount, 0, nil))
		flusher.Flush()
		time.Sleep(time.Duration(desc.DelayMs) * time.Millisecond)

		// Check client disconnect
		select {
		case <-r.Context().Done():
			return
		default:
		}
	}

	hosters := 0
	for _, res := range allResults {
		if res.Hoster != nil {
			hosters++
		}
	}
	summary := fmt.Sprintf("════════════════════════════════════════════════\n  %d link(s) encontrado(s)  |  hosters: %d  |  duração: %.1fs\n════════════════════════════════════════════════", len(allResults), hosters, time.Since(startedAt).Seconds())
	sendEvent(w, eventLog(summary))
	sendEvent(w, eventDone(filename, total, len(allResults), hosters, uint64(time.Since(startedAt).Milliseconds())))
	flusher.Flush()
}

func sendEvent(w http.ResponseWriter, data string) {
	fmt.Fprintf(w, "data: %s\n\n", data)
}

func eventStart(filename string, total int) string {
	b, _ := json.Marshal(map[string]interface{}{"type": "start", "filename": filename, "total": total})
	return string(b)
}

func eventProgress(current, total int, searcher, phase string, newResults, totalResults, rawResults, rejectedResults int, durationMs int, err *string) string {
	var errVal interface{} = nil
	if err != nil {
		errVal = *err
	}
	b, _ := json.Marshal(map[string]interface{}{
		"type": "progress", "current": current, "total": total, "searcher": searcher, "phase": phase,
		"newResults": newResults, "totalResults": totalResults, "rawResults": rawResults, "rejectedResults": rejectedResults,
		"durationMs": durationMs, "error": errVal,
	})
	return string(b)
}

func eventLog(msg string) string {
	b, _ := json.Marshal(map[string]interface{}{"type": "log", "payload": msg})
	return string(b)
}

func eventResult(source, rawURL string, hoster *string, score int) string {
	b, _ := json.Marshal(map[string]interface{}{"type": "result", "url": rawURL, "source": source, "hoster": hoster, "score": score})
	return string(b)
}

func eventDone(filename string, searchers, total, hosters int, durationMs uint64) string {
	b, _ := json.Marshal(map[string]interface{}{"type": "done", "filename": filename, "searchers": searchers, "total": total, "hosters": hosters, "durationMs": durationMs})
	return string(b)
}
