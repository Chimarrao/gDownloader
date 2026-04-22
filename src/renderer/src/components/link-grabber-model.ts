import type { DownloadChild, FileInfo } from '../../../shared/types'

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
  sourceUrls: string[]
  sourceLabels: string[]
}

export interface MirrorViewResult {
  url: string
  source: string
  hoster?: string | null
  score: number
}
