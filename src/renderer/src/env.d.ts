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
  cache: {
    stats: () => Promise<{
      totalBytes: number
      items: Array<{
        id: string
        label: string
        description: string
        bytes: number
        entries?: number
        clearable: boolean
      }>
    }>
    clear: (ids: string[]) => Promise<{
      totalBytes: number
      items: Array<{
        id: string
        label: string
        description: string
        bytes: number
        entries?: number
        clearable: boolean
      }>
    }>
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
      sessions: Array<{
        id: string
        ip: string
        userAgent: string
        createdAt: number
        lastSeenAt: number
        current?: boolean
      }>
      insecureCredentials: boolean
      error?: string
    }>
    generateCredentials: () => Promise<NonNullable<AppSettingsSnapshot['remoteAccess']>>
    revokeSession: (id: string) => Promise<boolean>
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
      filename?: string,
    ) => Promise<DownloadItem>
    cancel: (id: string) => Promise<void>
    pause: (id: string) => Promise<void>
    resume: (id: string) => Promise<void>
    retry: (id: string) => Promise<void>
    restart: (id: string) => Promise<void>
    force: (id: string) => Promise<void>
    setPriority: (id: string, priority: number) => Promise<void>
    setAutoTor: (id: string, enabled: boolean) => Promise<void>
    setSpeedLimit: (id: string, speedLimitKib: number) => Promise<void>
    move: (id: string, destDir: string) => Promise<void>
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
      cb: (payload: { url: string; urls?: string[]; provider: string; providerName?: string }) => void,
    ) => () => void
  }
  system: {
    notify: (title: string, body?: string) => Promise<boolean>
    diskSpace: (path: string) => Promise<{ path: string; freeBytes: number; totalBytes: number }>
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
  getDiskUsage: (path?: string) => Promise<{
    total: number
    available: number
    used: number
    mount: string
  }>
  getAllDisks: () => Promise<
    Array<{
      name: string
      mount: string
      total: number
      available: number
      used: number
      removable: boolean
      kind: string
    }>
  >
  config: {
    testProxy: () => Promise<{ ip: string; isTor: boolean }>
  }
  tor: {
    status: () => Promise<{
      state: 'disconnected' | 'connected'
      host: string
      port: number
      route: Array<{ role: string; country: string; code: string }>
      ip?: string
      country?: string
      countryCode?: string
      isTor?: boolean
    }>
    connect: () => Promise<{
      state: 'disconnected' | 'connected'
      host: string
      port: number
      route: Array<{ role: string; country: string; code: string }>
      ip?: string
      country?: string
      countryCode?: string
      isTor?: boolean
    }>
    disconnect: () => Promise<{
      state: 'disconnected' | 'connected'
      host: string
      port: number
      route: Array<{ role: string; country: string; code: string }>
      ip?: string
      country?: string
      countryCode?: string
      isTor?: boolean
    }>
    testConnection: () => Promise<{
      state: 'disconnected' | 'connected'
      host: string
      port: number
      route: Array<{ role: string; country: string; code: string }>
      ip?: string
      country?: string
      countryCode?: string
      isTor?: boolean
    }>
    newIdentity: () => Promise<{
      state: 'disconnected' | 'connected'
      host: string
      port: number
      route: Array<{ role: string; country: string; code: string }>
      ip?: string
      country?: string
      countryCode?: string
      isTor?: boolean
    }>
    bootstrapProgress: () => Promise<number>
    ensureRunning: () => Promise<{ running: boolean; port: number | null }>
    runtimeStatus: () => Promise<{ running: boolean; port: number | null }>
  }
  intercept: {
    status: () => Promise<{
      enabled: boolean
      proxyAddr: string
      caCertPath: string
      history: Array<{
        id: string
        url: string
        filename: string
        mimeType: string
        size: number
        status: string
        createdAt: number
      }>
    }>
    installCa: () => Promise<boolean>
    openProxySettings: () => Promise<boolean>
  }
  terabox: {
    netRequest: (params: {
      url: string
      method?: string
      headers?: Record<string, string>
      body?: string
    }) => Promise<unknown>
  }
  ytdlp: {
    status: () => Promise<{
      version: string | null
      updateAvailable: boolean
      state: 'ready' | 'downloading' | 'error'
      error?: string
    }>
    checkUpdate: () => Promise<{
      version: string | null
      updateAvailable: boolean
      state: 'ready' | 'downloading' | 'error'
      error?: string
    }>
    onProgress: (cb: (e: { bytesDownloaded: number; totalBytes: number }) => void) => () => void
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
