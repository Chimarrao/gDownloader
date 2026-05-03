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
    Pending,         // Na fila, aguardando vez para iniciar
    Downloading,     // Transferência em andamento agora
    Verifying,       // Calculando hash antes de marcar como concluído
    Paused,          // Pausado pelo usuário
    Complete,        // Concluído com sucesso
    Corrupted,       // Hash não bateu; precisa baixar novamente
    Error,           // Falhou com erro (ver campo error no Download)
    Cancelled,       // Cancelado pelo usuário via DELETE /downloads/:id
    RateLimited,     // Bloqueado pelo servidor; aguardando retry_at
    WaitingCaptcha,  // Aguardando o usuário resolver um captcha
    DiskFull,        // Sem espaço em disco suficiente para iniciar o download
}

// --- Representa um download na fila ---
// Serializado como JSON para a API REST e para o WebSocket
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Download {
    pub id: String,              // UUID único gerado ao criar (como um ID de banco de dados)
    pub url: String,             // URL original fornecida pelo usuário
    pub provider: String,        // Nome do provider: "Mega", "MediaFire", "Google Drive", "PixelDrain"
    pub identity_key: String,    // Identidade lógica do item para deduplicação
    pub filename: String,        // Nome do arquivo no disco
    pub size: u64,               // Tamanho total em bytes (0 se o servidor não informar)
    pub dest_path: String,       // Caminho absoluto onde o arquivo será salvo
    pub status: DownloadStatus,  // Estado atual — ver enum acima
    pub bytes_downloaded: u64,   // Quantidade de bytes já transferidos
    pub speed_bps: u64,          // Velocidade atual em bytes por segundo
    pub eta_secs: u64,           // Tempo estimado para concluir, em segundos
    pub is_folder: bool,
    pub children: Option<Vec<FileChildInfo>>,
    pub retry_count: u32,
    pub max_retries: u32,
    pub speed_limit_kib: u64,
    pub parallel_parts: u32,
    pub selected_children: Option<Vec<String>>,
    pub expected_hash: Option<ExpectedHash>,
    pub retry_at: Option<u64>,
    pub captcha_type: Option<String>,     // "recaptcha2" | "hcaptcha"
    pub captcha_sitekey: Option<String>,
    pub captcha_page_url: Option<String>,
    pub captcha_token: Option<String>,    // preenchido quando o usuário resolve
    pub error: Option<String>,   // Option = pode ser Some("mensagem") ou None — como string|null no PHP
    pub priority: i32,           // Prioridade formal da fila; maior = inicia antes
    pub created_at: u64,         // Timestamp Unix em segundos (como time() no PHP)
    pub started_at: Option<u64>,
    pub completed_at: Option<u64>,
    pub last_progress_at: Option<u64>,
    #[serde(default)]
    pub pinned: bool,            // Se true, este download fica fixado no topo da lista
    #[serde(default)]
    pub package_id: Option<String>, // ID do pacote ao qual este download pertence
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HashAlgorithm {
    Md5,
    Sha1,
    Sha256,
    Crc32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExpectedHash {
    pub algorithm: HashAlgorithm,
    pub value: String,
}

// --- Informações de um arquivo antes de iniciar o download ---
// Retornado pelo GET /file-info e usado internamente para nomear o arquivo
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChildInfo {
    pub filename: String,
    pub size: u64,
    pub mime_type: Option<String>,
    pub is_folder: bool,
    pub path: Option<String>,
    pub source_url: Option<String>,
    pub bytes_downloaded: Option<u64>,
    pub speed_bps: Option<u64>,
    pub eta_secs: Option<u64>,
    pub status: Option<DownloadStatus>,
}

// --- Informações de um arquivo antes de iniciar o download ---
// Retornado pelo GET /file-info e usado internamente para nomear o arquivo
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub filename: String,
    pub size: u64,                    // 0 se o servidor não informar Content-Length
    pub mime_type: Option<String>,    // Option = Some("video/mp4") ou None — como ?string no PHP
    pub is_folder: bool,
    pub children: Option<Vec<FileChildInfo>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TeraboxAccountSecret {
    pub email: String,
    pub password: String,
    #[serde(default)]
    pub cookies: Vec<String>,
    pub verified_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BruploadAccountSecret {
    pub email: String,
    pub password: String,
    #[serde(default)]
    pub cookies: Vec<String>,
    pub verified_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SecureSettings {
    pub nopecha_api_key: Option<String>,
    pub terabox_account: Option<TeraboxAccountSecret>,
    pub brupload_account: Option<BruploadAccountSecret>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicSettings {
    pub theme: String,
    pub locale: String,
    pub output_dir: String,
    pub max_concurrent_downloads: usize,
    pub max_retries_per_download: u32,
    pub speed_limit_kib: u64,
    pub parallel_parts_per_download: u32,
    pub font_size: u32,
    pub font_family: String,
    pub ui_zoom: f32,
    pub native_notification: bool,
    #[serde(default)]
    pub clipboard_monitor_enabled: bool,
    pub accent_color: Option<String>,
    #[serde(default)]
    pub proxy_mode: String, // "none", "http", "socks5", "tor"
    #[serde(default)]
    pub proxy_host: String,
    #[serde(default)]
    pub proxy_port: u16,
    #[serde(default)]
    pub proxy_username: Option<String>,
    #[serde(default)]
    pub proxy_password: Option<String>,
    #[serde(default)]
    pub start_tor: bool,
    #[serde(default)]
    pub reserved_disk_mb: u64,
    #[serde(default)]
    pub use_reconnect_on_rate_limit: bool,
    #[serde(default)]
    pub reconnect_method: String, // "none" | "router_script" | "curl_command"
    #[serde(default)]
    pub reconnect_command: String,
    #[serde(default)]
    pub router_ip: String,
    #[serde(default)]
    pub post_download_action: String, // "none" | "shutdown" | "sleep" | "hibernate" | "custom_command" | "webhook"
    #[serde(default)]
    pub post_download_action_trigger: String, // "queue_empty" | "per_item"
    #[serde(default)]
    pub post_download_command: String,
    #[serde(default)]
    pub post_download_webhook_url: String,
    #[serde(default)]
    pub auto_extract: bool,
    #[serde(default)]
    pub password_list: Vec<String>,
    #[serde(default)]
    pub duplicate_action: String,
}

impl Default for PublicSettings {
    fn default() -> Self {
        Self {
            theme: "light".to_string(),
            locale: "pt-BR".to_string(),
            output_dir: "~/Downloads".to_string(),
            max_concurrent_downloads: 3,
            max_retries_per_download: 3,
            speed_limit_kib: 0,
            parallel_parts_per_download: 4,
            font_size: 14,
            font_family: "Inter".to_string(),
            ui_zoom: 1.0,
            native_notification: true,
            clipboard_monitor_enabled: false,
            accent_color: None,
            proxy_mode: "none".to_string(),
            proxy_host: "".to_string(),
            proxy_port: 0,
            proxy_username: None,
            proxy_password: None,
            start_tor: false,
            reserved_disk_mb: 500,
            use_reconnect_on_rate_limit: false,
            reconnect_method: "none".to_string(),
            reconnect_command: String::new(),
            router_ip: String::new(),
            post_download_action: "none".to_string(),
            post_download_action_trigger: "queue_empty".to_string(),
            post_download_command: String::new(),
            post_download_webhook_url: String::new(),
            auto_extract: false,
            password_list: Vec::new(),
            duplicate_action: "ask".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Package {
    pub id: String,
    pub name: String,
    pub color: String,
    pub comment: Option<String>,
    pub dest_dir_override: Option<String>,
    pub priority: i32,
    pub created_at: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatePackageRequest {
    pub name: String,
    pub color: Option<String>,
    pub comment: Option<String>,
    pub dest_dir_override: Option<String>,
    pub priority: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryItem {
    pub id: String,
    pub url: String,
    pub title: String,
    pub thumbnail: String,
    pub date: String,
    pub format_id: String,
    pub output_path: Option<String>,
    pub sha256_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateDownload {
    pub id: String,
    pub filename: String,
    pub url: String,
    pub provider: String,
    pub path: String,
    pub status: DownloadStatus,
    pub completed_at: Option<u64>,
    pub identity_key: Option<String>,
    pub sha256_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateGroup {
    pub kind: String,
    pub key: String,
    pub items: Vec<DuplicateDownload>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedFileInfo {
    pub url: String,
    pub provider_id: String,
    pub name: String,
    pub size: u64,
    pub mime_type: Option<String>,
    pub is_folder: bool,
    pub children: Option<Vec<FileChildInfo>>,
    pub cached_at: u64,
    pub last_checked_at: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyConfigMigration {
    pub version: i64,
    pub name: String,
    pub applied_at: u64,
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
        child_path: Option<String>,
        child_filename: Option<String>,
        child_bytes: Option<u64>,
        child_total: Option<u64>,
        child_speed: Option<u64>,
        child_eta: Option<u64>,
    },
    Verifying {
        id: String,
        bytes_done: u64,
        bytes_total: u64,
        algorithm: HashAlgorithm,
    },
    // Download finalizado com sucesso
    Complete {
        id: String,
        path: String,  // Caminho do arquivo salvo no disco
    },
    Status {
        id: String,
        status: DownloadStatus,
    },
    // Mudança de status com metadados extras (rate limit, captcha)
    StatusChanged {
        id: String,
        status: DownloadStatus,
        error: Option<String>,
        retry_at: Option<u64>,
        captcha_type: Option<String>,
        captcha_sitekey: Option<String>,
        captcha_page_url: Option<String>,
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
    // Tick de estatísticas de velocidade em tempo real — enviado a cada segundo
    StatsTick {
        timestamp: u64,
        total_speed_bps: u64,
        per_host_speed: std::collections::HashMap<String, u64>,
    },
    DuplicateDetected {
        id: String,
        existing_id: String,
        existing_path: String,
        filename: String,
    },
}

// --- Body do POST /downloads ---
// Deserialize = o Axum lê o JSON do body e preenche esta struct automaticamente
// É como $request->validated() no Laravel com um FormRequest
#[derive(Debug, Deserialize)]
pub struct AddDownloadRequest {
    pub url: String,
    pub dest_dir: String,  // Diretório de destino (sem o nome do arquivo)
    pub max_retries: Option<u32>,
    pub speed_limit_kib: Option<u64>,
    pub parallel_parts: Option<u32>,
    pub selected_children: Option<Vec<String>>,
    pub expected_hash: Option<ExpectedHash>,
    pub priority: Option<i32>,
    pub duplicate_action: Option<String>,
}

// --- Resposta padrão de erro da API ---
// Todos os erros da API retornam { "error": "mensagem" }
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiError {
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duplicate: Option<DuplicateDownload>,
}

// Como um class em PHP — agrupa métodos desta struct
impl ApiError {
    // Construtor: aceita qualquer tipo que implemente Into<String>
    // Funciona com &str, String, etc. — como type juggling no PHP mas seguro
    pub fn new(msg: impl Into<String>) -> Self {
        Self {
            error: msg.into(),
            duplicate: None,
        }
    }

    pub fn duplicate(msg: impl Into<String>, duplicate: DuplicateDownload) -> Self {
        Self {
            error: msg.into(),
            duplicate: Some(duplicate),
        }
    }
}
