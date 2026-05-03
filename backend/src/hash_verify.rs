use anyhow::{anyhow, Result};
use crc32fast::Hasher as Crc32Hasher;
use sha1::{Digest, Sha1};
use sha2::Sha256;
use std::io::Read;
use tokio::sync::mpsc;

use crate::models::{ExpectedHash, HashAlgorithm};

#[derive(Debug, Clone)]
pub struct VerificationProgress {
    pub bytes_done: u64,
    pub bytes_total: u64,
}

#[derive(Debug, Clone)]
pub struct VerificationResult {
    pub actual: String,
    pub expected: String,
    pub matched: bool,
}

enum HashState {
    Md5(md5::Context),
    Sha1(Sha1),
    Sha256(Sha256),
    Crc32(Crc32Hasher),
}

impl HashState {
    fn new(algorithm: &HashAlgorithm) -> Self {
        match algorithm {
            HashAlgorithm::Md5 => Self::Md5(md5::Context::new()),
            HashAlgorithm::Sha1 => Self::Sha1(Sha1::new()),
            HashAlgorithm::Sha256 => Self::Sha256(Sha256::new()),
            HashAlgorithm::Crc32 => Self::Crc32(Crc32Hasher::new()),
        }
    }

    fn update(&mut self, bytes: &[u8]) {
        match self {
            Self::Md5(hasher) => hasher.consume(bytes),
            Self::Sha1(hasher) => hasher.update(bytes),
            Self::Sha256(hasher) => hasher.update(bytes),
            Self::Crc32(hasher) => hasher.update(bytes),
        }
    }

    fn finalize(self) -> String {
        match self {
            Self::Md5(hasher) => format!("{:x}", hasher.compute()),
            Self::Sha1(hasher) => format!("{:x}", hasher.finalize()),
            Self::Sha256(hasher) => format!("{:x}", hasher.finalize()),
            Self::Crc32(hasher) => format!("{:08x}", hasher.finalize()),
        }
    }
}

pub fn normalize_hash(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_hexdigit())
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

pub async fn verify_file(
    path: String,
    expected: ExpectedHash,
    progress_tx: mpsc::Sender<VerificationProgress>,
) -> Result<VerificationResult> {
    tokio::task::spawn_blocking(move || {
        let mut file = std::fs::File::open(&path)
            .map_err(|error| anyhow!("Falha ao abrir arquivo para verificar hash: {error}"))?;
        let total = file.metadata().map(|metadata| metadata.len()).unwrap_or(0);
        let mut hasher = HashState::new(&expected.algorithm);
        let mut buffer = vec![0u8; 1024 * 1024];
        let mut done = 0u64;

        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
            done = done.saturating_add(read as u64);
            let _ = progress_tx.blocking_send(VerificationProgress {
                bytes_done: done,
                bytes_total: total,
            });
        }

        let actual = hasher.finalize();
        let expected_normalized = normalize_hash(&expected.value);
        Ok(VerificationResult {
            matched: actual.eq_ignore_ascii_case(&expected_normalized),
            actual,
            expected: expected_normalized,
        })
    })
    .await?
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn verify_temp_file(algorithm: HashAlgorithm, value: &str) -> VerificationResult {
        let path = std::env::temp_dir().join(format!(
            "gdownloader-hash-test-{}-{}.txt",
            std::process::id(),
            value
        ));
        std::fs::write(&path, b"gDownloader hash test").unwrap();
        let (tx, mut rx) = mpsc::channel(8);
        let result = verify_file(
            path.to_string_lossy().to_string(),
            ExpectedHash {
                algorithm,
                value: value.to_string(),
            },
            tx,
        )
        .await
        .unwrap();
        let _progress = rx.recv().await.unwrap();
        let _ = std::fs::remove_file(path);
        result
    }

    #[test]
    fn normalizes_hash_values() {
        assert_eq!(normalize_hash(" SHA-256: AB cd-12 "), "a256abcd12");
        assert_eq!(normalize_hash("DEAD-BEEF"), "deadbeef");
    }

    #[tokio::test]
    async fn verifies_md5_sha1_sha256_and_crc32() {
        assert!(
            verify_temp_file(HashAlgorithm::Md5, "1a5af40a76e6618b3b2c23f21a70eda1")
                .await
                .matched
        );
        assert!(
            verify_temp_file(HashAlgorithm::Sha1, "c74b2046a18de55ac3fe42bb43dbdef54ca21371")
                .await
                .matched
        );
        assert!(
            verify_temp_file(
                HashAlgorithm::Sha256,
                "43afaf003909ad1a7766faa4b6a8db47ef80a9c5b8fa491b58524eb232267e4d",
            )
            .await
            .matched
        );
        assert!(verify_temp_file(HashAlgorithm::Crc32, "f6c3e2a3").await.matched);
    }

    #[tokio::test]
    async fn detects_hash_mismatch() {
        let result = verify_temp_file(HashAlgorithm::Crc32, "00000000").await;
        assert!(!result.matched);
        assert_eq!(result.actual, "f6c3e2a3");
    }
}
