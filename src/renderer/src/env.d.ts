/// <reference types="vite/client" />

import type {
  AppSettingsSnapshot,
  CachedFileInfoSnapshot,
  DownloadHistoryItem,
  DownloadItem,
  FileInfo,
} from '../../shared/types'

interface ModuleSummary {
  id: string
  name: string
  icon: string
  color: string
  capabilities?: {
    maxParallelDownloadsFree?: number | null
    requiresBrowserHelper?: boolean
    supportsFolder?: boolean
    supportsManualAuth?: boolean
    supportsAutoCaptcha?: boolean
    freeCooldownSecs?: number | null
    requiresAccountForLargeFiles?: boolean
    supportsParallelParts?: boolean
  }
  accountState?: {
    connected: boolean
    verifiedAt?: string | null
  } | null
}

type DownloadChannel =
  | 'download:progress'
  | 'download:status'
  | 'download:complete'
  | 'download:error'
  | 'download:cancelled'

interface MirrorStartPayload {
  filename: string
  total: number
}

interface MirrorProgressPayload {
  current: number
  total: number
  searcher: string
  phase: string
  newResults: number
  totalResults: number
  rawResults: number
  rejectedResults: number
  durationMs: number
  error?: string | null
}

interface MirrorResultPayload {
  url: string
  source: string
  hoster?: string | null
  score: number
}

interface MirrorDonePayload {
  filename: string
  searchers: number
  total: number
  hosters: number
  durationMs: number
}

type MirrorEvent =
  | { type: 'start'; payload: MirrorStartPayload }
  | { type: 'progress'; payload: MirrorProgressPayload }
  | { type: 'log'; payload: string }
  | { type: 'result'; payload: MirrorResultPayload }
  | { type: 'done'; payload: MirrorDonePayload }
  | { type: 'error'; payload: string }

interface RendererApi {
  settings: {
    load: () => Promise<AppSettingsSnapshot>
    save: (s: AppSettingsSnapshot) => Promise<AppSettingsSnapshot>
    chooseDirectory: () => Promise<string>
  }
  modules: {
    list: () => Promise<ModuleSummary[]>
    detect: (url: string) => Promise<ModuleSummary | null>
    fileInfo: (moduleId: string, url: string) => Promise<FileInfo>
    cachedFileInfo: (moduleId: string, url: string) => Promise<CachedFileInfoSnapshot | null>
    isLoggedIn: (moduleId: string) => Promise<boolean>
  }
  auth: {
    isLoggedIn: (moduleId: string) => Promise<boolean>
    login: (moduleId: string, params: Record<string, string>) => Promise<void>
    logout: (moduleId: string) => Promise<void>
    accountInfo: (moduleId: string) => Promise<{ email: string; verifiedAt?: string } | null>
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
    force: (id: string) => Promise<void>
    remove: (id: string) => Promise<void>
    removeWithFiles: (id: string) => Promise<void>
    clearFinished: () => Promise<void>
    list: () => Promise<DownloadItem[]>
    on: (channel: DownloadChannel, cb: (data: unknown) => void) => () => void
  }
  loadHistory: () => Promise<DownloadHistoryItem[]>
  saveHistory: (items: DownloadHistoryItem[]) => Promise<void>
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
  captcha: {
    nopechaSolve: (params: { type: string; sitekey: string; pageurl: string }) => Promise<string | null>
    openWindow: (params: { provider?: string; pageUrl: string; sourceUrl?: string }) => Promise<string | null>
    submit: (id: string, token: string) => Promise<void>
  }
  mirrors: {
    search: (filename: string) => Promise<void>
    abort: () => void
    onEvent: (cb: (event: MirrorEvent) => void) => () => void
  }
  getBackendPort: () => Promise<number>
}

declare global {
  interface Window {
    api: RendererApi
  }
}
