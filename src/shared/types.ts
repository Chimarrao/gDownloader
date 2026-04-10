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
  status: DownloadStatus
  percent: number
  speedBps: number
  etaSec: number
  error: string
  outputPath?: string
  addedAt: number
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
  fontSize: number
  fontFamily: string
  uiZoom: number
  nativeNotification: boolean
}
