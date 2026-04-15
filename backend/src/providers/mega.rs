use anyhow::{anyhow, Context, Result};
use base64::Engine;
use cipher::{KeyIvInit, StreamCipher};
use futures_util::StreamExt;
use serde_json::Value;
use tokio::io::AsyncWriteExt;

use crate::models::FileInfo;
use super::{Provider, ProgressUpdate};

// AES-128 em modo CTR (Counter Mode)
// AES-128 = cifra de bloco com chave de 128 bits (16 bytes)
// CTR = transforma a cifra de bloco em cifra de fluxo: gera um keystream e faz XOR com os dados
//       Ideal para streaming porque pode descriptografar chunks em qualquer ordem
// Ctr128BE = contador big-endian de 128 bits — formato exigido pelo Mega
type Aes128Ctr = ctr::Ctr128BE<aes::Aes128>;

pub struct MegaProvider;

// Como um class em PHP — agrupa os métodos estáticos e de instância do provider
impl MegaProvider {
    // Verifica se a URL pertence ao Mega
    // Cobre tanto mega.nz (domínio público) quanto mega.co.nz (domínio legado)
    pub fn matches(url: &str) -> bool {
        url.contains("mega.nz") || url.contains("mega.co.nz")
    }

    // Decodifica o Base64 customizado do Mega para bytes brutos
    // O Mega usa Base64 URL-safe SEM padding:
    //   '-' em vez de '+', '_' em vez de '/', sem '=' no final
    // Esta função converte para o Base64 padrão antes de decodificar
    pub fn mega_base64_decode(s: &str) -> Vec<u8> {
        // Substitui os caracteres do Mega pelos equivalentes Base64 padrão
        let standard = s.replace('-', "+").replace('_', "/");

        // Calcula e adiciona o padding '=' necessário para o Base64 padrão
        // Base64 exige que o comprimento da string seja múltiplo de 4
        let padding = match standard.len() % 4 {
            2 => "==",
            3 => "=",
            _ => "",
        };
        let padded = format!("{standard}{padding}");

        // Decodifica — .unwrap_or_default() retorna Vec vazio se falhar (Base64 inválido)
        // unwrap_or_default() = se for Err, retorna o valor padrão do tipo (Vec vazio aqui)
        base64::engine::general_purpose::STANDARD
            .decode(&padded)
            .unwrap_or_default()
    }

    // Faz o parse da URL do Mega e retorna (handle, key_bytes)
    //   handle    = identificador único do arquivo/pasta na API do Mega
    //   key_bytes = bytes da chave de descriptografia extraídos da URL
    //
    // Formatos de ARQUIVO suportados:
    //   Novo:   https://mega.nz/file/HANDLE#KEY
    //   Antigo: https://mega.nz/#!HANDLE!KEY
    //
    // Formato de PASTA (retorna None — use list_folder_files):
    //   https://mega.nz/folder/HANDLE#KEY
    pub fn parse_url(url: &str) -> Option<(String, Vec<u8>)> {
        // Links de pasta têm API diferente — rejeitamos aqui com mensagem útil
        if url.contains("/folder/") {
            eprintln!(
                "Mega: Links de PASTA (/folder/) retornam None em parse_url.\n\
                 Use list_folder_files() para listar arquivos de uma pasta."
            );
            return None;
        }

        // Formato novo: /file/HANDLE#KEY
        // Divide pelo '#' para separar handle (antes) de KEY (depois)
        if let Some(pos) = url.find("/file/") {
            let after = &url[pos + 6..]; // Avança 6 posições = len("/file/")
            let parts: Vec<&str> = after.splitn(2, '#').collect();
            if parts.len() == 2 {
                let handle = parts[0].to_string();
                let key_bytes = Self::mega_base64_decode(parts[1]);
                // 16 bytes = mínimo para AES-128; chaves válidas têm 16 ou 32 bytes
                if key_bytes.len() >= 16 {
                    return Some((handle, key_bytes));
                }
            }
        }

        // Formato antigo: /#!HANDLE!KEY
        // Usa '!' como separador em vez de '#'
        if let Some(pos) = url.find("#!") {
            let after = &url[pos + 2..]; // Avança 2 = len("#!")
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

    // Deriva a chave AES-128 (16 bytes) e o IV (16 bytes) a partir dos 32 bytes da URL
    //
    // O Mega armazena 32 bytes na URL com o seguinte layout:
    //   bytes[0..16]  = chave AES obfuscada por XOR com nonce+mac
    //   bytes[16..24] = nonce de 8 bytes — vira os primeiros 8 bytes do IV do AES-CTR
    //   bytes[24..32] = mac de verificação de integridade (não usado para download)
    //
    // Para obter a chave real: XOR byte a byte dos primeiros 16 com os bytes 16..32
    // É como uma operação bitwise simples em PHP: $key[$i] = $a[$i] ^ $b[$i]
    pub fn derive_key_and_iv(key_bytes: &[u8]) -> ([u8; 16], [u8; 16]) {
        // Links antigos (16 bytes) têm só a chave AES direta, sem XOR e sem IV/nonce
        if key_bytes.len() < 32 {
            let mut key = [0u8; 16];
            let len = key_bytes.len().min(16);
            key[..len].copy_from_slice(&key_bytes[..len]);
            return (key, [0u8; 16]);
        }

        // Chave AES real = XOR dos primeiros 16 bytes com os bytes 16..32
        let mut aes_key = [0u8; 16];
        for i in 0..16 {
            aes_key[i] = key_bytes[i] ^ key_bytes[i + 16];
        }

        // IV do AES-CTR = nonce (bytes 16..24) nos primeiros 8 bytes + 8 zeros
        // O AES-CTR usa o IV como contador inicial — o Mega usa nonce de 64 bits
        let mut iv = [0u8; 16];
        iv[..8].copy_from_slice(&key_bytes[16..24]);

        (aes_key, iv)
    }

    // Consulta a API do Mega para obter a URL temporária de download de um arquivo
    // Retorna (url_temporaria, tamanho_em_bytes)
    //
    // O Mega usa uma API RPC JSON: enviamos um array de comandos, recebemos array de respostas
    // É como fazer um POST com json_encode($payload) e json_decode($response) no PHP
    async fn get_download_url(client: &reqwest::Client, handle: &str) -> Result<(String, u64)> {
        // Endpoint RPC — cada chamada recebe um ID sequencial (id=0 aqui)
        let api_url = "https://g.api.mega.co.nz/cs?id=0";

        // Comando "g" = get (obter URL de download), "p" = public file handle
        let body = serde_json::json!([{"a": "g", "p": handle}]);

        // ? = se der erro, propaga automaticamente (como throw no PHP)
        // context() adiciona contexto ao erro — como wrapping de exceções
        let resp: serde_json::Value = client
            .post(api_url)
            .json(&body)
            .send()
            .await
            .context("Falha ao chamar API do Mega")?
            .json()
            .await
            .context("Falha ao parsear resposta do Mega")?;

        // A resposta é um array JSON — pegamos o primeiro elemento (nossa resposta)
        let result = &resp[0];

        // Se o resultado for um número negativo, é código de erro da API do Mega
        // -2 = arquivo não encontrado, -3 = acesso negado, -9 = não existe, etc.
        if let Some(err_code) = result.as_i64() {
            return Err(anyhow!(
                "Mega retornou erro {err_code}. O arquivo pode não existir ou ser privado."
            ));
        }

        // "g" = campo da URL de download temporária (expira em ~1h)
        let download_url = result["g"]
            .as_str()
            .ok_or_else(|| anyhow!("API do Mega não retornou URL de download"))?
            .to_string();

        // "s" = tamanho do arquivo em bytes
        // unwrap_or(0) = se o campo não existir, retorna 0 — como ?? 0 no PHP
        let size = result["s"].as_u64().unwrap_or(0);

        Ok((download_url, size))
    }

    // Lista os arquivos de uma pasta pública do Mega
    // Retorna Vec de (handle_arquivo, key_bytes, atributos_brutos)
    //
    // API: POST https://g.api.mega.co.nz/cs?id=0&n=FOLDER_HANDLE
    //      Body: [{"a":"f","c":1,"r":1}]
    // Resposta: { "f": [ { "h": "handle", "k": "chave_cifrada", "a": "attrs_cifrados", "t": 0|1 } ] }
    //   t=0 = arquivo, t=1 = pasta
    pub async fn list_folder_files(
        client: &reqwest::Client,
        folder_handle: &str,
    ) -> Result<Vec<(String, Vec<u8>, String)>> {
        // Passa o handle da pasta como parâmetro ?n= na query string
        let api_url = format!("https://g.api.mega.co.nz/cs?id=0&n={folder_handle}");

        // Comando "f" = folder (listar nós da pasta), "c"=1 e "r"=1 = listar recursivo
        let body = serde_json::json!([{"a": "f", "c": 1, "r": 1}]);

        let resp: Value = client
            .post(&api_url)
            .json(&body)
            .send()
            .await
            .context("Falha ao chamar API de pasta do Mega")?
            .json()
            .await
            .context("Falha ao parsear resposta de pasta do Mega")?;

        // Se a resposta for um número negativo, é erro da API
        if let Some(err_code) = resp[0].as_i64() {
            return Err(anyhow!(
                "Mega retornou erro {err_code} ao listar pasta {folder_handle}"
            ));
        }

        // "f" = array de nós (arquivos e subpastas) da pasta
        let nodes = resp[0]["f"]
            .as_array()
            .ok_or_else(|| anyhow!("Resposta da pasta do Mega não contém campo 'f'"))?;

        let mut files = Vec::new();

        for node in nodes {
            // t=0 = arquivo, t=1 = pasta — só nos interessa arquivos
            if node["t"].as_u64() != Some(0) {
                continue;
            }

            // "h" = handle único do nó (arquivo)
            let handle = match node["h"].as_str() {
                Some(h) => h.to_string(),
                None => continue,
            };

            // "k" = chave do nó cifrada com a chave da pasta
            // Formato: "FOLDER_HANDLE:BASE64_KEY" ou só "BASE64_KEY"
            let key_field = match node["k"].as_str() {
                Some(k) => k,
                None => continue,
            };

            // Remove o prefixo "HANDLE:" se presente (formato com owner)
            let key_part = if let Some(colon_pos) = key_field.rfind(':') {
                &key_field[colon_pos + 1..]
            } else {
                key_field
            };

            let key_bytes = Self::mega_base64_decode(key_part);
            if key_bytes.len() < 16 {
                continue;
            }

            // "a" = atributos do nó cifrados (contém o nome do arquivo em JSON)
            // Guardamos como string bruta — a descriptografia completa exigiria
            // implementar AES-CBC com a chave do nó, que é mais complexa
            let attrs_raw = node["a"].as_str().unwrap_or("").to_string();

            files.push((handle, key_bytes, attrs_raw));
        }

        Ok(files)
    }
}

// Como uma interface PHP implementada por MegaProvider
// Implementa os métodos get_file_info() e download() definidos no trait Provider
impl Provider for MegaProvider {
    fn name(&self) -> &str { "Mega" }

    // async fn = retorna uma Promise implícita (como no JS/PHP async)
    // A sintaxe Box::pin(async move { ... }) é a forma manual de escrever async fn em traits
    fn get_file_info<'a>(&'a self, url: &'a str)
        -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FileInfo>> + Send + 'a>>
    {
        Box::pin(async move {
            // Suporte a links de pasta: lista arquivos e retorna info do primeiro
            if url.contains("/folder/") {
                let folder_handle = {
                    // Extrai o handle da pasta da URL: /folder/HANDLE#KEY
                    let pos = url.find("/folder/")
                        .ok_or_else(|| anyhow!("URL de pasta do Mega inválida"))?;
                    let after = &url[pos + 8..]; // 8 = len("/folder/")
                    let handle = after.split('#').next()
                        .ok_or_else(|| anyhow!("Handle da pasta não encontrado na URL"))?;
                    handle.to_string()
                };

                let client = reqwest::Client::new();
                let files = Self::list_folder_files(&client, &folder_handle).await?;

                // Retorna FileInfo do primeiro arquivo encontrado na pasta
                if let Some((file_handle, _, _)) = files.first() {
                    let (_dl_url, size) = Self::get_download_url(&client, file_handle).await?;
                    return Ok(FileInfo {
                        filename: format!("mega_{file_handle}"),
                        size,
                        mime_type: None,
                    });
                } else {
                    return Err(anyhow!("Pasta do Mega está vazia ou sem arquivos acessíveis"));
                }
            }

            // Link de arquivo normal
            let (handle, _key_bytes) = Self::parse_url(url)
                .ok_or_else(|| anyhow!("URL do Mega inválida: {url}"))?;

            let client = reqwest::Client::new();
            let (_download_url, size) = Self::get_download_url(&client, &handle).await?;

            // O Mega cifra o nome do arquivo nos atributos do nó
            // Descriptografar o nome exigiria AES-CBC adicional
            // Por ora usamos o handle como identificador temporário
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
            // Suporte a links de pasta: baixa todos os arquivos em sequência
            if url.contains("/folder/") {
                let folder_handle = {
                    let pos = url.find("/folder/")
                        .ok_or_else(|| anyhow!("URL de pasta do Mega inválida"))?;
                    let after = &url[pos + 8..];
                    let handle = after.split('#').next()
                        .ok_or_else(|| anyhow!("Handle da pasta não encontrado na URL"))?;
                    handle.to_string()
                };

                let client = reqwest::Client::new();
                let files = Self::list_folder_files(&client, &folder_handle).await?;

                if files.is_empty() {
                    return Err(anyhow!("Pasta do Mega está vazia ou sem arquivos acessíveis"));
                }

                // Cria subpasta com o handle da pasta no destino
                // Equivalente a mkdir($dest_path . '/' . $folder_handle) no PHP
                let folder_dest = format!("{}/{}", dest_path.trim_end_matches('/'), folder_handle);
                tokio::fs::create_dir_all(&folder_dest).await?;

                let mut total_downloaded: u64 = 0;

                // Baixa cada arquivo da pasta em sequência
                for (file_handle, key_bytes, _attrs) in &files {
                    let (aes_key, iv) = Self::derive_key_and_iv(key_bytes);
                    let (download_url, file_size) = Self::get_download_url(&client, file_handle).await?;

                    let file_path = format!("{folder_dest}/{file_handle}");
                    let resp = client.get(&download_url).send().await?.error_for_status()?;

                    let mut file = tokio::fs::File::create(&file_path).await?;
                    let mut stream = resp.bytes_stream();

                    // Inicializa o cipher AES-128-CTR para este arquivo
                    let mut cipher = Aes128Ctr::new(&aes_key.into(), &iv.into());

                    while let Some(chunk) = stream.next().await {
                        let mut chunk = chunk?.to_vec();
                        // apply_keystream faz XOR do chunk com o keystream do AES-CTR
                        // Descriptografa os dados in-place (modifica o Vec diretamente)
                        cipher.apply_keystream(&mut chunk);
                        file.write_all(&chunk).await?;
                        total_downloaded += chunk.len() as u64;

                        let _ = progress_tx.send(ProgressUpdate {
                            bytes_downloaded: total_downloaded,
                            total_bytes: file_size,
                        }).await;
                    }

                    file.flush().await?;
                }

                return Ok(total_downloaded);
            }

            // --- Download de arquivo único ---

            // Passo 1: extrai handle e bytes da chave de descriptografia da URL
            let (handle, key_bytes) = Self::parse_url(url)
                .ok_or_else(|| anyhow!("URL do Mega inválida: {url}"))?;

            // Passo 2: deriva a chave AES-128 (16 bytes) e o IV (16 bytes) via XOR
            let (aes_key, iv) = Self::derive_key_and_iv(&key_bytes);

            // Passo 3: consulta a API do Mega para obter a URL temporária de download
            let client = reqwest::Client::new();
            let (download_url, total) = Self::get_download_url(&client, &handle).await?;

            // Passo 4: inicia o download em streaming — sem carregar tudo na memória
            let resp = client
                .get(&download_url)
                .send()
                .await?
                .error_for_status()?; // Lança erro se HTTP status != 2xx

            let mut file = tokio::fs::File::create(dest_path).await?;
            let mut stream = resp.bytes_stream();
            let mut downloaded: u64 = 0;

            // Inicializa o cipher AES-128-CTR
            // O cipher mantém estado interno (posição no keystream)
            // Chunks devem ser processados em ordem — a ordem importa no CTR
            let mut cipher = Aes128Ctr::new(&aes_key.into(), &iv.into());

            while let Some(chunk) = stream.next().await {
                // to_vec() converte Bytes para Vec<u8> para poder modificar in-place
                let mut chunk = chunk?.to_vec();

                // Descriptografa os bytes do chunk fazendo XOR com o keystream do AES-CTR
                cipher.apply_keystream(&mut chunk);

                file.write_all(&chunk).await?;
                downloaded += chunk.len() as u64;

                // Envia progresso para o handler; se o receptor foi dropado (download cancelado), ignora
                let _ = progress_tx
                    .send(ProgressUpdate { bytes_downloaded: downloaded, total_bytes: total })
                    .await;
            }

            // Garante que todos os bytes foram escritos no disco antes de retornar
            file.flush().await?;
            Ok(downloaded)
        })
    }
}
