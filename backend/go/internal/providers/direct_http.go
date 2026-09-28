package providers

import (
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"time"
)

// Portado de backend/src/providers/direct_http.rs — Range resume + merge_parts

type DirectHTTPProvider struct{}

type DirectHTTPMetadata struct {
	URL           string
	Filename      string
	Size          uint64
	MimeType      *string
	AcceptsRanges bool
	ETag          *string
	LastModified  *string
}

type PartRange struct {
	Index int
	Start uint64
	End   uint64
}

// Throttle constants portadas de direct_http.rs:47
const resumeSaveInterval = 2 * time.Second
const resumeSaveBytes = 8 * 1024 * 1024
const progressReportInterval = 120 * time.Millisecond

// ResumeStore simplificado — em Go compartilhamos o mesmo arquivo SQLite em WAL, mas para simplicidade usamos filesystem
type ResumeStore struct {
	DBPath      *string
	DownloadKey string
	URL         string
	ETag        *string
	LastModified *string
	mu          sync.Mutex
	offsets     map[int]uint64
}

func (p DirectHTTPProvider) Matches(rawURL string) bool {
	u := ParseURL(rawURL)
	if u == nil {
		return false
	}
	return u.Scheme == "http" || u.Scheme == "https"
}

func (p DirectHTTPProvider) CleanURL(rawURL string) string {
	if idx := strings.Index(rawURL, "#"); idx >= 0 {
		rawURL = rawURL[:idx]
	}
	return strings.TrimSpace(rawURL)
}

func (p DirectHTTPProvider) DownloadKey(rawURL, destPath string) string {
	return p.CleanURL(rawURL) + "\n" + destPath
}

func headerString(headers http.Header, name string) *string {
	v := headers.Get(name)
	v = strings.TrimSpace(v)
	if v == "" {
		return nil
	}
	return &v
}

func filenameFromContentDisposition(value string) *string {
	for _, part := range strings.Split(value, ";") {
		part = strings.TrimSpace(part)
		lower := strings.ToLower(part)
		if strings.HasPrefix(lower, "filename*=") {
			eq := strings.Index(part, "=")
			if eq < 0 {
				continue
			}
			raw := strings.Trim(strings.TrimSpace(part[eq+1:]), "\"")
			if strings.HasPrefix(raw, "UTF-8''") {
				raw = raw[7:]
			}
			decoded, err := url.QueryUnescape(raw)
			if err == nil {
				raw = decoded
			}
			if strings.TrimSpace(raw) != "" {
				return &raw
			}
		}
		if strings.HasPrefix(lower, "filename=") {
			eq := strings.Index(part, "=")
			if eq < 0 {
				continue
			}
			filename := strings.Trim(strings.TrimSpace(part[eq+1:]), "\"")
			if strings.TrimSpace(filename) != "" {
				return &filename
			}
		}
	}
	return nil
}

func filenameFromURL(rawURL string) string {
	u := ParseURL(rawURL)
	if u != nil {
		segments := strings.Split(strings.Trim(u.Path, "/"), "/")
		if len(segments) > 0 {
			last := segments[len(segments)-1]
			if decoded, err := url.PathUnescape(last); err == nil {
				last = decoded
			}
			safe := SanitizeFilename(last, "download")
			if safe != "download" && strings.TrimSpace(safe) != "" {
				return safe
			}
		}
	}
	return "download"
}

func capturedHeaders(raw map[string]string) http.Header {
	h := http.Header{}
	for name, value := range raw {
		lower := strings.ToLower(name)
		if lower == "host" || lower == "connection" || lower == "content-length" || lower == "range" || lower == "accept-encoding" {
			continue
		}
		h.Set(lower, value)
	}
	return h
}

// Probe faz HEAD (ou GET Range) para descobrir tamanho, etag etc — portado de direct_http.rs:142
func (p DirectHTTPProvider) Probe(client *http.Client, rawURL string, headers http.Header) (*DirectHTTPMetadata, error) {
	clean := p.CleanURL(rawURL)
	req, _ := http.NewRequest("HEAD", clean, nil)
	for k, vals := range headers {
		for _, v := range vals {
			req.Header.Add(k, v)
		}
	}
	resp, err := client.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	if resp.StatusCode == http.StatusMethodNotAllowed || resp.StatusCode == http.StatusNotImplemented || resp.StatusCode == http.StatusForbidden {
		req2, _ := http.NewRequest("GET", clean, nil)
		for k, vals := range headers {
			for _, v := range vals {
				req2.Header.Add(k, v)
			}
		}
		req2.Header.Set("Range", "bytes=0-0")
		resp2, err := client.Do(req2)
		if err != nil {
			return nil, err
		}
		defer resp2.Body.Close()
		if resp2.StatusCode >= 400 {
			return nil, fmt.Errorf("probe falhou: %d", resp2.StatusCode)
		}
		resp = resp2
	} else if resp.StatusCode >= 400 {
		return nil, fmt.Errorf("probe falhou: %d", resp.StatusCode)
	}

	var dispositionFilename *string
	if cd := headerString(resp.Header, "Content-Disposition"); cd != nil {
		dispositionFilename = filenameFromContentDisposition(*cd)
	}
	var size uint64
	if cl := headerString(resp.Header, "Content-Length"); cl != nil {
		if v, err := strconv.ParseUint(*cl, 10, 64); err == nil {
			size = v
		}
	}
	if size == 0 {
		if cr := resp.Header.Get("Content-Range"); cr != "" {
			if parts := strings.Split(cr, "/"); len(parts) > 1 {
				if v, err := strconv.ParseUint(strings.TrimSpace(parts[len(parts)-1]), 10, 64); err == nil {
					size = v
				}
			}
		}
	}
	acceptsRanges := false
	if ar := headerString(resp.Header, "Accept-Ranges"); ar != nil {
		if strings.Contains(strings.ToLower(*ar), "bytes") {
			acceptsRanges = true
		}
	}
	if resp.StatusCode == http.StatusPartialContent {
		acceptsRanges = true
	}
	var filename string
	if dispositionFilename != nil {
		filename = SanitizeFilename(*dispositionFilename, "download")
	} else {
		filename = filenameFromURL(clean)
	}
	var mime *string
	if ct := headerString(resp.Header, "Content-Type"); ct != nil {
		mime = ct
	}
	var etag *string = headerString(resp.Header, "ETag")
	var lastMod *string = headerString(resp.Header, "Last-Modified")

	return &DirectHTTPMetadata{
		URL:           clean,
		Filename:      filename,
		Size:          size,
		MimeType:      mime,
		AcceptsRanges: acceptsRanges,
		ETag:          etag,
		LastModified:  lastMod,
	}, nil
}

func BuildParts(totalBytes uint64, parallelParts int) []PartRange {
	const minPartSize uint64 = 2 * 1024 * 1024
	usefulParts := int(totalBytes/minPartSize) + 1
	if usefulParts < 1 {
		usefulParts = 1
	}
	partCount := parallelParts
	if partCount > usefulParts {
		partCount = usefulParts
	}
	if partCount < 1 {
		partCount = 1
	}
	var out []PartRange
	for i := 0; i < partCount; i++ {
		start := (totalBytes * uint64(i)) / uint64(partCount)
		end := ((totalBytes * uint64(i+1)) / uint64(partCount)) - 1
		// for last part ensure end = total-1
		if i == partCount-1 {
			end = totalBytes - 1
		}
		out = append(out, PartRange{Index: i, Start: start, End: end})
	}
	return out
}

// MergePartsInto portado de providers/mod.rs:566 — junta partes via rename + append (1x espaço)
func MergePartsInto(destPath string, partPaths []string, totalBytes uint64) error {
	if len(partPaths) == 0 {
		return fmt.Errorf("nenhuma parte para juntar")
	}
	merging := destPath + ".merging"
	_ = os.Remove(merging)
	if err := os.Rename(partPaths[0], merging); err != nil {
		return err
	}
	out, err := os.OpenFile(merging, os.O_APPEND|os.O_WRONLY, 0644)
	if err != nil {
		return err
	}
	defer out.Close()

	buf := make([]byte, 1024*1024)
	for _, partPath := range partPaths[1:] {
		f, err := os.Open(partPath)
		if err != nil {
			return err
		}
		for {
			n, err := f.Read(buf)
			if n > 0 {
				if _, werr := out.Write(buf[:n]); werr != nil {
					f.Close()
					return werr
				}
			}
			if err == io.EOF {
				break
			}
			if err != nil {
				f.Close()
				return err
			}
		}
		f.Close()
		_ = os.Remove(partPath)
	}
	_ = out.Close()
	_ = os.Remove(destPath)
	return os.Rename(merging, destPath)
}

// DownloadSingle com Range resume — portado de direct_http.rs:270
func (p DirectHTTPProvider) DownloadSingle(client *http.Client, meta *DirectHTTPMetadata, destPath string, headers http.Header, progressFn func(downloaded, total uint64)) (uint64, error) {
	var existingBytes uint64
	if info, err := os.Stat(destPath); err == nil && !info.IsDir() {
		existingBytes = uint64(info.Size())
	}
	resumeFrom := existingBytes
	var req *http.Request
	var err error
	if resumeFrom > 0 && meta.AcceptsRanges {
		req, err = http.NewRequest("GET", meta.URL, nil)
		if err != nil {
			return 0, err
		}
		req.Header.Set("Range", fmt.Sprintf("bytes=%d-", resumeFrom))
	} else {
		resumeFrom = 0
		req, err = http.NewRequest("GET", meta.URL, nil)
		if err != nil {
			return 0, err
		}
	}
	for k, vals := range headers {
		for _, v := range vals {
			req.Header.Add(k, v)
		}
	}
	resp, err := client.Do(req)
	if err != nil {
		return 0, err
	}
	defer resp.Body.Close()

	resumed := resumeFrom > 0 && resp.StatusCode == http.StatusPartialContent
	if resumeFrom > 0 && !resumed {
		_ = os.Remove(destPath)
		resumeFrom = 0
		req2, _ := http.NewRequest("GET", meta.URL, nil)
		for k, vals := range headers {
			for _, v := range vals {
				req2.Header.Add(k, v)
			}
		}
		resp2, err := client.Do(req2)
		if err != nil {
			return 0, err
		}
		defer resp2.Body.Close()
		if resp2.StatusCode >= 400 {
			return 0, fmt.Errorf("HTTP %d", resp2.StatusCode)
		}
		resp = resp2
	} else if resp.StatusCode >= 400 {
		return 0, fmt.Errorf("HTTP %d", resp.StatusCode)
	}

	total := meta.Size
	if total == 0 {
		if cr := resp.Header.Get("Content-Range"); cr != "" {
			if parts := strings.Split(cr, "/"); len(parts) > 1 {
				if v, err := strconv.ParseUint(strings.TrimSpace(parts[len(parts)-1]), 10, 64); err == nil {
					total = v
				}
			}
		}
		if total == 0 {
			if cl := resp.Header.Get("Content-Length"); cl != "" {
				if v, err := strconv.ParseUint(cl, 10, 64); err == nil {
					total = v + resumeFrom
				}
			}
		}
	}

	// Handle resume truncate
	var file *os.File
	if resumed {
		// Truncate to resumeFrom in case file has extra bytes beyond saved offset
		if f, err := os.OpenFile(destPath, os.O_WRONLY, 0644); err == nil {
			_ = f.Truncate(int64(resumeFrom))
			f.Close()
		}
		file, err = os.OpenFile(destPath, os.O_APPEND|os.O_WRONLY|os.O_CREATE, 0644)
	} else {
		if err := os.MkdirAll(filepath.Dir(destPath), 0755); err != nil {
			return 0, err
		}
		file, err = os.Create(destPath)
	}
	if err != nil {
		return 0, err
	}
	defer file.Close()

	downloaded := resumeFrom
	buf := make([]byte, 32*1024)
	for {
		n, err := resp.Body.Read(buf)
		if n > 0 {
			if _, werr := file.Write(buf[:n]); werr != nil {
				return downloaded, werr
			}
			downloaded += uint64(n)
			if progressFn != nil {
				progressFn(downloaded, total)
			}
		}
		if err == io.EOF {
			break
		}
		if err != nil {
			return downloaded, err
		}
	}
	if total > 0 && downloaded < total {
		return downloaded, fmt.Errorf("Download HTTP incompleto: %d/%d bytes", downloaded, total)
	}
	return downloaded, nil
}

// DownloadSegmented — portado de direct_http.rs:403
func (p DirectHTTPProvider) DownloadSegmented(client *http.Client, meta *DirectHTTPMetadata, destPath string, parallelParts int, headers http.Header, progressFn func(downloaded, total uint64)) (uint64, error) {
	parts := BuildParts(meta.Size, parallelParts)
	if len(parts) <= 1 {
		return p.DownloadSingle(client, meta, destPath, headers, progressFn)
	}
	if err := os.MkdirAll(filepath.Dir(destPath), 0755); err != nil {
		return 0, err
	}
	totalDownloaded := atomic.Uint64{}
	offsets := make(map[int]uint64)
	for _, part := range parts {
		partPath := fmt.Sprintf("%s.part%d", destPath, part.Index)
		partLen := part.End - part.Start + 1
		var fileLen uint64
		if info, err := os.Stat(partPath); err == nil && !info.IsDir() {
			fileLen = uint64(info.Size())
		}
		offset := fileLen
		if offset > partLen {
			offset = partLen
		}
		if offset == 0 && fileLen > 0 {
			_ = os.Remove(partPath)
			offset = 0
		} else if offset < fileLen {
			if f, err := os.OpenFile(partPath, os.O_WRONLY, 0644); err == nil {
				_ = f.Truncate(int64(offset))
				f.Close()
			}
		}
		offsets[part.Index] = offset
		totalDownloaded.Add(offset)
	}

	var wg sync.WaitGroup
	errCh := make(chan error, len(parts))

	for _, part := range parts {
		offset := offsets[part.Index]
		partLen := part.End - part.Start + 1
		if offset >= partLen {
			continue
		}
		wg.Add(1)
		go func(pr PartRange, off uint64) {
			defer wg.Done()
			rangeStart := pr.Start + off
			req, _ := http.NewRequest("GET", meta.URL, nil)
			for k, vals := range headers {
				for _, v := range vals {
					req.Header.Add(k, v)
				}
			}
			req.Header.Set("Range", fmt.Sprintf("bytes=%d-%d", rangeStart, pr.End))
			resp, err := client.Do(req)
			if err != nil {
				errCh <- err
				return
			}
			defer resp.Body.Close()
			if resp.StatusCode != http.StatusPartialContent {
				errCh <- fmt.Errorf("Servidor HTTP ignorou Range para a parte %d", pr.Index)
				return
			}
			partPath := fmt.Sprintf("%s.part%d", destPath, pr.Index)
			f, err := os.OpenFile(partPath, os.O_APPEND|os.O_WRONLY|os.O_CREATE, 0644)
			if err != nil {
				errCh <- err
				return
			}
			defer f.Close()
			buf := make([]byte, 32*1024)
			partDownloaded := off
			for {
				n, err := resp.Body.Read(buf)
				if n > 0 {
					if _, werr := f.Write(buf[:n]); werr != nil {
						errCh <- werr
						return
					}
					partDownloaded += uint64(n)
					downloaded := totalDownloaded.Add(uint64(n))
					if progressFn != nil {
						progressFn(downloaded, meta.Size)
					}
					_ = partDownloaded
				}
				if err == io.EOF {
					break
				}
				if err != nil {
					errCh <- err
					return
				}
			}
			if partDownloaded < partLen {
				errCh <- fmt.Errorf("Parte HTTP incompleta %d: %d/%d bytes", pr.Index, partDownloaded, partLen)
				return
			}
		}(part, offset)
	}

	wg.Wait()
	close(errCh)
	for err := range errCh {
		if err != nil {
			return totalDownloaded.Load(), err
		}
	}

	partPaths := make([]string, len(parts))
	for i, pr := range parts {
		partPaths[i] = fmt.Sprintf("%s.part%d", destPath, pr.Index)
	}
	if err := MergePartsInto(destPath, partPaths, meta.Size); err != nil {
		return 0, err
	}
	return meta.Size, nil
}
