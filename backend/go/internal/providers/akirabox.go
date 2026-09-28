package providers

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
	"strings"
	"time"
)

// Portado de backend/src/providers/akirabox.rs — AkiraBox via Electron helper proxy.

type AkiraboxProvider struct{}

type AkiraboxHelperJobStatus struct {
	Status         string  `json:"status"`
	BytesDownloaded uint64 `json:"bytesDownloaded"`
	TotalBytes     uint64 `json:"totalBytes"`
	SpeedBps       uint64 `json:"speedBps"`
	EtaSecs        uint64 `json:"etaSecs"`
	Filename       *string `json:"filename"`
	Error          *string `json:"error"`
}

func (p AkiraboxProvider) Matches(rawURL string) bool {
	if !HostMatches(rawURL, []string{"akirabox.to", "www.akirabox.to"}) {
		return false
	}
	segs := PathSegments(rawURL)
	if len(segs) < 2 {
		return false
	}
	return segs[1] == "file"
}

func akiraboxProxyPort() string {
	return os.Getenv("AKIRABOX_PROXY_PORT")
}

func akiraboxHelperToken() string {
	return strings.TrimSpace(os.Getenv("GDOWNLOADER_HELPER_TOKEN"))
}

func akiraboxProxyAction(payload interface{}) (map[string]interface{}, error) {
	port := akiraboxProxyPort()
	if port == "" {
		return nil, fmt.Errorf("Helper local do AkiraBox não disponível")
	}
	token := akiraboxHelperToken()
	if token == "" {
		return nil, fmt.Errorf("Token do helper local do AkiraBox não disponível")
	}
	body, _ := json.Marshal(payload)
	req, _ := http.NewRequest("POST", fmt.Sprintf("http://127.0.0.1:%s/", port), bytes.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("X-GDownloader-Token", token)
	client := &http.Client{Timeout: 180 * time.Second}
	resp, err := client.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	b, _ := io.ReadAll(resp.Body)
	var out map[string]interface{}
	if err := json.Unmarshal(b, &out); err != nil {
		return nil, err
	}
	return out, nil
}

func (p AkiraboxProvider) HelperFileInfo(rawURL string) (map[string]interface{}, error) {
	return akiraboxProxyAction(map[string]interface{}{"action": "akirabox_file_info", "url": rawURL})
}

func (p AkiraboxProvider) HelperStartDownload(sourceURL, destPath string) (string, error) {
	result, err := akiraboxProxyAction(map[string]interface{}{"action": "akirabox_download_file", "url": sourceURL, "destPath": destPath})
	if err != nil {
		return "", err
	}
	jobID, _ := result["jobId"].(string)
	if jobID == "" {
		return "", fmt.Errorf("Helper local do AkiraBox não retornou um jobId")
	}
	return jobID, nil
}

func (p AkiraboxProvider) HelperJobStatus(jobID string) (*AkiraboxHelperJobStatus, error) {
	result, err := akiraboxProxyAction(map[string]interface{}{"action": "akirabox_job_status", "jobId": jobID})
	if err != nil {
		return nil, err
	}
	b, _ := json.Marshal(result)
	var status AkiraboxHelperJobStatus
	if err := json.Unmarshal(b, &status); err != nil {
		return nil, err
	}
	return &status, nil
}

func (p AkiraboxProvider) FetchFileInfo(rawURL string) (string, uint64, error) {
	info, err := p.HelperFileInfo(rawURL)
	if err != nil {
		return "", 0, err
	}
	// Expect fields filename and size
	filename, _ := info["filename"].(string)
	if filename == "" {
		filename = "arquivo_akirabox"
	}
	var size uint64
	switch v := info["size"].(type) {
	case float64:
		size = uint64(v)
	case int:
		size = uint64(v)
	}
	return SanitizeFilename(filename, "arquivo_akirabox"), size, nil
}
