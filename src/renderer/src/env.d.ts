/// <reference types="vite/client" />

import type {
  AppSettingsSnapshot,
  ArchivePassword,
  CachedFileInfoSnapshot,
  CreateDownloadPackagePayload,
  DownloadEvent,
  DuplicateGroup,
  DownloadPackage,
  DownloadHistoryItem,
  DownloadItem,
  HistorySearchFilters,
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
  | 'download:verifying'
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
  remoteAccess: {
    info: () => Promise<{
      enabled: boolean
      running: boolean
      lanIp: string
      port: number
      username: string
      password: string
      url: string
      credentialUrl: string
      qrCodeDataUrl?: string
      error?: string
    }>
    generateCredentials: () => Promise<NonNullable<AppSettingsSnapshot['remoteAccess']>>
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
      selectedChildren?: string[],
      expectedHash?: { algorithm: string; value: string },
      duplicateActionOverride?: 'ask' | 'skip' | 'rename' | 'always_download',
    ) => Promise<DownloadItem>
    cancel: (id: string) => Promise<void>
    pause: (id: string) => Promise<void>
    resume: (id: string) => Promise<void>
    retry: (id: string) => Promise<void>
    restart: (id: string) => Promise<void>
    force: (id: string) => Promise<void>
    setPriority: (id: string, priority: number) => Promise<void>
    setSpeedLimit: (id: string, speedLimitKib: number) => Promise<void>
    remove: (id: string) => Promise<void>
    removeWithFiles: (id: string) => Promise<void>
    clearFinished: () => Promise<void>
    togglePin: (id: string) => Promise<void>
    duplicates: () => Promise<DuplicateGroup[]>
    events: (id: string) => Promise<DownloadEvent[]>
    list: () => Promise<DownloadItem[]>
    on: (channel: DownloadChannel, cb: (data: unknown) => void) => () => void
  }
  loadHistory: (filters?: HistorySearchFilters) => Promise<DownloadHistoryItem[]>
  saveHistory: (items: DownloadHistoryItem[]) => Promise<void>
  appendHistory: (item: DownloadHistoryItem) => Promise<void>
  historyHosts: () => Promise<string[]>
  removeHistoryItem: (id: string) => Promise<void>
  clearHistory: () => Promise<void>
  openPath: (path: string) => Promise<void>
  showInFolder: (path: string) => Promise<void>
  clipboard: {
    writeText: (text: string) => Promise<boolean>
    onLinkDetected: (
      cb: (payload: { url: string; provider: string; providerName?: string }) => void,
    ) => () => void
  }
  system: {
    notify: (title: string, body?: string) => Promise<boolean>
  }
  logs: {
    tail: (maxLines?: number) => Promise<{ path: string; lines: string[] }>
    watch: (cb: (payload: { path: string; lines: string[] }) => void) => () => void
  }
  archive: {
    extract: (archivePath: string) => Promise<string>
    autoExtract: (
      archivePath: string,
      passwords: string[],
    ) => Promise<{
      success: boolean
      outputDir?: string
      error?: string
      passwordUsed?: string
    }>
  }
  archivePasswords: {
    list: () => Promise<ArchivePassword[]>
    import: (passwords: string[]) => Promise<void>
    forget: (password: string) => Promise<void>
  }
  packages: {
    list: () => Promise<DownloadPackage[]>
    create: (payload: CreateDownloadPackagePayload) => Promise<DownloadPackage>
    remove: (id: string) => Promise<void>
    assign: (packageId: string, downloadId: string) => Promise<void>
    unassign: (downloadId: string) => Promise<void>
  }
  links: {
    importContainer: (file: File) => Promise<Array<{ url: string; filename: string; size: number }>>
  }
  captcha: {
    nopechaSolve: (params: {
      type: string
      sitekey: string
      pageurl: string
    }) => Promise<string | null>
    openWindow: (params: {
      provider?: string
      pageUrl: string
      sourceUrl?: string
    }) => Promise<string | null>
    submit: (id: string, token: string) => Promise<void>
  }
  mirrors: {
    search: (filename: string) => Promise<void>
    abort: () => void
    onEvent: (cb: (event: MirrorEvent) => void) => () => void
  }
  getRealtimeStats: () => Promise<{
    ticks: Array<{
      timestamp: number
      total_speed_bps: number
      per_host_speed: Record<string, number>
    }>
  }>
  config: {
    testProxy: () => Promise<{ ip: string }>
  }
  terabox: {
    netRequest: (params: {
      url: string
      method?: string
      headers?: Record<string, string>
      body?: string
    }) => Promise<unknown>
  }
  getBackendPort: () => Promise<number>
  tray: {
    updateStats: (data: { activeCount: number; speed: string }) => void
  }
}

declare global {
  interface Window {
    api: RendererApi
  }
}
