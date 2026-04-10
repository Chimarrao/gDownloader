// Importa os macros de serialização JSON
use serde::{Deserialize, Serialize};

// --- Status de um download ---
// #[derive(...)] faz o Rust gerar código automático:
// - Serialize/Deserialize: converte para/de JSON
// - Debug: permite imprimir com {:?}
// - Clone: permite copiar o valor
// - PartialEq: permite comparar com ==
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")] // JSON usa snake_case: "in_progress" em vez de "InProgress"
pub enum DownloadStatus {
    Pending,      // Na fila, aguardando
    Downloading,  // Baixando agora
    Paused,       // Pausado pelo usuário
    Complete,     // Concluído com sucesso
    Error,        // Falhou
    Cancelled,    // Cancelado pelo usuário
}

// --- Representa um download na fila ---
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Download {
    pub id: String,           // ID único (UUID)
    pub url: String,          // URL original do arquivo
    pub provider: String,     // "mega", "mediafire", "gdrive", "pixeldrain"
    pub filename: String,     // Nome do arquivo
    pub size: u64,            // Tamanho em bytes (0 se desconhecido)
    pub dest_path: String,    // Caminho onde será salvo
    pub status: DownloadStatus,
    pub bytes_downloaded: u64, // Bytes já baixados
    pub speed_bps: u64,       // Velocidade atual em bytes/segundo
    pub eta_secs: u64,        // Tempo estimado restante em segundos
    pub error: Option<String>, // Mensagem de erro (se houver)
    pub created_at: u64,      // Unix timestamp de criação
}

// --- Informações de um arquivo antes de baixar ---
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub filename: String,
    pub size: u64,       // 0 se desconhecido
    pub mime_type: Option<String>,
}

// --- Evento enviado pelo WebSocket para a UI ---
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsEvent {
    // Atualização de progresso
    Progress {
        id: String,
        bytes: u64,
        total: u64,
        speed: u64,
        eta: u64,
        status: DownloadStatus,
    },
    // Download concluído
    Complete {
        id: String,
        path: String,
    },
    // Erro no download
    Error {
        id: String,
        message: String,
    },
    // URL detectada no clipboard
    ClipboardUrl {
        url: String,
        provider: String,
    },
}

// --- Body do POST /downloads ---
#[derive(Debug, Deserialize)]
pub struct AddDownloadRequest {
    pub url: String,
    pub dest_dir: String,  // Diretório onde salvar (sem o nome do arquivo)
}

// --- Resposta padrão de erro da API ---
#[derive(Debug, Serialize)]
pub struct ApiError {
    pub error: String,
}

impl ApiError {
    pub fn new(msg: impl Into<String>) -> Self {
        Self { error: msg.into() }
    }
}
