package integrity

import (
	"bytes"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"
)

// Portado de backend/src/integrity.rs:14

type Integrity struct {
	Ok     bool
	Reason string
}

func (i Integrity) IsOk() bool { return i.Ok }
func (i Integrity) ReasonStr() *string {
	if i.Ok {
		return nil
	}
	return &i.Reason
}

// CheckFile portado de integrity.rs:33 — verificação leve pós-download
func CheckFile(path string, expectedSize uint64) Integrity {
	info, err := os.Stat(path)
	if err != nil {
		return Integrity{Ok: false, Reason: fmt.Sprintf("arquivo inacessível: %v", err)}
	}
	if !info.Mode().IsRegular() {
		return Integrity{Ok: false, Reason: "o destino não é um arquivo"}
	}
	size := uint64(info.Size())
	if size == 0 {
		return Integrity{Ok: false, Reason: "arquivo vazio (0 bytes)"}
	}
	if expectedSize > 0 && size != expectedSize {
		return Integrity{Ok: false, Reason: fmt.Sprintf("tamanho difere do esperado: %d bytes (esperado %d)", size, expectedSize)}
	}

	head, tail, err := readHeadTail(path, size)
	if err != nil {
		return Integrity{Ok: false, Reason: fmt.Sprintf("falha ao ler o arquivo: %v", err)}
	}

	ext := strings.ToLower(strings.TrimPrefix(filepath.Ext(path), "."))
	if ext != "html" && ext != "htm" && ext != "xhtml" && ext != "svg" {
		if looksLikeHTML(head) {
			return Integrity{Ok: false, Reason: "o conteúdo parece uma página HTML de erro, não o arquivo"}
		}
	}

	if reason := checkSignature(ext, head, tail); reason != nil {
		return Integrity{Ok: false, Reason: *reason}
	}

	return Integrity{Ok: true}
}

func readHeadTail(path string, size uint64) ([]byte, []byte, error) {
	f, err := os.Open(path)
	if err != nil {
		return nil, nil, err
	}
	defer f.Close()

	headLen := int(min64(size, 64))
	head := make([]byte, headLen)
	if _, err := io.ReadFull(f, head); err != nil {
		return nil, nil, err
	}
	tailLen := int(min64(size, 64))
	tail := make([]byte, tailLen)
	if _, err := f.Seek(-int64(tailLen), io.SeekEnd); err != nil {
		return nil, nil, err
	}
	if _, err := io.ReadFull(f, tail); err != nil {
		return nil, nil, err
	}
	return head, tail, nil
}

func min64(a, b uint64) uint64 {
	if a < b {
		return a
	}
	return b
}

func looksLikeHTML(head []byte) bool {
	n := len(head)
	if n > 64 {
		n = 64
	}
	start := strings.ToLower(strings.TrimLeft(string(head[:n]), " \t\r\n"))
	if strings.HasPrefix(start, "<!doctype html") {
		return true
	}
	if strings.HasPrefix(start, "<html") {
		return true
	}
	if strings.HasPrefix(start, "<?xml") && strings.Contains(start, "<html") {
		return true
	}
	return false
}

func startsWith(b, sig []byte) bool {
	if len(b) < len(sig) {
		return false
	}
	return bytes.Equal(b[:len(sig)], sig)
}

func mismatch(fmtName string) *string {
	s := fmt.Sprintf("assinatura de %s não confere (arquivo possivelmente corrompido)", fmtName)
	return &s
}

func checkSignature(ext string, head, tail []byte) *string {
	switch ext {
	case "mkv", "webm", "mka":
		if !startsWith(head, []byte{0x1A, 0x45, 0xDF, 0xA3}) {
			return mismatch("Matroska/WebM")
		}
	case "mp4", "m4v", "mov", "m4a":
		if !(len(head) >= 8 && bytes.Equal(head[4:8], []byte("ftyp"))) {
			return mismatch("MP4/MOV")
		}
	case "avi":
		if !(len(head) >= 4 && bytes.Equal(head[:4], []byte("RIFF"))) {
			return mismatch("AVI")
		}
	case "zip", "apk", "docx", "xlsx", "pptx", "epub", "jar":
		if !(startsWith(head, []byte{0x50, 0x4B, 0x03, 0x04}) ||
			startsWith(head, []byte{0x50, 0x4B, 0x05, 0x06}) ||
			startsWith(head, []byte{0x50, 0x4B, 0x07, 0x08})) {
			return mismatch("ZIP")
		}
	case "rar":
		if !startsWith(head, []byte{0x52, 0x61, 0x72, 0x21, 0x1A, 0x07}) {
			return mismatch("RAR")
		}
	case "7z":
		if !startsWith(head, []byte{0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C}) {
			return mismatch("7-Zip")
		}
	case "gz", "tgz":
		if !startsWith(head, []byte{0x1F, 0x8B}) {
			return mismatch("GZIP")
		}
	case "pdf":
		if !startsWith(head, []byte("%PDF")) {
			return mismatch("PDF")
		}
		tailStr := string(tail)
		if !strings.Contains(tailStr, "%%EOF") {
			s := "PDF truncado: não encontrei o marcador %%EOF no final"
			return &s
		}
	case "png":
		if !startsWith(head, []byte{0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A}) {
			return mismatch("PNG")
		}
	case "jpg", "jpeg":
		if !startsWith(head, []byte{0xFF, 0xD8, 0xFF}) {
			return mismatch("JPEG")
		}
		if !(len(tail) >= 2 && tail[len(tail)-2] == 0xFF && tail[len(tail)-1] == 0xD9) {
			s := "JPEG truncado: não termina com o marcador de fim (FFD9)"
			return &s
		}
	case "gif":
		if !(startsWith(head, []byte("GIF87a")) || startsWith(head, []byte("GIF89a"))) {
			return mismatch("GIF")
		}
	case "flac":
		if !startsWith(head, []byte("fLaC")) {
			return mismatch("FLAC")
		}
	case "mp3":
		if !(startsWith(head, []byte("ID3")) || (len(head) >= 2 && head[0] == 0xFF && head[1]&0xE0 == 0xE0)) {
			return mismatch("MP3")
		}
	case "iso":
		return nil
	default:
		return nil
	}
	return nil
}
