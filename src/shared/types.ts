import { DownloadStatus } from './constants'

export { DownloadStatus }

export interface QuotaInfo {
  used: number
  total: number
}

export interface AccountInfo {
  email: string
  quota?: QuotaInfo
}

export interface FileInfo {
  name: string
  size: number
  mimeType?: string
  isFolder?: boolean
  children?: DownloadChild[]
}

export interface DownloadChild {
  filename: string
  size: number
  mimeType?: string
  isFolder: boolean
  path?: string
  sourceUrl?: string
  bytesDownloaded?: number
  speedBps?: number
  etaSec?: number
  status?: DownloadStatus
}

export interface DownloadOpts {
  onProgress: (percent: number, speedBps: number, etaSec: number) => void
  signal: AbortSignal
}

export interface ModuleAuth {
  type: 'credentials' | 'oauth2'
  login(params: Record<string, string>): Promise<void>
  logout(): Promise<void>
  isLoggedIn(): Promise<boolean>
  getAccountInfo(): Promise<AccountInfo>
}

export interface DownloaderModule {
  id: string
  name: string
  icon: string
  color: string
  urlPatterns: RegExp[]
  auth?: ModuleAuth
  getFileInfo(url: string): Promise<FileInfo>
  download(url: string, destPath: string, opts: DownloadOpts): Promise<void>
}

export interface DownloadItem {
  id: string
  url: string
  moduleId: string
  title: string
  size: number
  isFolder?: boolean
  children?: DownloadChild[]
  status: DownloadStatus
  percent: number
  speedBps: number
  etaSec: number
  retryCount?: number
  maxRetries?: number
  retryAt?: number
  captchaType?: string
  captchaSitekey?: string
  captchaPageUrl?: string
  error: string
  expectedHash?: ExpectedHash
  outputPath?: string
  priority?: number
  pinned?: boolean
  packageId?: string
  addedAt: number
  startedAt?: number
  completedAt?: number
  lastProgressAt?: number
}

export interface DownloadPackage {
  id: string
  name: string
  color: string
  comment?: string | null
  destDirOverride?: string | null
  priority: number
  createdAt: number
}

export interface CreateDownloadPackagePayload {
  name: string
  color?: string
  comment?: string
  destDirOverride?: string
  priority?: number
}

export type HashAlgorithm = 'md5' | 'sha1' | 'sha256' | 'crc32'

export interface ExpectedHash {
  algorithm: HashAlgorithm
  value: string
}

export interface PersistedDownloadItem {
  id: string
  url: string
  moduleId: string
  title: string
  size: number
  status: string
  percent: number
  error: string
  outputPath?: string
  addedAt: number
}

export interface PersistedSettings {
  theme: string
  locale: string
  outputDir: string
  maxConcurrentDownloads: number
  maxRetriesPerDownload?: number
  speedLimitKib?: number
  parallelPartsPerDownload?: number
  fontSize: number
  fontFamily: string
  uiZoom: number
  nativeNotification: boolean
  clipboardMonitorEnabled: boolean
  accentColor?: string
  proxyMode?: string
  proxyHost?: string
  proxyPort?: number
  proxyUsername?: string
  proxyPassword?: string
  startTor?: boolean
  useReconnectOnRateLimit?: boolean
  reconnectMethod?: string
  reconnectCommand?: string
  routerIp?: string
  postDownloadAction?: string
  postDownloadActionTrigger?: string
  postDownloadCommand?: string
  postDownloadWebhookUrl?: string
  autoExtract?: boolean
  passwordList?: string[]
  duplicateAction?: 'ask' | 'skip' | 'rename' | 'always_download'
}

export interface AppSettingsSnapshot extends PersistedSettings {
  nopechaApiKey?: string
}

export interface DownloadHistoryItem {
  id: string
  url: string
  title: string
  thumbnail: string
  date: string
  formatId: string
  outputPath?: string
  sha256Hash?: string
}

export interface DuplicateDownload {
  id: string
  filename: string
  url: string
  provider: string
  path: string
  status: DownloadStatus
  completedAt?: number
  identityKey?: string
  sha256Hash?: string
}

export interface DuplicateGroup {
  kind: string
  key: string
  items: DuplicateDownload[]
}

export interface CachedFileInfoSnapshot extends FileInfo {
  providerId?: string
  cachedAt?: number
  lastCheckedAt?: number
}

export type DownloadStatusExtended =
  | 'pending'
  | 'downloading'
  | 'verifying'
  | 'paused'
  | 'complete'
  | 'corrupted'
  | 'error'
  | 'cancelled'
  | 'rate_limited'
  | 'waiting_captcha'
  | 'disk_full'
