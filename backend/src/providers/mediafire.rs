use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use scraper::{Html, Selector};
use tokio::io::AsyncWriteExt;

use crate::models::FileInfo;
use super::{Provider, ProgressUpdate};

pub struct MediaFireProvider;

// Como um class em PHP — agrupa os métodos estáticos e de instância do provider
impl MediaFireProvider {
    // Verifica se a URL pertence ao MediaFire
    pub fn matches(url: &str) -> bool {
        url.contains("mediafire.com")
    }

    // Extrai o link direto de download do HTML da página do MediaFire
    //
    // O MediaFire não expõe a URL de download diretamente — renderiza uma página HTML
    // com um botão que aponta para o CDN real. Precisamos fazer scraping dessa página.
    // É o mesmo que usar DOMDocument + XPath no PHP para buscar um elemento no HTML.
    pub fn extract_direct_link(html: &str) -> Option<String> {
        // Html::parse_document analisa o HTML como uma árvore DOM
        // Equivalente a $dom = new DOMDocument(); $dom->loadHTML($html); no PHP
        let document = Html::parse_document(html);

        // Estratégia 1: botão com id="downloadButton" (layout padrão do MediaFire)
        // Selector::parse("#downloadButton") funciona como CSS selector — igual ao JS:
        //   document.querySelector('#downloadButton')
        if let Ok(selector) = Selector::parse("#downloadButton") {
            if let Some(el) = document.select(&selector).next() {
                if let Some(href) = el.value().attr("href") {
                    if href.starts_with("http") {
                        return Some(href.to_string());
                    }
                }
            }
        }

        // Estratégia 2 (fallback): qualquer <a> apontando para o CDN do MediaFire
        // Padrões reconhecidos: download*.mediafire.com/get/ ou /download/
        if let Ok(selector) = Selector::parse("a[href]") {
            for el in document.select(&selector) {
                if let Some(href) = el.value().attr("href") {
                    if href.starts_with("http")
                        && href.contains(".mediafire.com")
                        && (href.contains("/get/") || href.contains("/download/"))
                    {
                        return Some(href.to_string());
                    }
                }
            }
        }

        // Nenhuma estratégia encontrou um link — arquivo pode estar expirado ou protegido
        None
    }

    // Cria um cliente HTTP com User-Agent de browser real
    // O MediaFire bloqueia requisições sem User-Agent ou com User-Agent de bot
    // É como passar um header Accept ou User-Agent no curl_setopt() do PHP
    fn http_client() -> Result<reqwest::Client> {
        Ok(reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
            // Segue até 5 redirects automaticamente (o MediaFire redireciona para o CDN)
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()?)
    }
}

// Como uma interface PHP implementada por MediaFireProvider
impl Provider for MediaFireProvider {
    fn name(&self) -> &str { "MediaFire" }

    // async fn = retorna uma Promise implícita (como no JS/PHP async)
    fn get_file_info<'a>(&'a self, url: &'a str)
        -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FileInfo>> + Send + 'a>>
    {
        Box::pin(async move {
            let client = Self::http_client()?;

            // Passo 1: baixa a página HTML do arquivo (a página de compartilhamento)
            let html = client.get(url).send().await?.text().await?;

            // Passo 2: faz scraping do HTML para encontrar o link de download direto
            // ? = se der erro, propaga automaticamente (como throw no PHP)
            let direct_url = Self::extract_direct_link(&html)
                .ok_or_else(|| anyhow!(
                    "Link de download não encontrado na página do MediaFire.\n\
                     Verifique se o link é público e está correto."
                ))?;

            // Passo 3: consulta os metadados do arquivo via HEAD request
            // HEAD = como GET mas sem body — só os headers (Content-Length, Content-Type)
            // É eficiente: não baixa o arquivo, só verifica tamanho e tipo
            let resp = match client.head(&direct_url).send().await {
                Ok(r) => Some(r),
                Err(_) => {
                    // Alguns servidores CDN bloqueiam HEAD — tenta GET com Range como fallback
                    // Range: bytes=0-0 = baixa só 1 byte para obter os headers
                    client
                        .get(&direct_url)
                        .header("Range", "bytes=0-0")
                        .send()
                        .await
                        .ok()
                }
            };

            let (size, mime_type) = if let Some(resp) = resp {
                // Tenta obter o tamanho total do arquivo
                let size = resp.content_length().unwrap_or_else(|| {
                    // Se Content-Length não veio, tenta extrair do Content-Range
                    // Formato do header: "bytes 0-0/TOTAL_SIZE"
                    // O total fica depois da '/' — parseamos com split e parse
                    resp.headers()
                        .get("content-range")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|s| {
                            s.split('/').last()
                                .and_then(|size_str| size_str.parse::<u64>().ok())
                        })
                        // Se for None/Err, retorna o valor padrão do tipo (0 para u64)
                        .unwrap_or(0)
                });

                // Lê o Content-Type para determinar o MIME type do arquivo
                let mime = resp
                    .headers()
                    .get("content-type")
                    .and_then(|v| v.to_str().ok())
                    .map(String::from);

                (size, mime)
            } else {
                // Nenhuma requisição funcionou — retorna tamanho 0 e sem MIME
                (0, None)
            };

            // Tenta extrair o nome do arquivo da URL
            // URLs do MediaFire têm formato: .../file/ID/NOME_DO_ARQUIVO/file
            // Pega o último segmento não vazio que não termine em ".file"
            let filename = url
                .split('/')
                .rev() // Reverte a ordem — começa do último segmento
                .find(|s| !s.is_empty() && !s.ends_with(".file"))
                .map(String::from)
                .unwrap_or_else(|| "arquivo_mediafire".to_string());

            Ok(FileInfo { filename, size, mime_type })
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
            let client = Self::http_client()?;

            // Passo 1: scraping da página para obter o link de download direto
            let html = client.get(url).send().await?.text().await?;
            let direct_url = Self::extract_direct_link(&html)
                .ok_or_else(|| anyhow!("Link de download não encontrado na página do MediaFire"))?;

            // Passo 2: baixa o arquivo do CDN em streaming
            // error_for_status() = lança erro se HTTP status != 2xx (como verificar $http_code no PHP)
            let resp = client.get(&direct_url).send().await?.error_for_status()?;

            let total = resp.content_length().unwrap_or(0);
            let mut file = tokio::fs::File::create(dest_path).await?;

            // bytes_stream() = stream de chunks de bytes (sem carregar tudo na memória)
            // Equivalente a ler um arquivo em chunks com fread() no PHP
            let mut stream = resp.bytes_stream();
            let mut downloaded: u64 = 0;

            while let Some(chunk) = stream.next().await {
                let chunk = chunk?;
                file.write_all(&chunk).await?;
                downloaded += chunk.len() as u64;

                // Envia atualização de progresso para o handler de downloads
                // Se o receptor foi dropado (download cancelado), ignora o erro
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
