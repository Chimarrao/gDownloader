use anyhow::Result;
use crate::models::FileInfo;

// Declara os sub-módulos de cada provedor de download
// Em PHP seria algo como: require_once 'providers/MegaProvider.php';
pub mod gdrive;
pub mod mediafire;
pub mod mega;
pub mod pixeldrain;

// Estrutura de atualização de progresso enviada pelo provider durante o download
// Usada para calcular velocidade (bytes/s) e ETA no handler de downloads
#[derive(Debug, Clone)]
pub struct ProgressUpdate {
    pub bytes_downloaded: u64,
    pub total_bytes: u64, // 0 se o servidor não informar Content-Length
}

// Trait = como uma interface PHP — define o contrato que todo provider deve seguir
// Send + Sync = restrições de thread-safety: o provider pode ser movido entre threads
//              e compartilhado por referência entre threads — obrigatório com tokio
pub trait Provider: Send + Sync {
    // Retorna o nome legível do provider para logs e exibição na UI
    fn name(&self) -> &str;

    // Busca metadados do arquivo (nome, tamanho, MIME) sem baixar o conteúdo
    //
    // A assinatura complexa é necessária porque Rust não suporta `async fn` em traits
    // diretamente (ainda). Pin<Box<dyn Future>> é o equivalente manual de uma Promise:
    //   - Box<dyn Future> = heap-allocated, dyn Future = qualquer Future (como interface JS)
    //   - Pin = garante que o Future não seja movido na memória (requisito do async runtime)
    //   - Send = pode ser enviado para outra thread (necessário para tokio)
    //   - 'a = lifetime: o Future não pode viver mais que &self e &url
    fn get_file_info<'a>(
        &'a self,
        url: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FileInfo>> + Send + 'a>>;

    // Baixa o arquivo para dest_path e envia atualizações de progresso pelo canal
    // Result<u64> = Ok(bytes_baixados) ou Err(motivo_do_erro) — como try/catch mas em forma de valor
    fn download<'a>(
        &'a self,
        url: &'a str,
        dest_path: &'a str,
        // Sender do canal de progresso — o provider envia, o handler recebe
        // mpsc = Multiple Producer, Single Consumer (como uma fila de mensagens)
        progress_tx: tokio::sync::mpsc::Sender<ProgressUpdate>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u64>> + Send + 'a>>;
}

// Detecta qual provider consegue lidar com a URL fornecida
// Retorna Box<dyn Provider> = heap-allocated, dyn Provider = qualquer tipo que implemente
// Provider (como type hinting de interface no PHP: function foo(ProviderInterface $p))
// Option<T> = pode ser Some(provider) ou None — como nullable no PHP (Provider|null)
pub fn detect_provider(url: &str) -> Option<Box<dyn Provider>> {
    // Testa cada provider na ordem de prioridade
    // O primeiro que reconhecer a URL vence — sem overlap entre eles
    if mega::MegaProvider::matches(url) {
        return Some(Box::new(mega::MegaProvider));
    }
    if mediafire::MediaFireProvider::matches(url) {
        return Some(Box::new(mediafire::MediaFireProvider));
    }
    if gdrive::GDriveProvider::matches(url) {
        return Some(Box::new(gdrive::GDriveProvider));
    }
    if pixeldrain::PixelDrainProvider::matches(url) {
        return Some(Box::new(pixeldrain::PixelDrainProvider));
    }
    // URL não reconhecida por nenhum provider suportado
    None
}
