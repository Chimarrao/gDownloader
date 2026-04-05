import { createWriteStream } from 'fs'
import { Readable } from 'stream'
import type { DownloaderModule, FileInfo, DownloadOpts, ModuleAuth, AccountInfo } from '../shared/types'
import { loadCredentials, saveCredentials, deleteCredentials } from '../main/credentials-store'

const API = 'https://www.mediafire.com/api/1.5'
const APP_ID = '42335'

let _sessionToken: string | null = null
let _email: string | null = null

function extractQuickKey(url: string): string {
  const match =
    url.match(/mediafire\.com\/file\/([a-z0-9]+)/i) ??
    url.match(/mediafire\.com\/\?([a-z0-9]+)/i)
  if (!match) throw new Error(`Não foi possível extrair quick_key da URL: ${url}`)
  return match[1]
}

async function apiGet(endpoint: string, params: Record<string, string> = {}): Promise<Record<string, unknown>> {
  const qs = new URLSearchParams({ ...params, response_format: 'json' }).toString()
  const res = await fetch(`${API}${endpoint}.php?${qs}`)
  if (!res.ok) throw new Error(`MediaFire API HTTP ${res.status}`)
  const json = await res.json() as { response: { result: string; message?: string } & Record<string, unknown> }
  if (json.response.result !== 'Success') {
    throw new Error(json.response.message ?? 'MediaFire API error')
  }
  return json.response
}

async function apiPost(endpoint: string, body: Record<string, string>): Promise<Record<string, unknown>> {
  const res = await fetch(`${API}${endpoint}.php`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
    body: new URLSearchParams({ ...body, response_format: 'json' }).toString()
  })
  if (!res.ok) throw new Error(`MediaFire API HTTP ${res.status}`)
  const json = await res.json() as { response: { result: string; message?: string } & Record<string, unknown> }
  if (json.response.result !== 'Success') {
    throw new Error(json.response.message ?? 'MediaFire API error')
  }
  return json.response
}

async function tryAutoLogin(): Promise<void> {
  if (_sessionToken) return
  const creds = loadCredentials('mediafire')
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
    const res = await apiPost('/user/get_session_token', {
      email,
      password,
      application_id: APP_ID
    })
    _sessionToken = res.session_token as string
    _email = email
    saveCredentials('mediafire', { email, password })
  },

  async logout(): Promise<void> {
    if (_sessionToken) {
      try {
        await apiGet('/user/destroy_session', { session_token: _sessionToken })
      } catch { /* ignore */ }
    }
    _sessionToken = null
    _email = null
    deleteCredentials('mediafire')
  },

  async isLoggedIn(): Promise<boolean> {
    return _sessionToken !== null
  },

  async getAccountInfo(): Promise<AccountInfo> {
    if (!_sessionToken) throw new Error('Não autenticado')
    const res = await apiGet('/user/get_info', { session_token: _sessionToken })
    const info = res.user_info as { email: string; used_storage_size: string; storage_limit: string }
    return {
      email: info.email,
      quota: {
        used: parseInt(info.used_storage_size) || 0,
        total: parseInt(info.storage_limit) || 0
      }
    }
  }
}

const mediafireModule: DownloaderModule = {
  id: 'mediafire',
  name: 'MediaFire',
  icon: 'mediafire.svg',
  color: '#1299F3',
  urlPatterns: [/mediafire\.com/i],
  auth,

  async getFileInfo(url: string): Promise<FileInfo> {
    await tryAutoLogin()
    const quickKey = extractQuickKey(url)
    const params: Record<string, string> = { quick_key: quickKey }
    if (_sessionToken) params.session_token = _sessionToken
    const res = await apiGet('/file/get_info', params)
    const info = res.file_info as { filename: string; size: string; mimetype: string }
    return {
      name: info.filename,
      size: parseInt(info.size) || -1,
      mimeType: info.mimetype
    }
  },

  async download(url: string, destPath: string, opts: DownloadOpts): Promise<void> {
    await tryAutoLogin()
    const quickKey = extractQuickKey(url)
    const params: Record<string, string> = { quick_key: quickKey }
    if (_sessionToken) params.session_token = _sessionToken

    const res = await apiGet('/file/get_links', params)
    const links = res.links as Array<{ normal_download: string }>
    const directUrl = links[0]?.normal_download
    if (!directUrl) throw new Error('Não foi possível obter o link direto do arquivo')

    const dlRes = await fetch(directUrl, { signal: opts.signal as RequestInit['signal'] })
    if (!dlRes.ok) throw new Error(`Download falhou: HTTP ${dlRes.status}`)
    if (!dlRes.body) throw new Error('Response body is null')

    const totalSize = parseInt(dlRes.headers.get('content-length') ?? '0') || 0
    let downloaded = 0
    const startTime = Date.now()

    const reader = dlRes.body.getReader()
    const dest = createWriteStream(destPath)

    const nodeReadable = new Readable({ read() {} })
    nodeReadable.pipe(dest)

    try {
      while (true) {
        const { done, value } = await reader.read()
        if (done) break
        if (opts.signal.aborted) {
          reader.cancel()
          break
        }
        nodeReadable.push(value)
        downloaded += value.length
        const elapsed = (Date.now() - startTime) / 1000
        const speedBps = elapsed > 0 ? downloaded / elapsed : 0
        const percent = totalSize > 0 ? Math.min(100, Math.round((downloaded / totalSize) * 100)) : 0
        const etaSec = speedBps > 0 ? Math.round((totalSize - downloaded) / speedBps) : 0
        opts.onProgress(percent, speedBps, etaSec)
      }
    } finally {
      nodeReadable.push(null)
    }

    await new Promise<void>((resolve, reject) => {
      dest.on('finish', resolve)
      dest.on('error', reject)
    })
  }
}

export default mediafireModule
