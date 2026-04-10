use anyhow::{anyhow, Context, Result};
use base64::Engine;
use cipher::{KeyIvInit, StreamCipher};
use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;

use crate::models::FileInfo;
use super::{Provider, ProgressUpdate};

// AES-128 em modo CTR (Counter)
// AES-128 = cifra de bloco com chave de 128 bits (16 bytes)
// CTR = transforma AES de cifra de bloco em cifra de fluxo, ideal para streaming
// Ctr128BE = CTR com contador big-endian de 128 bits
type Aes128Ctr = ctr::Ctr128BE<aes::Aes128>;

pub struct MegaProvider;

impl MegaProvider {
    pub fn matches(url: &str) -> bool {
        url.contains("mega.nz") || url.contains("mega.co.nz")
    }

    // Decodifica o base64 customizado do Mega
    // O Mega usa base64 URL-safe sem padding: '-' em vez de '+', '_' em vez de '/'
    pub fn mega_base64_decode(s: &str) -> Vec<u8> {
        // Substitui os caracteres do Mega pelo base64 padrão
        let standard = s.replace('-', "+").replace('_', "/");
        // Adiciona padding '=' até o comprimento ser múltiplo de 4
        let padding = match standard.len() % 4 {
            2 => "==",
            3 => "=",
            _ => "",
        };
        let padded = format!("{standard}{padding}");
        base64::engine::general_purpose::STANDARD
            .decode(&padded)
            .unwrap_or_default()
    }

    // Faz o parse da URL do Mega e retorna (handle, key_bytes)
    // handle = identificador do arquivo na API do Mega
    // key_bytes = 32 bytes da chave de descriptografia
    //
    // Formatos suportados:
    //   Novo: https://mega.nz/file/HANDLE#KEY
    //   Antigo: https://mega.nz/#!HANDLE!KEY
    pub fn parse_url(url: &str) -> Option<(String, Vec<u8>)> {
        // Formato novo: /file/HANDLE#KEY
        if let Some(pos) = url.find("/file/") {
            let after = &url[pos + 6..];
            let parts: Vec<&str> = after.splitn(2, '#').collect();
            if parts.len() == 2 {
                let handle = parts[0].to_string();
                let key_bytes = Self::mega_base64_decode(parts[1]);
                if key_bytes.len() >= 16 {
                    return Some((handle, key_bytes));
                }
            }
        }

        // Formato antigo: /#!HANDLE!KEY
        if let Some(pos) = url.find("#!") {
            let after = &url[pos + 2..];
            let parts: Vec<&str> = after.splitn(2, '!').collect();
            if parts.len() == 2 {
                let handle = parts[0].to_string();
                let key_bytes = Self::mega_base64_decode(parts[1]);
                if key_bytes.len() >= 16 {
                    return Some((handle, key_bytes));
                }
            }
        }

        None
    }

    // Deriva a chave AES-128 e o IV a partir dos bytes da URL
    //
    // O Mega armazena 32 bytes na URL:
    //   bytes[0..16]  = chave AES XOR'd com nonce+mac (obfuscado)
    //   bytes[16..24] = nonce (8 bytes) — usado como IV no AES-CTR
    //   bytes[24..32] = mac (não usado para download)
    //
    // Para obter a chave real: XOR dos primeiros 16 bytes com os bytes 16..32
    pub fn derive_key_and_iv(key_bytes: &[u8]) -> ([u8; 16], [u8; 16]) {
        // Garante que temos pelo menos 32 bytes
        let key_bytes = if key_bytes.len() >= 32 {
            key_bytes
        } else {
            // Se a chave for menor (links antigos têm 16 bytes = só a chave AES)
            return {
                let mut key = [0u8; 16];
                let len = key_bytes.len().min(16);
                key[..len].copy_from_slice(&key_bytes[..len]);
                (key, [0u8; 16])
            };
        };

        // Chave AES = XOR dos primeiros 16 bytes com os bytes 16..32
        let mut aes_key = [0u8; 16];
        for i in 0..16 {
            aes_key[i] = key_bytes[i] ^ key_bytes[i + 16];
        }

        // IV do AES-CTR = nonce (bytes 16..24) + 8 zeros
        // O nonce de 8 bytes fica nos primeiros 8 bytes do IV de 16 bytes
        let mut iv = [0u8; 16];
        iv[..8].copy_from_slice(&key_bytes[16..24]);

        (aes_key, iv)
    }

    // Chama a API do Mega para obter a URL temporária de download
    // A API do Mega é uma API RPC JSON — enviamos um comando e recebemos resposta
    async fn get_download_url(client: &reqwest::Client, handle: &str) -> Result<(String, u64)> {
        // Endpoint da API do Mega — recebe uma lista de comandos RPC
        let api_url = "https://g.api.mega.co.nz/cs?id=0";

        // Comando "g" = get download info, "p" = file handle público
        let body = serde_json::json!([{"a": "g", "p": handle}]);

        let resp: serde_json::Value = client
            .post(api_url)
            .json(&body)
            .send()
            .await
            .context("Falha ao chamar API do Mega")?
            .json()
            .await
            .context("Falha ao parsear resposta do Mega")?;

        // A resposta é um array — pegamos o primeiro elemento
        let result = &resp[0];

        // Se o resultado for um número negativo, é um código de erro do Mega
        // -2 = arquivo não encontrado, -3 = acesso negado, etc.
        if let Some(err_code) = result.as_i64() {
            return Err(anyhow!(
                "Mega retornou erro {err_code}. O arquivo pode não existir ou ser privado."
            ));
        }

        // "g" = URL de download temporária
        let download_url = result["g"]
            .as_str()
            .ok_or_else(|| anyhow!("API do Mega não retornou URL de download"))?
            .to_string();

        // "s" = tamanho do arquivo em bytes
        let size = result["s"].as_u64().unwrap_or(0);

        Ok((download_url, size))
    }
}

impl Provider for MegaProvider {
    fn name(&self) -> &str { "Mega" }

    fn get_file_info<'a>(&'a self, url: &'a str)
        -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FileInfo>> + Send + 'a>>
    {
        Box::pin(async move {
            let (handle, _key_bytes) = Self::parse_url(url)
                .ok_or_else(|| anyhow!("URL do Mega inválida: {url}"))?;

            let client = reqwest::Client::new();
            let (_download_url, size) = Self::get_download_url(&client, &handle).await?;

            // O Mega criptografa até o nome do arquivo nos atributos
            // Para descriptografar o nome precisaria de crypto adicional
            // Por enquanto usamos o handle como identificador
            Ok(FileInfo {
                filename: format!("mega_{handle}"),
                size,
                mime_type: None,
            })
        })
    }

    fn download<'a>(
        &'a self,
        url: &'a str,
        dest_path: &'a str,
        progress_tx: tokio::sync::mpsc::Sender<ProgressUpdate>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u64>> + Send + 'a>>
    {
        Box::pin(async move {
            // Passo 1: parse da URL para extrair handle e chave
            let (handle, key_bytes) = Self::parse_url(url)
                .ok_or_else(|| anyhow!("URL do Mega inválida: {url}"))?;

            // Passo 2: deriva chave AES e IV
            let (aes_key, iv) = Self::derive_key_and_iv(&key_bytes);

            // Passo 3: obtém URL temporária de download via API
            let client = reqwest::Client::new();
            let (download_url, total) = Self::get_download_url(&client, &handle).await?;

            // Passo 4: baixa o arquivo criptografado
            let resp = client
                .get(&download_url)
                .send()
                .await?
                .error_for_status()?;

            let mut file = tokio::fs::File::create(dest_path).await?;
            let mut stream = resp.bytes_stream();
            let mut downloaded: u64 = 0;

            // Inicializa o cipher AES-128-CTR
            // O cipher mantém estado interno (posição no keystream)
            // Cada chunk é descriptografado em sequência — a ordem importa
            let mut cipher = Aes128Ctr::new(&aes_key.into(), &iv.into());

            while let Some(chunk) = stream.next().await {
                // Converte para Vec<u8> para poder modificar in-place
                let mut chunk = chunk?.to_vec();

                // apply_keystream descriptografa os bytes in-place
                // XOR cada byte do arquivo criptografado com o keystream do AES-CTR
                cipher.apply_keystream(&mut chunk);

                file.write_all(&chunk).await?;
                downloaded += chunk.len() as u64;

                let _ = progress_tx
                    .send(ProgressUpdate { bytes_downloaded: downloaded, total_bytes: total })
                    .await;
            }

            file.flush().await?;
            Ok(downloaded)
        })
    }
}

// --- Testes unitários ---
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_new_format() {
        // Formato novo: /file/HANDLE#KEY (KEY deve ter 32 bytes = 43 chars base64)
        let url = "https://mega.nz/file/AbCdEfGh#AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        let parsed = MegaProvider::parse_url(url);
        assert!(parsed.is_some());
        let (handle, _) = parsed.unwrap();
        assert_eq!(handle, "AbCdEfGh");
    }

    #[test]
    fn test_parse_old_format() {
        // Formato antigo: /#!HANDLE!KEY
        let url = "https://mega.nz/#!AbCdEfGh!AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        let parsed = MegaProvider::parse_url(url);
        assert!(parsed.is_some());
        let (handle, _) = parsed.unwrap();
        assert_eq!(handle, "AbCdEfGh");
    }

    #[test]
    fn test_mega_base64_all_zeros() {
        // 32 zeros em base64 do Mega (sem padding, com - e _ em vez de + e /)
        let decoded = MegaProvider::mega_base64_decode(
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
        );
        assert_eq!(decoded.len(), 32);
        assert!(decoded.iter().all(|&b| b == 0));
    }

    #[test]
    fn test_derive_key_xor() {
        // Com bytes todos zeros: XOR(0, 0) = 0
        let key_bytes = vec![0u8; 32];
        let (aes_key, iv) = MegaProvider::derive_key_and_iv(&key_bytes);
        assert_eq!(aes_key, [0u8; 16]);
        assert_eq!(iv, [0u8; 16]);
    }

    #[test]
    fn test_derive_key_xor_non_zero() {
        // Verifica que o XOR está funcionando corretamente
        // Se bytes[0] = 0xFF e bytes[16] = 0x0F, então chave[0] = 0xFF ^ 0x0F = 0xF0
        let mut key_bytes = vec![0u8; 32];
        key_bytes[0] = 0xFF;
        key_bytes[16] = 0x0F;
        let (aes_key, _) = MegaProvider::derive_key_and_iv(&key_bytes);
        assert_eq!(aes_key[0], 0xFF ^ 0x0F);
    }
}
