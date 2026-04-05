import { createWriteStream } from 'fs'
import { pipeline } from 'stream/promises'
import type { DownloaderModule, FileInfo, DownloadOpts, ModuleAuth, AccountInfo } from '../shared/types'
import { loadCredentials, saveCredentials, deleteCredentials } from '../main/credentials-store'

// Use require to avoid TS type issues with megajs
// eslint-disable-next-line @typescript-eslint/no-require-imports
const megajs = require('megajs')

let _storage: unknown | null = null

async function tryAutoLogin(): Promise<void> {
  if (_storage) return
  const creds = loadCredentials('mega')
  if (creds?.email && creds?.password) {
    try {
      await auth.login(creds)
    } catch {
      // proceed as guest
    }
  }
}

const auth: ModuleAuth = {
  type: 'credentials',

  async login(params: Record<string, string>): Promise<void> {
    const { email, password } = params
    if (!email || !password) throw new Error('email and password são obrigatórios')
    const storage = new megajs.Storage({ email, password, autologin: false })
    await storage.login()
    _storage = storage
    saveCredentials('mega', { email, password })
  },

  async logout(): Promise<void> {
    if (_storage) {
      try { await (_storage as { close(): Promise<void> }).close() } catch { /* ignore */ }
      _storage = null
    }
    deleteCredentials('mega')
  },

  async isLoggedIn(): Promise<boolean> {
    return _storage !== null
  },

  async getAccountInfo(): Promise<AccountInfo> {
    if (!_storage) throw new Error('Não autenticado')
    const s = _storage as { email: string; getQuota?(): Promise<{ used: number; total: number }>; quota?: { used: number; total: number } }
    let quota: { used: number; total: number } | undefined
    try {
      if (typeof s.getQuota === 'function') {
        quota = await s.getQuota()
      } else if (s.quota) {
        quota = s.quota
      }
    } catch { /* ignore quota errors */ }
    return { email: s.email, quota }
  }
}

const megaModule: DownloaderModule = {
  id: 'mega',
  name: 'Mega',
  icon: 'mega.svg',
  color: '#D9272E',
  urlPatterns: [
    /mega\.nz\/(?:file|folder|#)/i,
    /mega\.co\.nz/i
  ],
  auth,

  async getFileInfo(url: string): Promise<FileInfo> {
    await tryAutoLogin()
    const file = megajs.File.fromURL(url)
    await file.loadAttributes()
    return {
      name: file.name as string,
      size: (file.size as number) ?? -1
    }
  },

  async download(url: string, destPath: string, opts: DownloadOpts): Promise<void> {
    await tryAutoLogin()
    const file = megajs.File.fromURL(url)
    await file.loadAttributes()

    const totalSize: number = (file.size as number) ?? 0
    let downloaded = 0
    const startTime = Date.now()

    const downloadStream = file.download({ maxConnections: 4 }) as NodeJS.ReadableStream & { destroy(): void }

    downloadStream.on('data', (chunk: Buffer) => {
      if (opts.signal.aborted) {
        downloadStream.destroy()
        return
      }
      downloaded += chunk.length
      const elapsed = (Date.now() - startTime) / 1000
      const speedBps = elapsed > 0 ? downloaded / elapsed : 0
      const percent = totalSize > 0 ? Math.min(100, Math.round((downloaded / totalSize) * 100)) : 0
      const etaSec = speedBps > 0 ? Math.round((totalSize - downloaded) / speedBps) : 0
      opts.onProgress(percent, speedBps, etaSec)
    })

    opts.signal.addEventListener('abort', () => downloadStream.destroy())

    const dest = createWriteStream(destPath)
    await pipeline(downloadStream, dest)
  }
}

export default megaModule
