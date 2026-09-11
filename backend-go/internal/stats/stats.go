package stats

import (
	"sync"
	"time"
)

// Portado de backend/src/stats.rs:5 — mantém mesma forma JSON (snake_case)
type RealtimeStats struct {
	Timestamp      uint64            `json:"timestamp"`
	TotalSpeedBps  uint64            `json:"total_speed_bps"`
	PerHostSpeed   map[string]uint64 `json:"per_host_speed"`
}

// StatsManager — ring buffer thread-safe, portado de stats.rs:13
type StatsManager struct {
	mu         sync.RWMutex
	buffer     []RealtimeStats
	bufferSize int
}

func New(bufferSize int) *StatsManager {
	if bufferSize <= 0 {
		bufferSize = 60
	}
	return &StatsManager{
		buffer:     make([]RealtimeStats, 0, bufferSize),
		bufferSize: bufferSize,
	}
}

func (m *StatsManager) RecordStats(totalSpeedBps uint64, perHostSpeed map[string]uint64) {
	if perHostSpeed == nil {
		perHostSpeed = map[string]uint64{}
	}
	// Clone map to avoid external mutation
	clone := make(map[string]uint64, len(perHostSpeed))
	for k, v := range perHostSpeed {
		clone[k] = v
	}
	timestamp := uint64(time.Now().Unix())
	s := RealtimeStats{
		Timestamp:     timestamp,
		TotalSpeedBps: totalSpeedBps,
		PerHostSpeed:  clone,
	}
	m.mu.Lock()
	defer m.mu.Unlock()
	m.buffer = append(m.buffer, s)
	if len(m.buffer) > m.bufferSize {
		// remove oldest (ring behavior)
		copy(m.buffer[0:], m.buffer[1:])
		m.buffer = m.buffer[:m.bufferSize]
	}
}

func (m *StatsManager) GetRealtimeStats() []RealtimeStats {
	m.mu.RLock()
	defer m.mu.RUnlock()
	out := make([]RealtimeStats, len(m.buffer))
	copy(out, m.buffer)
	return out
}

func (m *StatsManager) GetCurrentStats() *RealtimeStats {
	m.mu.RLock()
	defer m.mu.RUnlock()
	if len(m.buffer) == 0 {
		return nil
	}
	last := m.buffer[len(m.buffer)-1]
	// return copy
	c := last
	return &c
}

// Global singleton usado pelo handler e pelo sampler em main.go
var DefaultManager = New(60)
