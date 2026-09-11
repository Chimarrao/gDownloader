package stats

import (
	"encoding/json"
	"net/http"
)

// Handler portado de backend/src/routes/stats.rs:4 — GET /stats/realtime {ticks: RealtimeStats[]}
func Handler(w http.ResponseWriter, r *http.Request) {
	ticks := DefaultManager.GetRealtimeStats()
	if ticks == nil {
		ticks = []RealtimeStats{}
	}
	w.Header().Set("Content-Type", "application/json")
	_ = json.NewEncoder(w).Encode(map[string]interface{}{
		"ticks": ticks,
	})
}

// Variant that allows injecting a manager (used in tests)
func HandlerWithManager(w http.ResponseWriter, r *http.Request, m *StatsManager) {
	ticks := m.GetRealtimeStats()
	if ticks == nil {
		ticks = []RealtimeStats{}
	}
	w.Header().Set("Content-Type", "application/json")
	_ = json.NewEncoder(w).Encode(map[string]interface{}{
		"ticks": ticks,
	})
}
