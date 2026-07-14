//! Verificação leve de integridade pós-download.
//!
//! Sem hash do host não dá pra provar integridade byte-a-byte, mas dá pra pegar os
//! defeitos mais comuns de forma barata (lê só o começo e o fim do arquivo):
//! - arquivo vazio (0 bytes);
//! - tamanho diferente do esperado (download truncado / partes mescladas erradas);
//! - página de erro HTML salva no lugar do arquivo (host devolveu erro);
//! - assinatura (magic bytes) do formato não confere com a extensão.

use std::path::Path;
use tokio::io::{AsyncReadExt, AsyncSeekExt, SeekFrom};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Integrity {
    Ok,
    /// Provavelmente quebrado; carrega o motivo legível.
    Broken(String),
}

impl Integrity {
    pub fn is_ok(&self) -> bool {
        matches!(self, Integrity::Ok)
    }
    pub fn reason(&self) -> Option<&str> {
        match self {
            Integrity::Ok => None,
            Integrity::Broken(reason) => Some(reason),
        }
    }
}

/// Confere um arquivo já baixado. `expected_size` = 0 quando desconhecido.
pub async fn check_file(path: &str, expected_size: u64) -> Integrity {
    let meta = match tokio::fs::metadata(path).await {
        Ok(meta) => meta,
        Err(error) => return Integrity::Broken(format!("arquivo inacessível: {error}")),
    };
    if !meta.is_file() {
        return Integrity::Broken("o destino não é um arquivo".to_string());
    }
    let size = meta.len();
    if size == 0 {
        return Integrity::Broken("arquivo vazio (0 bytes)".to_string());
    }
    if expected_size > 0 && size != expected_size {
        return Integrity::Broken(format!(
            "tamanho difere do esperado: {size} bytes (esperado {expected_size})"
        ));
    }

    let (head, tail) = match read_head_tail(path, size).await {
        Ok(bytes) => bytes,
        Err(error) => return Integrity::Broken(format!("falha ao ler o arquivo: {error}")),
    };

    let ext = Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .unwrap_or_default();

    // Página de erro HTML salva como se fosse o arquivo (host devolveu erro).
    if !matches!(ext.as_str(), "html" | "htm" | "xhtml" | "svg") && looks_like_html(&head) {
        return Integrity::Broken(
            "o conteúdo parece uma página HTML de erro, não o arquivo".to_string(),
        );
    }

    if let Some(reason) = check_signature(&ext, &head, &tail) {
        return Integrity::Broken(reason);
    }

    Integrity::Ok
}

async fn read_head_tail(path: &str, size: u64) -> std::io::Result<(Vec<u8>, Vec<u8>)> {
    let mut file = tokio::fs::File::open(path).await?;
    let head_len = size.min(64) as usize;
    let mut head = vec![0u8; head_len];
    file.read_exact(&mut head).await?;

    let tail_len = size.min(64) as usize;
    let mut tail = vec![0u8; tail_len];
    file.seek(SeekFrom::End(-(tail_len as i64))).await?;
    file.read_exact(&mut tail).await?;
    Ok((head, tail))
}

fn looks_like_html(head: &[u8]) -> bool {
    let start = String::from_utf8_lossy(&head[..head.len().min(64)])
        .trim_start()
        .to_ascii_lowercase();
    start.starts_with("<!doctype html")
        || start.starts_with("<html")
        || start.starts_with("<?xml") && start.contains("<html")
}

fn starts_with(bytes: &[u8], sig: &[u8]) -> bool {
    bytes.len() >= sig.len() && &bytes[..sig.len()] == sig
}

/// Retorna `Some(motivo)` se a assinatura do formato conhecido não bater.
/// Extensões desconhecidas retornam `None` (não marcamos como quebrado).
fn check_signature(ext: &str, head: &[u8], tail: &[u8]) -> Option<String> {
    let mismatch = |fmt: &str| Some(format!("assinatura de {fmt} não confere (arquivo possivelmente corrompido)"));

    match ext {
        "mkv" | "webm" | "mka" => {
            // EBML header
            (!starts_with(head, &[0x1A, 0x45, 0xDF, 0xA3])).then(|| mismatch("Matroska/WebM")).flatten()
        }
        "mp4" | "m4v" | "mov" | "m4a" => {
            // caixa 'ftyp' nos bytes 4..8
            (!(head.len() >= 8 && &head[4..8] == b"ftyp")).then(|| mismatch("MP4/MOV")).flatten()
        }
        "avi" => (!(head.len() >= 4 && &head[..4] == b"RIFF")).then(|| mismatch("AVI")).flatten(),
        "zip" | "apk" | "docx" | "xlsx" | "pptx" | "epub" | "jar" => {
            (!(starts_with(head, &[0x50, 0x4B, 0x03, 0x04])
                || starts_with(head, &[0x50, 0x4B, 0x05, 0x06])
                || starts_with(head, &[0x50, 0x4B, 0x07, 0x08])))
            .then(|| mismatch("ZIP")).flatten()
        }
        "rar" => (!starts_with(head, &[0x52, 0x61, 0x72, 0x21, 0x1A, 0x07])).then(|| mismatch("RAR")).flatten(),
        "7z" => (!starts_with(head, &[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C])).then(|| mismatch("7-Zip")).flatten(),
        "gz" | "tgz" => (!starts_with(head, &[0x1F, 0x8B])).then(|| mismatch("GZIP")).flatten(),
        "pdf" => {
            if !starts_with(head, b"%PDF") {
                return mismatch("PDF");
            }
            // PDF válido termina com %%EOF (pode ter espaços/quebras depois).
            let tail_str = String::from_utf8_lossy(tail);
            (!tail_str.contains("%%EOF"))
                .then(|| "PDF truncado: não encontrei o marcador %%EOF no final".to_string())
        }
        "png" => (!starts_with(head, &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A])).then(|| mismatch("PNG")).flatten(),
        "jpg" | "jpeg" => {
            if !starts_with(head, &[0xFF, 0xD8, 0xFF]) {
                return mismatch("JPEG");
            }
            (!(tail.len() >= 2 && tail[tail.len() - 2..] == [0xFF, 0xD9]))
                .then(|| "JPEG truncado: não termina com o marcador de fim (FFD9)".to_string())
        }
        "gif" => (!(starts_with(head, b"GIF87a") || starts_with(head, b"GIF89a"))).then(|| mismatch("GIF")).flatten(),
        "flac" => (!starts_with(head, b"fLaC")).then(|| mismatch("FLAC")).flatten(),
        "mp3" => {
            // ID3 ou frame sync 0xFFEx
            (!(starts_with(head, b"ID3") || (head.len() >= 2 && head[0] == 0xFF && head[1] & 0xE0 == 0xE0)))
                .then(|| mismatch("MP3")).flatten()
        }
        "iso" => None, // sem assinatura confiável no começo
        _ => None,     // formato desconhecido → não julgamos
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn write_temp(name: &str, bytes: &[u8]) -> String {
        let path = std::env::temp_dir().join(format!("gdl-integ-{}-{name}", uuid::Uuid::new_v4()));
        tokio::fs::write(&path, bytes).await.unwrap();
        path.to_string_lossy().to_string()
    }

    #[tokio::test]
    async fn ok_for_valid_matroska() {
        let mut data = vec![0x1A, 0x45, 0xDF, 0xA3];
        data.extend_from_slice(&[0u8; 100]);
        let path = write_temp("v.mkv", &data).await;
        assert_eq!(check_file(&path, 0).await, Integrity::Ok);
        let _ = tokio::fs::remove_file(&path).await;
    }

    #[tokio::test]
    async fn broken_for_html_error_page_saved_as_mkv() {
        let path = write_temp("x.mkv", b"<!DOCTYPE html><html><body>403</body></html>").await;
        assert!(!check_file(&path, 0).await.is_ok());
        let _ = tokio::fs::remove_file(&path).await;
    }

    #[tokio::test]
    async fn broken_for_size_mismatch_and_empty() {
        let path = write_temp("a.bin", b"1234567890").await;
        assert!(!check_file(&path, 999).await.is_ok());
        assert!(check_file(&path, 10).await.is_ok());
        let empty = write_temp("e.bin", b"").await;
        assert!(!check_file(&empty, 0).await.is_ok());
        let _ = tokio::fs::remove_file(&path).await;
        let _ = tokio::fs::remove_file(&empty).await;
    }

    #[tokio::test]
    async fn unknown_extension_is_not_judged() {
        let path = write_temp("f.xyzformat", b"qualquer coisa aqui").await;
        assert_eq!(check_file(&path, 0).await, Integrity::Ok);
        let _ = tokio::fs::remove_file(&path).await;
    }
}
