package hashverify

import (
	"crypto/md5"
	"crypto/sha1"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"hash"
	"hash/crc32"
	"io"
	"os"
	"strings"
	"unicode"

	"gdownloader-go/internal/models"
)

// Portado de backend/src/hash_verify.rs:11

type VerificationProgress struct {
	BytesDone  uint64 `json:"bytesDone"`
	BytesTotal uint64 `json:"bytesTotal"`
}

type VerificationResult struct {
	Actual   string `json:"actual"`
	Expected string `json:"expected"`
	Matched  bool   `json:"matched"`
}

// NormalizeHash portado de hash_verify.rs:59 — filtra só hex digits, lowercases
func NormalizeHash(value string) string {
	var b strings.Builder
	for _, ch := range value {
		if unicode.Is(unicode.ASCII_Hex_Digit, ch) {
			b.WriteRune(unicode.ToLower(ch))
		}
	}
	return b.String()
}

// VerifyFile portado de hash_verify.rs:67 — verifica arquivo em disco contra ExpectedHash
// progressFn pode ser nil; se não nil é chamado a cada chunk (estrangulamento cabe ao chamador filtrar)
func VerifyFile(path string, expected models.ExpectedHash, progressFn func(VerificationProgress)) (VerificationResult, error) {
	file, err := os.Open(path)
	if err != nil {
		return VerificationResult{}, fmt.Errorf("Falha ao abrir arquivo para verificar hash: %w", err)
	}
	defer file.Close()

	info, err := file.Stat()
	var total uint64
	if err == nil {
		total = uint64(info.Size())
	}

	var h hash.Hash
	var crcHash hash.Hash32
	var algo string

	switch expected.Algorithm {
	case models.HashAlgoMd5:
		h = md5.New()
		algo = "md5"
	case models.HashAlgoSha1:
		h = sha1.New()
		algo = "sha1"
	case models.HashAlgoSha256:
		h = sha256.New()
		algo = "sha256"
	case models.HashAlgoCrc32:
		crcHash = crc32.NewIEEE()
		algo = "crc32"
	default:
		return VerificationResult{}, fmt.Errorf("algoritmo de hash desconhecido: %s", expected.Algorithm)
	}

	buf := make([]byte, 1024*1024)
	var done uint64

	for {
		n, readErr := file.Read(buf)
		if n > 0 {
			if algo == "crc32" {
				_, _ = crcHash.Write(buf[:n])
			} else {
				_, _ = h.Write(buf[:n])
			}
			done += uint64(n)
			if progressFn != nil {
				progressFn(VerificationProgress{BytesDone: done, BytesTotal: total})
			}
		}
		if readErr == io.EOF {
			break
		}
		if readErr != nil {
			return VerificationResult{}, readErr
		}
	}

	var actual string
	if algo == "crc32" {
		actual = fmt.Sprintf("%08x", crcHash.Sum32())
	} else {
		actual = hex.EncodeToString(h.Sum(nil))
	}

	expectedNormalized := NormalizeHash(expected.Value)
	matched := strings.EqualFold(actual, expectedNormalized)

	return VerificationResult{
		Actual:   actual,
		Expected: expectedNormalized,
		Matched:  matched,
	}, nil
}

// VerifyFileWithChannel é um wrapper que envia progresso para um canal (compatível com padrão Rust mpsc)
func VerifyFileWithChannel(path string, expected models.ExpectedHash, ch chan<- VerificationProgress) (VerificationResult, error) {
	fn := func(p VerificationProgress) {
		select {
		case ch <- p:
		default:
		}
	}
	if ch == nil {
		fn = nil
	}
	return VerifyFile(path, expected, fn)
}
