import { createWriteStream } from 'fs'
import { createServer } from 'http'
import { shell } from 'electron'
import { google } from 'googleapis'
import type { DownloaderModule, FileInfo, DownloadOpts, ModuleAuth, AccountInfo } from '../shared/types'
import { loadCredentials, saveCredentials, deleteCredentials } from '../main/credentials-store'

const OAUTH_PORT = 54329
const REDIRECT_URI = `http://localhost:${OAUTH_PORT}/callback`

function getOAuthCredentials(): { clientId: string; clientSecret: string } | null {
  if (process.env.GDRIVE_CLIENT_ID && process.env.GDRIVE_CLIENT_SECRET) {
    return { clientId: process.env.GDRIVE_CLIENT_ID, clientSecret: process.env.GDRIVE_CLIENT_SECRET }
  }
  try {
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    const cfg = require('../../resources/gdrive-config.json')
    return { clientId: cfg.client_id, clientSecret: cfg.client_secret }
  } catch {
    return null
  }
}

function makeOAuth2Client() {
  const creds = getOAuthCredentials()
  if (!creds) throw new Error('Google Drive OAuth não configurado. Defina GDRIVE_CLIENT_ID e GDRIVE_CLIENT_SECRET.')
  return new google.auth.OAuth2(creds.clientId, creds.clientSecret, REDIRECT_URI)
}

function extractFileId(url: string): string {
  const match =
    url.match(/\/d\/([a-zA-Z0-9_-]+)/) ??
    url.match(/[?&]id=([a-zA-Z0-9_-]+)/)
  if (!match) throw new Error(`Não foi possível extrair file ID da URL: ${url}`)
  return match[1]
}

function waitForAuthCode(): Promise<string> {
  return new Promise((resolve, reject) => {
    const server = createServer((req, res) => {
      try {
        const reqUrl = new URL(req.url!, `http://localhost:${OAUTH_PORT}`)
        const code = reqUrl.searchParams.get('code')
        res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' })
        res.end('<h1>Autenticado com sucesso! Pode fechar esta aba.</h1>')
        server.close()
        if (code) resolve(code)
        else reject(new Error('Código OAuth não recebido'))
      } catch (e) {
        reject(e)
      }
    })
    server.listen(OAUTH_PORT, '127.0.0.1')
    server.on('error', reject)
    setTimeout(() => { server.close(); reject(new Error('Timeout OAuth (5 min)')) }, 300_000)
  })
}

type TokenData = Record<string, string>
let _tokens: TokenData | null = null

async function tryAutoLogin(): Promise<void> {
  if (_tokens) return
  const saved = loadCredentials('gdrive')
  if (saved) _tokens = saved
}

function getAuthedClient() {
  const client = makeOAuth2Client()
  if (_tokens) client.setCredentials(_tokens)
  return client
}

const auth: ModuleAuth = {
  type: 'oauth2',

  // params unused for OAuth2 — the flow opens the browser automatically
  async login(_params: Record<string, string>): Promise<void> {
    const client = makeOAuth2Client()
    const url = client.generateAuthUrl({
      access_type: 'offline',
      scope: ['https://www.googleapis.com/auth/drive.readonly'],
      prompt: 'consent'
    })
    await shell.openExternal(url)
    const code = await waitForAuthCode()
    const { tokens } = await client.getToken(code)
    _tokens = tokens as TokenData
    saveCredentials('gdrive', _tokens)
  },

  async logout(): Promise<void> {
    _tokens = null
    deleteCredentials('gdrive')
  },

  async isLoggedIn(): Promise<boolean> {
    await tryAutoLogin()
    return _tokens !== null
  },

  async getAccountInfo(): Promise<AccountInfo> {
    await tryAutoLogin()
    if (!_tokens) throw new Error('Não autenticado')
    const oauth2 = google.oauth2({ version: 'v2', auth: getAuthedClient() })
    const res = await oauth2.userinfo.get()
    return { email: res.data.email ?? 'desconhecido' }
  }
}

const gdriveModule: DownloaderModule = {
  id: 'gdrive',
  name: 'Google Drive',
  icon: 'gdrive.svg',
  color: '#1FA463',
  urlPatterns: [
    /drive\.google\.com/i,
    /docs\.google\.com\/.*\/d\//i
  ],
  auth,

  async getFileInfo(url: string): Promise<FileInfo> {
    await tryAutoLogin()
    const fileId = extractFileId(url)
    const driveAuth = _tokens ? getAuthedClient() : undefined
    const drive = google.drive({ version: 'v3', auth: driveAuth })
    const res = await drive.files.get({
      fileId,
      fields: 'name,size,mimeType',
      supportsAllDrives: true
    })
    return {
      name: res.data.name ?? fileId,
      size: parseInt(res.data.size ?? '0') || -1,
      mimeType: res.data.mimeType ?? undefined
    }
  },

  async download(url: string, destPath: string, opts: DownloadOpts): Promise<void> {
    await tryAutoLogin()
    const fileId = extractFileId(url)
    const driveAuth = _tokens ? getAuthedClient() : undefined
    const drive = google.drive({ version: 'v3', auth: driveAuth })

    // Get file size for progress
    let totalSize = 0
    try {
      const meta = await drive.files.get({ fileId, fields: 'size', supportsAllDrives: true })
      totalSize = parseInt(meta.data.size ?? '0') || 0
    } catch { /* ignore, progress will be 0% */ }

    const dlRes = await drive.files.get(
      { fileId, alt: 'media', supportsAllDrives: true },
      { responseType: 'stream' }
    )

    const stream = dlRes.data as NodeJS.ReadableStream & { destroy(): void }
    const dest = createWriteStream(destPath)
    let downloaded = 0
    const startTime = Date.now()

    stream.on('data', (chunk: Buffer) => {
      downloaded += chunk.length
      const elapsed = (Date.now() - startTime) / 1000
      const speedBps = elapsed > 0 ? downloaded / elapsed : 0
      const percent = totalSize > 0 ? Math.min(100, Math.round((downloaded / totalSize) * 100)) : 0
      const etaSec = speedBps > 0 ? Math.round((totalSize - downloaded) / speedBps) : 0
      opts.onProgress(percent, speedBps, etaSec)
    })

    opts.signal.addEventListener('abort', () => stream.destroy())

    await new Promise<void>((resolve, reject) => {
      stream.pipe(dest)
      dest.on('finish', resolve)
      dest.on('error', reject)
      stream.on('error', reject)
    })
  }
}

export default gdriveModule
