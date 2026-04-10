// Tipos que espelham as structs do Rust (backend/src/models.rs)
// Devem ser mantidos em sincronia com o backend
// snake_case vem do Rust (serde serializa assim por padrão)

// Status de um download — espelha o enum DownloadStatus do Rust
export type DownloadStatus =
  | 'pending'      // Na fila, aguardando
  | 'downloading'  // Baixando agora
  | 'paused'       // Pausado
  | 'complete'     // Concluído com sucesso
  | 'error'        // Falhou
  | 'cancelled'    // Cancelado pelo usuário

// Espelha a struct Download do Rust
export interface Download {
  id: string
  url: string
  provider: string       // "Mega", "MediaFire", "Google Drive", "PixelDrain"
  filename: string
  size: number           // bytes (0 se desconhecido)
  dest_path: string
  status: DownloadStatus
  bytes_downloaded: number
  speed_bps: number      // bytes por segundo
  eta_secs: number       // segundos restantes estimados
  error: string | null
  created_at: number     // Unix timestamp em segundos
}

// Espelha a struct FileInfo do Rust
export interface FileInfo {
  filename: string
  size: number
  mime_type: string | null
}

// Espelha o enum WsEvent do Rust
// O serde serializa enums com #[serde(tag = "type")] assim:
// { "type": "progress", "id": "...", ... }
export type WsEvent =
  | {
      type: 'progress'
      id: string
      bytes: number
      total: number
      speed: number
      eta: number
      status: DownloadStatus
    }
  | { type: 'complete'; id: string; path: string }
  | { type: 'error'; id: string; message: string }
  | { type: 'clipboard_url'; url: string; provider: string }
