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
    chooseDirectory: () => Promise<string>
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
    accountInfo: (moduleId: string) => Promise<{ email: string } | null>
  }
  downloads: {
    add: (
      url: string,
      moduleId: string,
      title: string,
      size: number,
      destDir: string,
      selectedChildren?: string[]
    ) => Promise<DownloadItem>
    cancel: (id: string) => Promise<void>
    pause: (id: string) => Promise<void>
    resume: (id: string) => Promise<void>
    retry: (id: string) => Promise<void>
    restart: (id: string) => Promise<void>
    remove: (id: string) => Promise<void>
    clearFinished: () => Promise<void>
    list: () => Promise<DownloadItem[]>
    on: (channel: DownloadChannel, cb: (data: unknown) => void) => () => void
  }
  loadHistory: () => Promise<HistoryItem[]>
  saveHistory: (items: HistoryItem[]) => Promise<void>
  clearHistory: () => Promise<void>
  openPath: (path: string) => Promise<void>
  showInFolder: (path: string) => Promise<void>
  clipboard: {
    writeText: (text: string) => Promise<boolean>
  }
  system: {
    notify: (title: string, body?: string) => Promise<boolean>
  }
  archive: {
    extract: (archivePath: string) => Promise<string>
  }
}

declare global {
  interface Window {
    api: RendererApi
  }
}
