use anyhow::Result;
use crate::models::FileInfo;

// Declara os sub-módulos de cada provedor
pub mod gdrive;
pub mod mediafire;
pub mod mega;
pub mod pixeldrain;

// Atualização de progresso enviada pelo provider durante o download
// Usada para calcular velocidade e ETA na fila
#[derive(Debug, Clone)]
pub struct ProgressUpdate {
    pub bytes_downloaded: u64,
    pub total_bytes: u64, // 0 se o servidor não informar o tamanho total
}

// Trait = interface em Rust
// Todo provider deve implementar estes métodos
// Send + Sync = pode ser usado com segurança entre threads (necessário com tokio)
pub trait Provider: Send + Sync {
    // Retorna o nome do provider (para logs e exibição na UI)
    fn name(&self) -> &str;

    // Retorna informações do arquivo sem baixar (nome, tamanho, etc.)
    // É equivalente a um método async em uma interface TypeScript
    // A sintaxe com Pin<Box<dyn Future>> é necessária porque Rust não suporta
    // async em traits diretamente (ainda) — é como retornar uma Promise em JS
    fn get_file_info<'a>(
        &'a self,
        url: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FileInfo>> + Send + 'a>>;

    // Baixa o arquivo para dest_path, enviando atualizações pelo progress_tx
    // Retorna o total de bytes baixados
    fn download<'a>(
        &'a self,
        url: &'a str,
        dest_path: &'a str,
        progress_tx: tokio::sync::mpsc::Sender<ProgressUpdate>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u64>> + Send + 'a>>;
}

// Detecta qual provider deve tratar uma URL
// Retorna Box<dyn Provider> — um provider de tipo desconhecido em tempo de compilação
// É como retornar um objeto de interface no TypeScript: a função não sabe o tipo exato,
// só sabe que implementa Provider
pub fn detect_provider(url: &str) -> Option<Box<dyn Provider>> {
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
    None
}

// --- Testes de detecção de URL ---
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_mega() {
        let p = detect_provider("https://mega.nz/file/AbCdEfGh#key123");
        assert!(p.is_some());
        assert_eq!(p.unwrap().name(), "Mega");
    }

    #[test]
    fn test_detect_pixeldrain() {
        let p = detect_provider("https://pixeldrain.com/u/AbCdEfGh");
        assert!(p.is_some());
        assert_eq!(p.unwrap().name(), "PixelDrain");
    }

    #[test]
    fn test_detect_mediafire() {
        let p = detect_provider("https://www.mediafire.com/file/abc123/file.zip/file");
        assert!(p.is_some());
        assert_eq!(p.unwrap().name(), "MediaFire");
    }

    #[test]
    fn test_detect_gdrive() {
        let p = detect_provider("https://drive.google.com/file/d/1ABC/view");
        assert!(p.is_some());
        assert_eq!(p.unwrap().name(), "Google Drive");
    }

    #[test]
    fn test_unknown_url() {
        let p = detect_provider("https://example.com/file.zip");
        assert!(p.is_none());
    }
}
