/// <reference types="vite/client" />

import type { DownloadItem, FileInfo, PersistedSettings } from '../../shared/types'

interface ModuleSummary {
  id: string
  name: string
  icon: string
  color: string
}

interface HistoryItem {
  id: string
  url: string
  title: string
  thumbnail: string
  date: string
  formatId: string
  outputPath?: string
}

type DownloadChannel =
  | 'download:progress'
  | 'download:status'
  | 'download:complete'
  | 'download:error'
  | 'download:cancelled'

interface RendererApi {
  settings: {
    load: () => Promise<PersistedSettings>
    save: (s: PersistedSettings) => Promise<void>
  }
  modules: {
    list: () => Promise<ModuleSummary[]>
    detect: (url: string) => Promise<ModuleSummary | null>
    fileInfo: (moduleId: string, url: string) => Promise<FileInfo>
  }
  auth: {
    isLoggedIn: (moduleId: string) => Promise<boolean>
    login: (moduleId: string, params: Record<string, string>) => Promise<void>
    logout: (moduleId: string) => Promise<void>
    accountInfo: (moduleId: string) => Promise<unknown>
  }
  downloads: {
    add: (
      url: string,
      moduleId: string,
      title: string,
      size: number,
      destDir: string
    ) => Promise<DownloadItem>
    cancel: (id: string) => Promise<void>
    list: () => Promise<DownloadItem[]>
    on: (channel: DownloadChannel, cb: (data: unknown) => void) => () => void
  }
  loadHistory: () => Promise<HistoryItem[]>
  saveHistory: (items: HistoryItem[]) => Promise<void>
  clearHistory: () => Promise<void>
  openPath: (path: string) => Promise<void>
  showInFolder: (path: string) => Promise<void>
}

declare global {
  interface Window {
    api: RendererApi
  }
}

