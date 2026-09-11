package system

import (
	"encoding/json"
	"net/http"
	"os"
	"path/filepath"
	"runtime"
	"syscall"
)

// Portado de backend/src/routes/system.rs — DiskUsage e DiskEntry mantêm mesma forma JSON

type DiskUsage struct {
	Total     uint64 `json:"total"`
	Available uint64 `json:"available"`
	Used      uint64 `json:"used"`
	Mount     string `json:"mount"`
}

type DiskEntry struct {
	Name      string `json:"name"`
	Mount     string `json:"mount"`
	Total     uint64 `json:"total"`
	Available uint64 `json:"available"`
	Used      uint64 `json:"used"`
	Removable bool   `json:"removable"`
	Kind      string `json:"kind"`
	ReadBps   uint64 `json:"readBps"`
	WriteBps  uint64 `json:"writeBps"`
}

// statvfsUsage portado de system.rs:216 — retorna total, available via syscall.Statfs
func statvfsUsage(path string) (uint64, uint64, bool) {
	var stat syscall.Statfs_t
	if err := syscall.Statfs(path, &stat); err != nil {
		return 0, 0, false
	}
	// prefer Frsize if available, else Bsize
	var blockSize uint64
	// Statfs_t fields differ per OS: use Bsize as block size
	blockSize = uint64(stat.Bsize)
	if blockSize == 0 {
		blockSize = 4096
	}
	total := uint64(stat.Blocks) * blockSize
	available := uint64(stat.Bavail) * blockSize
	return total, available, true
}

func mountPointFor(path string) string {
	// Best-effort: resolve absolute path and try to return its mount via Statfs comparison
	// Simplificado: retorna "/" se não conseguir resolver melhor
	if path == "" {
		return "/"
	}
	// Walk up path until we find where device changes — simplificado retorna raiz
	abs, err := filepath.Abs(path)
	if err != nil {
		return "/"
	}
	// On macOS/Linux we could compare device IDs but keeping simple
	// Return "/" for now; if path is under /Volumes/* on macOS maybe return that prefix
	if runtime.GOOS == "darwin" && len(abs) > 9 && abs[:9] == "/Volumes/" {
		parts := filepath.SplitList(abs) // not needed
		_ = parts
		// extract /Volumes/<name>
		sep := len("/Volumes/")
		rest := abs[sep:]
		if idx := indexByte(rest, '/'); idx >= 0 {
			return abs[:sep+idx]
		}
		return abs
	}
	return "/"
}

func indexByte(s string, c byte) int {
	for i := 0; i < len(s); i++ {
		if s[i] == c {
			return i
		}
	}
	return -1
}

// Handler GET /system/disk?path=... — portado de system.rs:90
func DiskUsageHandler(w http.ResponseWriter, r *http.Request) {
	q := r.URL.Query()
	target := q.Get("path")
	if target == "" {
		if home := os.Getenv("HOME"); home != "" {
			target = home
		} else {
			target = "/"
		}
	}
	total, available, ok := statvfsUsage(target)
	var mount string
	var used uint64
	if ok {
		mount = mountPointFor(target)
		used = total - min(total, available)
		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(DiskUsage{
			Total:     total,
			Available: available,
			Used:      used,
			Mount:     mount,
		})
		return
	}
	// Fallback: try "/"
	total, available, _ = statvfsUsage("/")
	mount = "/"
	used = total - min(total, available)
	w.Header().Set("Content-Type", "application/json")
	_ = json.NewEncoder(w).Encode(DiskUsage{
		Total:     total,
		Available: available,
		Used:      used,
		Mount:     mount,
	})
}

func min(a, b uint64) uint64 {
	if a < b {
		return a
	}
	return b
}

// Handler GET /system/disks — portado de system.rs:152
func ListDisksHandler(w http.ResponseWriter, r *http.Request) {
	// Para manter vet compatível, retornamos ao menos o volume raiz com dados de statvfs
	// O Rust enumera todos os discos via sysinfo; aqui simplificamos para raiz + /Volumes/* em macOS
	entries := []DiskEntry{}

	// Root
	if total, avail, ok := statvfsUsage("/"); ok {
		entries = append(entries, DiskEntry{
			Name:      "/",
			Mount:     "/",
			Total:     total,
			Available: avail,
			Used:      total - min(total, avail),
			Removable: false,
			Kind:      "SSD",
			ReadBps:   0,
			WriteBps:  0,
		})
	}

	// On macOS, enumerate /Volumes/*
	if runtime.GOOS == "darwin" {
		if infos, err := os.ReadDir("/Volumes"); err == nil {
			for _, info := range infos {
				mount := filepath.Join("/Volumes", info.Name())
				if total, avail, ok := statvfsUsage(mount); ok && total > 0 {
					// Avoid duplicating root if same device
					dup := false
					for _, e := range entries {
						if e.Total == total && e.Available == avail {
							dup = true
							break
						}
						if e.Mount == mount {
							dup = true
							break
						}
					}
					if dup {
						continue
					}
					entries = append(entries, DiskEntry{
						Name:      info.Name(),
						Mount:     mount,
						Total:     total,
						Available: avail,
						Used:      total - min(total, avail),
						Removable: true,
						Kind:      "Desconhecido",
						ReadBps:   0,
						WriteBps:  0,
					})
				}
			}
		}
	}

	// On Linux, try to parse /proc/mounts as fallback for more entries
	if runtime.GOOS == "linux" && len(entries) == 1 {
		// Read /proc/mounts to discover additional mounts (best-effort)
		if data, err := os.ReadFile("/proc/mounts"); err == nil {
			seen := map[string]bool{"/": true}
			lines := splitLines(string(data))
			for _, line := range lines {
				fields := splitFields(line)
				if len(fields) < 2 {
					continue
				}
				mount := fields[1]
				if seen[mount] {
					continue
				}
				// Skip virtual filesystems
				if mount == "/proc" || mount == "/sys" || mount == "/dev" || mount == "/run" {
					continue
				}
				if total, avail, ok := statvfsUsage(mount); ok && total > 0 {
					seen[mount] = true
					entries = append(entries, DiskEntry{
						Name:      filepath.Base(mount),
						Mount:     mount,
						Total:     total,
						Available: avail,
						Used:      total - min(total, avail),
						Removable: false,
						Kind:      "Desconhecido",
						ReadBps:   0,
						WriteBps:  0,
					})
				}
			}
		}
	}

	// Sort by total descending (like Rust)
	for i := 0; i < len(entries); i++ {
		for j := i + 1; j < len(entries); j++ {
			if entries[j].Total > entries[i].Total {
				entries[i], entries[j] = entries[j], entries[i]
			}
		}
	}

	w.Header().Set("Content-Type", "application/json")
	_ = json.NewEncoder(w).Encode(entries)
}

func splitLines(s string) []string {
	var out []string
	start := 0
	for i := 0; i < len(s); i++ {
		if s[i] == '\n' {
			out = append(out, s[start:i])
			start = i + 1
		}
	}
	if start < len(s) {
		out = append(out, s[start:])
	}
	return out
}

func splitFields(s string) []string {
	var fields []string
	start := -1
	for i, c := range s {
		if c == ' ' || c == '\t' {
			if start != -1 {
				fields = append(fields, s[start:i])
				start = -1
			}
		} else if start == -1 {
			start = i
		}
	}
	if start != -1 {
		fields = append(fields, s[start:])
	}
	return fields
}
