import type { DownloadChild, ExpectedHash, FileInfo } from '../../../shared/types'

export interface ModuleSummary {
  id: string
  name: string
  icon: string
  color: string
}

export interface SelectableChild extends DownloadChild {
  selected?: boolean
}

export interface RowFileInfo extends Omit<FileInfo, 'children'> {
  children?: SelectableChild[]
}

export interface CapturedRow {
  url: string
  displayName: string
  module: ModuleSummary | null
  info: RowFileInfo | null
  loading: boolean
  error: string
  availability: 'checking' | 'online' | 'offline' | 'unknown'
  cachedInfo: boolean
  selected: boolean
  expanded: boolean
  // Nome escolhido pelo usuário (renomear antes de baixar). Vazio = usa o detectado.
  customName?: string
  sourceUrls: string[]
  sourceLabels: string[]
  // Preenchido quando a URL já está na fila ou no histórico de concluídos.
  // 'queue' = já adicionado à fila; 'history' = já baixado antes.
  alreadyKnown?: 'queue' | 'history'
  destDir?: string
  expectedHash?: ExpectedHash
  youtubeOutputFormat?: string
  youtubeDownloadThumbnail?: boolean
  youtubeDownloadSubtitles?: boolean
  youtubeMultiAudio?: boolean
  // Divide o vídeo em arquivos separados por capítulo (--split-chapters do yt-dlp),
  // usando os capítulos que o próprio YouTube informa (descrição/timestamps).
  youtubeSplitChapters?: boolean
  // "Pacote completo": vídeo + todas as faixas de áudio + legendas + thumbnail +
  // descrição + metadados, como o jDownloader costuma entregar.
  youtubeDownloadPack?: boolean
  // Idiomas de legenda como lista separada por vírgula (ex.: "pt,en,es"). Vazio
  // usa o padrão das Configurações.
  youtubeSubLangs?: string
  // Kill switch por download: se marcado, o download nunca inicia (nem continua)
  // sem um circuito Tor ativo — não cai pra conexão direta silenciosamente.
  torRequired?: boolean
}

