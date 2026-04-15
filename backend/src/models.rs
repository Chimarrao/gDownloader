// Importa os macros de serialização/deserialização JSON do Serde
// Serde = biblioteca padrão do Rust para converter structs em JSON e vice-versa
// Equivalente ao json_encode/json_decode do PHP, mas com tipagem estática
use serde::{Deserialize, Serialize};

// --- Status de um download ---
// #[derive(...)] instrui o compilador a gerar implementações automaticamente:
//   Serialize/Deserialize = converte para/de JSON (como implements JsonSerializable no PHP)
//   Debug    = permite imprimir com {:?} para depuração
//   Clone    = permite copiar o valor com .clone() (como clone() em PHP)
//   PartialEq = permite comparar com == (como == em PHP para objetos value)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
// serde rename_all = os variants do enum viram snake_case no JSON
// "InProgress" → "in_progress", "Complete" → "complete", etc.
#[serde(rename_all = "snake_case")]
pub enum DownloadStatus {
    Pending,      // Na fila, aguardando vez para iniciar
    Downloading,  // Transferência em andamento agora
    Paused,       // Pausado pelo usuário (reservado para implementação futura)
    Complete,     // Concluído com sucesso
    Error,        // Falhou com erro (ver campo error no Download)
    Cancelled,    // Cancelado pelo usuário via DELETE /downloads/:id
}

// --- Representa um download na fila ---
// Serializado como JSON para a API REST e para o WebSocket
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Download {
    pub id: String,              // UUID único gerado ao criar (como um ID de banco de dados)
    pub url: String,             // URL original fornecida pelo usuário
    pub provider: String,        // Nome do provider: "Mega", "MediaFire", "Google Drive", "PixelDrain"
    pub filename: String,        // Nome do arquivo no disco
    pub size: u64,               // Tamanho total em bytes (0 se o servidor não informar)
    pub dest_path: String,       // Caminho absoluto onde o arquivo será salvo
    pub status: DownloadStatus,  // Estado atual — ver enum acima
    pub bytes_downloaded: u64,   // Quantidade de bytes já transferidos
    pub speed_bps: u64,          // Velocidade atual em bytes por segundo
    pub eta_secs: u64,           // Tempo estimado para concluir, em segundos
    pub error: Option<String>,   // Option = pode ser Some("mensagem") ou None — como string|null no PHP
    pub created_at: u64,         // Timestamp Unix em segundos (como time() no PHP)
}

// --- Informações de um arquivo antes de iniciar o download ---
// Retornado pelo GET /file-info e usado internamente para nomear o arquivo
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub filename: String,
    pub size: u64,                    // 0 se o servidor não informar Content-Length
    pub mime_type: Option<String>,    // Option = Some("video/mp4") ou None — como ?string no PHP
}

// --- Evento enviado pelo WebSocket para a UI ---
// #[serde(tag = "type")] adiciona um campo "type" no JSON para identificar o variant
// Exemplo: { "type": "progress", "id": "...", "bytes": 1024, ... }
// É como um discriminated union no TypeScript ou um tagged serializer no PHP
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsEvent {
    // Progresso de download — enviado a cada chunk recebido
    Progress {
        id: String,
        bytes: u64,
        total: u64,
        speed: u64,
        eta: u64,
        status: DownloadStatus,
    },
    // Download finalizado com sucesso
    Complete {
        id: String,
        path: String,  // Caminho do arquivo salvo no disco
    },
    // Erro durante o download ou cancelamento
    Error {
        id: String,
        message: String,
    },
    // URL detectada no clipboard do usuário (funcionalidade futura)
    ClipboardUrl {
        url: String,
        provider: String,
    },
}

// --- Body do POST /downloads ---
// Deserialize = o Axum lê o JSON do body e preenche esta struct automaticamente
// É como $request->validated() no Laravel com um FormRequest
#[derive(Debug, Deserialize)]
pub struct AddDownloadRequest {
    pub url: String,
    pub dest_dir: String,  // Diretório de destino (sem o nome do arquivo)
}

// --- Resposta padrão de erro da API ---
// Todos os erros da API retornam { "error": "mensagem" }
#[derive(Debug, Serialize)]
pub struct ApiError {
    pub error: String,
}

// Como um class em PHP — agrupa métodos desta struct
impl ApiError {
    // Construtor: aceita qualquer tipo que implemente Into<String>
    // Funciona com &str, String, etc. — como type juggling no PHP mas seguro
    pub fn new(msg: impl Into<String>) -> Self {
        Self { error: msg.into() }
    }
}
