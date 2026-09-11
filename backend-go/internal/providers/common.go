package providers

import (
	"fmt"
	"net/url"
	"regexp"
	"strconv"
	"strings"
)

// Portado de backend/src/providers/mod.rs — helpers compartilhados

const DefaultUserAgent = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/136 Safari/537.36"
const DefaultAcceptLanguage = "pt-BR,pt;q=0.9,en;q=0.8"

type ProviderCapabilities struct {
	MaxParallelDownloadsFree     *int  `json:"maxParallelDownloadsFree,omitempty"`
	RequiresBrowserHelper        bool  `json:"requiresBrowserHelper"`
	SupportsFolder               bool  `json:"supportsFolder"`
	SupportsManualAuth           bool  `json:"supportsManualAuth"`
	SupportsAutoCaptcha          bool  `json:"supportsAutoCaptcha"`
	FreeCooldownSecs             *uint64 `json:"freeCooldownSecs,omitempty"`
	RequiresAccountForLargeFiles bool  `json:"requiresAccountForLargeFiles"`
	SupportsParallelParts        bool  `json:"supportsParallelParts"`
}

type ProviderDescriptor struct {
	ID           string               `json:"id"`
	Name         string               `json:"name"`
	Color        string               `json:"color"`
	Capabilities ProviderCapabilities `json:"capabilities"`
}

func SanitizeFilename(name, fallback string) string {
	trimmed := strings.TrimSpace(name)
	candidate := trimmed
	if candidate == "" {
		candidate = fallback
	}
	var b strings.Builder
	for _, ch := range candidate {
		switch ch {
		case '/', '\\', ':', '*', '?', '"', '<', '>', '|':
			b.WriteRune('_')
		default:
			b.WriteRune(ch)
		}
	}
	return strings.TrimSpace(b.String())
}

func ParseHumanSize(value string) uint64 {
	normalized := strings.ToUpper(strings.ReplaceAll(strings.TrimSpace(value), ",", "."))
	re := regexp.MustCompile(`([0-9]+(?:\.[0-9]+)?)\s*(B|KB|MB|GB|TB)`)
	m := re.FindStringSubmatch(normalized)
	if len(m) < 3 {
		return 0
	}
	num, _ := strconv.ParseFloat(m[1], 64)
	var mult float64
	switch m[2] {
	case "KB":
		mult = 1024
	case "MB":
		mult = 1024 * 1024
	case "GB":
		mult = 1024 * 1024 * 1024
	case "TB":
		mult = 1024 * 1024 * 1024 * 1024
	default:
		mult = 1
	}
	return uint64(num * mult)
}

func ParseURL(raw string) *url.URL {
	clean := strings.Split(raw, "#")[0]
	u, err := url.Parse(clean)
	if err != nil {
		return nil
	}
	return u
}

func HostMatches(raw string, hosts []string) bool {
	u := ParseURL(raw)
	if u == nil {
		return false
	}
	host := strings.ToLower(u.Hostname())
	for _, h := range hosts {
		if host == h {
			return true
		}
	}
	return false
}

func PathSegments(raw string) []string {
	u := ParseURL(raw)
	if u == nil {
		return nil
	}
	parts := strings.Split(strings.Trim(u.Path, "/"), "/")
	var out []string
	for _, p := range parts {
		if p != "" {
			out = append(out, p)
		}
	}
	return out
}

func providerIDFromName(name string) string {
	switch name {
	case "Mega":
		return "mega"
	case "MediaFire":
		return "mediafire"
	case "Google Drive":
		return "gdrive"
	case "PixelDrain":
		return "pixeldrain"
	case "Gofile":
		return "gofile"
	case "1Fichier":
		return "fichier"
	case "Drime":
		return "drime"
	case "Rapidgator":
		return "rapidgator"
	case "BRUpload":
		return "brupload"
	case "BRFiles":
		return "brfiles"
	case "MoonDL":
		return "moondl"
	case "AkiraBox":
		return "akirabox"
	case "Katfile":
		return "katfile"
	case "Terabox":
		return "terabox"
	case "Transfer.it":
		return "transferit"
	case "Send.now":
		return "sendnow"
	case "Internet Archive":
		return "internetarchive"
	case "YouTube":
		return "youtube"
	case "OneDrive":
		return "onedrive"
	case "Direct HTTP":
		return "direct_http"
	default:
		return "unknown"
	}
}

func capabilitiesForProviderName(name string) ProviderCapabilities {
	switch name {
	case "BRFiles":
		return ProviderCapabilities{MaxParallelDownloadsFree: intPtr(1), FreeCooldownSecs: u64Ptr(3600), SupportsParallelParts: true}
	case "MoonDL":
		return ProviderCapabilities{SupportsAutoCaptcha: true, FreeCooldownSecs: u64Ptr(3600), MaxParallelDownloadsFree: intPtr(1), SupportsParallelParts: true}
	case "1Fichier":
		return ProviderCapabilities{FreeCooldownSecs: u64Ptr(300), MaxParallelDownloadsFree: intPtr(1), SupportsParallelParts: true}
	case "AkiraBox":
		return ProviderCapabilities{RequiresBrowserHelper: true, SupportsAutoCaptcha: true, MaxParallelDownloadsFree: intPtr(1), SupportsParallelParts: true}
	case "Katfile":
		return ProviderCapabilities{RequiresBrowserHelper: true, SupportsAutoCaptcha: true, MaxParallelDownloadsFree: intPtr(1), SupportsParallelParts: true}
	case "Terabox":
		return ProviderCapabilities{RequiresBrowserHelper: true, SupportsFolder: true, SupportsManualAuth: true, MaxParallelDownloadsFree: intPtr(1), SupportsParallelParts: true}
	case "MediaFire":
		return ProviderCapabilities{SupportsFolder: true, SupportsParallelParts: true}
	case "Mega":
		return ProviderCapabilities{SupportsFolder: true, MaxParallelDownloadsFree: intPtr(1), FreeCooldownSecs: u64Ptr(1800), SupportsParallelParts: true}
	case "Google Drive":
		return ProviderCapabilities{SupportsFolder: true, SupportsParallelParts: true}
	case "YouTube":
		return ProviderCapabilities{SupportsFolder: true, SupportsParallelParts: false}
	case "Send.now":
		return ProviderCapabilities{SupportsFolder: true, SupportsParallelParts: false}
	case "Internet Archive":
		return ProviderCapabilities{SupportsFolder: true, SupportsParallelParts: true}
	case "Rapidgator":
		return ProviderCapabilities{SupportsAutoCaptcha: true, FreeCooldownSecs: u64Ptr(3600), MaxParallelDownloadsFree: intPtr(1), SupportsParallelParts: true}
	case "BRUpload":
		return ProviderCapabilities{SupportsAutoCaptcha: true, FreeCooldownSecs: u64Ptr(60), MaxParallelDownloadsFree: intPtr(1), SupportsParallelParts: true}
	default:
		return ProviderCapabilities{SupportsParallelParts: true}
	}
}

func ExtractWaitSecondsFromText(value string) *uint64 {
	lower := strings.ToLower(value)
	patterns := []struct {
		re  string
		mul uint64
	}{
		{`(\d+)\s*(?:hora|horas|hour|hours)`, 3600},
		{`(\d+)\s*(?:minuto|minutos|minute|minutes)`, 60},
		{`(\d+)\s*(?:segundo|segundos|second|seconds)`, 1},
	}
	var total uint64
	matched := false
	for _, p := range patterns {
		re := regexp.MustCompile(p.re)
		for _, m := range re.FindAllStringSubmatch(lower, -1) {
			var n uint64
			if _, err := fmt.Sscanf(m[1], "%d", &n); err == nil {
				total += n * p.mul
				matched = true
			}
		}
	}
	if matched && total > 0 {
		return &total
	}
	if re := regexp.MustCompile(`(?:wait|aguarde|try again in)\s*(\d+)`); re != nil {
		if m := re.FindStringSubmatch(lower); len(m) > 1 {
			var n uint64
			if _, err := fmt.Sscanf(m[1], "%d", &n); err == nil {
				return &n
			}
		}
	}
	return nil
}

func intPtr(v int) *int { return &v }
func u64Ptr(v uint64) *uint64 { return &v }

func AllProviderDescriptors() []ProviderDescriptor {
	names := []struct {
		name  string
		color string
	}{
		{"Mega", "#e84d3d"},
		{"MediaFire", "#0062C7"},
		{"Google Drive", "#4285F4"},
		{"PixelDrain", "#ff6600"},
		{"Gofile", "#1a1d29"},
		{"1Fichier", "#e67e22"},
		{"Drime", "#2ec4b6"},
		{"Rapidgator", "#23a2dc"},
		{"BRUpload", "#f97316"},
		{"BRFiles", "#22c55e"},
		{"MoonDL", "#64748b"},
		{"AkiraBox", "#0f172a"},
		{"Katfile", "#2563eb"},
		{"Terabox", "#2a6df5"},
		{"Transfer.it", "#1D81FF"},
		{"Send.now", "#d9ecff"},
		{"Internet Archive", "#5d6472"},
		{"YouTube", "#FF0000"},
		{"OneDrive", "#0a66d9"},
		{"Direct HTTP", "#0f766e"},
	}
	var out []ProviderDescriptor
	for _, n := range names {
		out = append(out, ProviderDescriptor{
			ID: n.name, // temp
			Name: n.name,
			Color: n.color,
			Capabilities: capabilitiesForProviderName(n.name),
		})
		for i := range out {
			out[i].ID = providerIDFromName(out[i].Name)
		}
	}
	return out
}
