import { randomBytes, timingSafeEqual } from 'crypto'
import { createReadStream, readFileSync, statSync } from 'fs'
import { createServer, type IncomingMessage, type Server, type ServerResponse } from 'http'
import { networkInterfaces } from 'os'
import { extname, join } from 'path'

import type { PersistedSettings } from '../shared/types'
import { logMain } from './debug-log'

type RemoteAccessSettings = NonNullable<PersistedSettings['remoteAccess']>

interface RemoteAccessInfo {
  enabled: boolean
  running: boolean
  allowLan: boolean
  lanIp: string
  bindHost: string
  port: number
  username: string
  password: string
  url: string
  /** Link de login por token de uso único (sem senha na URL). */
  credentialUrl: string
  qrCodeDataUrl?: string
  sessions: RemoteAccessSession[]
  insecureCredentials: boolean
  error?: string
}

export interface RemoteAccessSession {
  id: string
  ip: string
  userAgent: string
  createdAt: number
  lastSeenAt: number
  current?: boolean
}

interface RemoteAccessServerOptions {
  getRustPort: () => number | null
  getGoPort: () => number | null
  getSettings: () => PersistedSettings
  persistSettings: (settings: PersistedSettings) => Promise<void>
}

interface DownloadRow {
  id: string
  filename?: string
  title?: string
  url: string
  provider?: string
  status: string
  size?: number
  bytes_downloaded?: number
  speed_bps?: number
  eta_secs?: number
  dest_path?: string
}

// eslint-disable-next-line @typescript-eslint/no-require-imports
const QRCode = require('qrcode') as {
  toDataURL: (text: string, options?: Record<string, unknown>) => Promise<string>
}

export function generateStrongRemotePassword(): string {
  // 128 bits de entropia (32 hex). Prefixo só para legibilidade.
  return `gd-${randomBytes(16).toString('hex')}`
}

export function generateRemoteAccessCredentials(): RemoteAccessSettings {
  return {
    enabled: false,
    allowLan: false,
    username: 'gdownloader',
    password: generateStrongRemotePassword(),
    port: 9786,
  }
}

export function normalizeRemoteAccess(settings: PersistedSettings): RemoteAccessSettings {
  const current = settings.remoteAccess
  return {
    enabled: Boolean(current?.enabled),
    allowLan: Boolean(current?.allowLan),
    username: String(current?.username || 'gdownloader').trim() || 'gdownloader',
    password: String(current?.password ?? '').trim(),
    port: clampPort(current?.port ?? 9786),
  }
}

const WEAK_PASSWORDS = new Set([
  '',
  'gd-1234',
  '123456',
  'password',
  'admin',
  'gdownloader',
  'senha',
  '1234',
  '12345',
  '12345678',
])

export function credentialsAreInsecure(settings: RemoteAccessSettings): boolean {
  const username = settings.username.trim().toLowerCase()
  const password = settings.password
  if (WEAK_PASSWORDS.has(password.toLowerCase())) return true
  if (password.length < 16) return true
  if (username === 'admin' && password.length < 20) return true
  if (username === 'gdownloader' && password === 'gd-1234') return true
  return false
}

function bindHostFor(settings: RemoteAccessSettings): string {
  return settings.allowLan ? '0.0.0.0' : '127.0.0.1'
}

function displayHostFor(settings: RemoteAccessSettings, lanIp: string): string {
  return settings.allowLan ? lanIp : '127.0.0.1'
}

export function getLanIp(): string {
  const candidates: string[] = []
  for (const iface of Object.values(networkInterfaces())) {
    for (const item of iface ?? []) {
      if (item.family !== 'IPv4' || item.internal) continue
      candidates.push(item.address)
    }
  }

  return (
    candidates.find((ip) => ip.startsWith('192.168.')) ??
    candidates.find((ip) => /^172\.(1[6-9]|2\d|3[01])\./.test(ip)) ??
    candidates.find((ip) => ip.startsWith('10.')) ??
    candidates[0] ??
    '127.0.0.1'
  )
}

function clampPort(value: unknown): number {
  const port = Math.trunc(Number(value))
  if (!Number.isFinite(port)) return 9786
  return Math.min(65535, Math.max(1024, port))
}

function jsonResponse(res: ServerResponse, status: number, payload: unknown): void {
  res.writeHead(status, {
    'Content-Type': 'application/json; charset=utf-8',
    'Cache-Control': 'no-store',
  })
  res.end(JSON.stringify(payload))
}

const MIME_TYPES: Record<string, string> = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.mjs': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.svg': 'image/svg+xml',
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.jpeg': 'image/jpeg',
  '.webp': 'image/webp',
  '.ico': 'image/x-icon',
  '.woff': 'font/woff',
  '.woff2': 'font/woff2',
  '.ttf': 'font/ttf',
}

// Serve o build real do app Vue (out/renderer) pro cliente remoto — mesma UI
// do desktop, não uma página mobile separada mantida à parte. Em dev, em vez
// do build estático (que ficaria desatualizado a cada mudança), repassa pro
// próprio servidor de dev do Vite — o mesmo bundle com HMR que o Electron
// carrega localmente.
async function serveFrontend(res: ServerResponse, pathname: string): Promise<void> {
  const viteDevUrl = process.env.ELECTRON_RENDERER_URL
  if (viteDevUrl) {
    try {
      const upstream = await fetch(`${viteDevUrl}${pathname === '/' ? '/' : pathname}`)
      const body = Buffer.from(await upstream.arrayBuffer())
      res.writeHead(upstream.status, {
        'Content-Type': upstream.headers.get('content-type') ?? 'application/octet-stream',
        'Cache-Control': 'no-store',
      })
      res.end(body)
    } catch (error) {
      jsonResponse(res, 502, {
        error: `Servidor de dev do Vite indisponível: ${error instanceof Error ? error.message : String(error)}`,
      })
    }
    return
  }

  const rendererRoot = join(__dirname, '../renderer')
  const requested = pathname === '/' ? '/index.html' : pathname
  const fullPath = join(rendererRoot, requested)
  // Guarda contra path traversal (ex.: "/../../etc/passwd").
  if (!fullPath.startsWith(rendererRoot)) {
    jsonResponse(res, 403, { error: 'Caminho inválido' })
    return
  }

  try {
    const stat = statSync(fullPath)
    if (!stat.isFile()) throw new Error('not a file')
    const contentType = MIME_TYPES[extname(fullPath)] ?? 'application/octet-stream'
    res.writeHead(200, {
      'Content-Type': contentType,
      // index.html nunca cacheia (sempre busca a versão nova); assets com
      // hash no nome (gerados pelo Vite) podem cachear pra sempre.
      'Cache-Control': requested === '/index.html' ? 'no-store' : 'public, max-age=31536000, immutable',
    })
    createReadStream(fullPath).pipe(res)
  } catch {
    // Sem vue-router (SPA de aba única via v-if) — qualquer caminho desconhecido
    // ainda deve devolver o index.html, não 404.
    try {
      const html = readFileSync(join(rendererRoot, 'index.html'), 'utf8')
      res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8', 'Cache-Control': 'no-store' })
      res.end(html)
    } catch {
      jsonResponse(res, 404, {
        error: 'Build do app (out/renderer) não encontrado. Rode "npm run build" antes de usar o acesso remoto em produção.',
      })
    }
  }
}

function noContent(res: ServerResponse): void {
  res.writeHead(204, { 'Cache-Control': 'no-store' })
  res.end()
}

function readBody(req: IncomingMessage): Promise<unknown> {
  return new Promise((resolve, reject) => {
    const chunks: Buffer[] = []
    let size = 0
    req.on('data', (chunk: Buffer) => {
      size += chunk.byteLength
      if (size > 1024 * 1024) {
        reject(new Error('Payload muito grande'))
        req.destroy()
        return
      }
      chunks.push(chunk)
    })
    req.on('end', () => {
      const raw = Buffer.concat(chunks).toString('utf8').trim()
      if (!raw) {
        resolve({})
        return
      }
      try {
        resolve(JSON.parse(raw))
      } catch {
        reject(new Error('JSON inválido'))
      }
    })
    req.on('error', reject)
  })
}

function safeEqual(left: string, right: string): boolean {
  const leftBuffer = Buffer.from(left)
  const rightBuffer = Buffer.from(right)
  if (leftBuffer.byteLength !== rightBuffer.byteLength) return false
  return timingSafeEqual(leftBuffer, rightBuffer)
}

function isAuthorized(req: IncomingMessage, settings: RemoteAccessSettings): boolean {
  const header = req.headers.authorization ?? ''
  if (!header.startsWith('Basic ')) return false
  const decoded = Buffer.from(header.slice(6), 'base64').toString('utf8')
  const [username, ...passwordParts] = decoded.split(':')
  const password = passwordParts.join(':')
  return safeEqual(username, settings.username) && safeEqual(password, settings.password)
}

function authRequired(res: ServerResponse): void {
  res.writeHead(401, {
    'WWW-Authenticate': 'Basic realm="gDownloader Local"',
    'Content-Type': 'text/plain; charset=utf-8',
  })
  res.end('Autenticação necessária')
}

function buildTokenUrl(settings: RemoteAccessSettings, host: string, token: string): string {
  return `http://${host}:${settings.port}/auth?t=${encodeURIComponent(token)}`
}

function buildUrl(settings: RemoteAccessSettings, host: string): string {
  return `http://${host}:${settings.port}/`
}

async function qrCodeDataUrl(text: string): Promise<string> {
  return QRCode.toDataURL(text, {
    margin: 1,
    width: 220,
    color: {
      dark: '#101827',
      light: '#ffffff',
    },
  })
}

// eslint-disable-next-line @typescript-eslint/explicit-function-return-type
export function createRemoteAccessServer(options: RemoteAccessServerOptions) {
  let server: Server | null = null
  let activeSettings: RemoteAccessSettings = normalizeRemoteAccess(options.getSettings())
  let lastError = ''
  const loginTokens = new Map<string, number>()
  const sessions = new Map<string, RemoteAccessSession>()
  /** Rate-limit de auth por IP: bloqueia após N falhas. */
  const authFailures = new Map<string, { count: number; blockedUntil: number }>()
  const AUTH_MAX_FAILURES = 8
  const AUTH_BLOCK_MS = 60_000

  function requestIp(req: IncomingMessage): string {
    // Não confia em X-Forwarded-For (fácil de forjar em bind local/LAN).
    return req.socket.remoteAddress?.replace(/^::ffff:/, '') || 'desconhecido'
  }

  function isAuthBlocked(ip: string): boolean {
    const entry = authFailures.get(ip)
    if (!entry) return false
    if (entry.blockedUntil > Date.now()) return true
    if (entry.blockedUntil > 0 && entry.blockedUntil <= Date.now()) {
      authFailures.delete(ip)
    }
    return false
  }

  function recordAuthFailure(ip: string): void {
    const entry = authFailures.get(ip) ?? { count: 0, blockedUntil: 0 }
    entry.count += 1
    if (entry.count >= AUTH_MAX_FAILURES) {
      entry.blockedUntil = Date.now() + AUTH_BLOCK_MS
      entry.count = 0
      logMain('remote-access', 'IP bloqueado temporariamente por falhas de auth', { ip })
    }
    authFailures.set(ip, entry)
  }

  function clearAuthFailures(ip: string): void {
    authFailures.delete(ip)
  }

  function parseCookies(req: IncomingMessage): Record<string, string> {
    const out: Record<string, string> = {}
    for (const chunk of String(req.headers.cookie ?? '').split(';')) {
      const [key, ...rest] = chunk.trim().split('=')
      if (key) out[key] = decodeURIComponent(rest.join('=') || '')
    }
    return out
  }

  function createLoginToken(): string {
    const token = randomBytes(24).toString('hex')
    loginTokens.set(token, Date.now() + 5 * 60_000)
    return token
  }

  function createSession(req: IncomingMessage): RemoteAccessSession {
    const session: RemoteAccessSession = {
      id: randomBytes(18).toString('hex'),
      ip: requestIp(req),
      userAgent: String(req.headers['user-agent'] ?? 'desconhecido'),
      createdAt: Date.now(),
      lastSeenAt: Date.now(),
    }
    sessions.set(session.id, session)
    return session
  }

  function sessionFromRequest(req: IncomingMessage): RemoteAccessSession | null {
    const id = parseCookies(req).gdl_remote_session
    if (!id) return null
    const session = sessions.get(id)
    if (!session) return null
    session.lastSeenAt = Date.now()
    return session
  }

  function activeSessions(currentId?: string): RemoteAccessSession[] {
    return [...sessions.values()]
      .sort((left, right) => right.lastSeenAt - left.lastSeenAt)
      .map((session) => ({ ...session, current: session.id === currentId }))
  }

  async function backendFetch(path: string, init?: RequestInit): Promise<Response> {
    const rustPort = options.getRustPort()
    if (!rustPort) {
      throw new Error('Backend Rust ainda não está disponível')
    }
    return fetch(`http://127.0.0.1:${rustPort}${path}`, init)
  }

  // Alguns caminhos (ex.: /capture-state) só existem no sidecar Go — mesmo
  // allowlist de rotas migradas que o preload usa no app principal
  // (src/preload/index.ts, goRoutes), só que decidido aqui no servidor em vez
  // do cliente.
  const GO_ONLY_PREFIXES = ['/capture-state']
  async function goFetch(path: string, init?: RequestInit): Promise<Response> {
    const goPort = options.getGoPort()
    if (!goPort) {
      throw new Error('Backend Go ainda não está disponível')
    }
    return fetch(`http://127.0.0.1:${goPort}${path}`, init)
  }
  async function backendFetchAny(path: string, init?: RequestInit): Promise<Response> {
    if (GO_ONLY_PREFIXES.some((prefix) => path.startsWith(prefix))) {
      return goFetch(path, init)
    }
    return backendFetch(path, init)
  }
  async function backendJsonAny<T>(path: string, init?: RequestInit): Promise<T> {
    const response = await backendFetchAny(path, init)
    if (!response.ok) {
      const body = await response.json().catch(() => ({ error: `HTTP ${response.status}` }))
      throw new Error(body.error ?? `HTTP ${response.status}`)
    }
    return response.json() as Promise<T>
  }

  async function backendJson<T>(path: string, init?: RequestInit): Promise<T> {
    const response = await backendFetch(path, init)
    if (!response.ok) {
      const body = await response.json().catch(() => ({ error: `HTTP ${response.status}` }))
      throw new Error(body.error ?? `HTTP ${response.status}`)
    }
    return response.json() as Promise<T>
  }

  async function proxyNoContent(path: string, init?: RequestInit): Promise<void> {
    const response = await backendFetch(path, init)
    if (!response.ok) {
      const body = await response.json().catch(() => ({ error: `HTTP ${response.status}` }))
      throw new Error(body.error ?? `HTTP ${response.status}`)
    }
  }

  // Transmite o arquivo de um download para o dispositivo que acessa via rede.
  // Suporta Range (206) para retomada automática do navegador em conexões instáveis.
  async function serveDownloadFile(req: IncomingMessage, res: ServerResponse, id: string): Promise<void> {
    const downloads = await backendJson<DownloadRow[]>('/downloads').catch(() => [] as DownloadRow[])
    const download = downloads.find((row) => row.id === id)
    if (!download?.dest_path) {
      jsonResponse(res, 404, { error: 'Arquivo não encontrado' })
      return
    }

    let stat
    try {
      stat = statSync(download.dest_path)
    } catch {
      jsonResponse(res, 404, { error: 'Arquivo ainda não está disponível no disco' })
      return
    }
    if (!stat.isFile()) {
      jsonResponse(res, 404, { error: 'Arquivo indisponível' })
      return
    }

    const fileSize = stat.size
    const filename = download.filename || download.title || 'download'
    const baseHeaders: Record<string, string> = {
      'Content-Type': 'application/octet-stream',
      'Accept-Ranges': 'bytes',
      'Content-Disposition': `attachment; filename*=UTF-8''${encodeURIComponent(filename)}`,
      'Cache-Control': 'no-store',
    }

    const rangeHeader = req.headers.range
    let start = 0
    let end = fileSize - 1
    let status = 200
    if (rangeHeader) {
      const match = /bytes=(\d*)-(\d*)/.exec(rangeHeader)
      if (match) {
        if (match[1]) start = Number.parseInt(match[1], 10)
        if (match[2]) end = Number.parseInt(match[2], 10)
      }
      if (Number.isNaN(start) || start < 0) start = 0
      if (Number.isNaN(end) || end >= fileSize) end = fileSize - 1
      if (start > end || start >= fileSize) {
        res.writeHead(416, { 'Content-Range': `bytes */${fileSize}` })
        res.end()
        return
      }
      status = 206
      baseHeaders['Content-Range'] = `bytes ${start}-${end}/${fileSize}`
    }
    baseHeaders['Content-Length'] = String(end - start + 1)

    res.writeHead(status, baseHeaders)
    if (req.method === 'HEAD') {
      res.end()
      return
    }

    const stream = createReadStream(download.dest_path, { start, end })
    stream.on('error', () => {
      if (!res.headersSent) jsonResponse(res, 500, { error: 'Falha ao ler o arquivo' })
      else res.destroy()
    })
    stream.pipe(res)
  }

  function publicRemoteSettings(settings: PersistedSettings): Record<string, unknown> {
    // Nunca vaza proxy, remoteAccess, cookies nem outros secrets pela UI remota.
    return {
      outputDir: settings.outputDir,
      maxConcurrentDownloads: settings.maxConcurrentDownloads,
      maxRetriesPerDownload: settings.maxRetriesPerDownload,
      speedLimitKib: settings.speedLimitKib ?? 0,
      duplicateAction: settings.duplicateAction,
      nativeNotification: settings.nativeNotification,
    }
  }

  async function handleApi(req: IncomingMessage, res: ServerResponse, url: URL): Promise<void> {
    const pathname = url.pathname
    if (req.method === 'GET' && pathname === '/api/state') {
      const [downloads, settings, packages, stats] = await Promise.all([
        backendJson<DownloadRow[]>('/downloads').catch(() => []),
        backendJson<PersistedSettings>('/config/public'),
        backendJson<unknown[]>('/packages').catch(() => []),
        backendJson<unknown>('/stats/realtime').catch(() => ({ ticks: [] })),
      ])
      // dest_path completo no celular é útil para download; não é secret de app.
      jsonResponse(res, 200, {
        downloads,
        settings: publicRemoteSettings(settings),
        packages,
        stats,
      })
      return
    }

    if (req.method === 'GET' && pathname === '/api/settings') {
      const settings = await backendJson<PersistedSettings>('/config/public')
      jsonResponse(res, 200, publicRemoteSettings(settings))
      return
    }

    if (req.method === 'POST' && pathname === '/api/settings') {
      // Superfície mínima: a UI remota NÃO pode alterar remoteAccess, proxy,
      // credenciais, Tor nem pastas arbitrárias — só preferências de fila seguras.
      const payload = (await readBody(req)) as Record<string, unknown>
      const current = options.getSettings()
      const next: PersistedSettings = {
        ...current,
        maxConcurrentDownloads: clampInt(
          payload.maxConcurrentDownloads,
          current.maxConcurrentDownloads,
          1,
          20,
        ),
        speedLimitKib: clampInt(payload.speedLimitKib, current.speedLimitKib ?? 0, 0, 100_000_000),
        maxRetriesPerDownload: clampInt(
          payload.maxRetriesPerDownload,
          current.maxRetriesPerDownload ?? 3,
          0,
          1_000_000,
        ),
        nativeNotification: typeof payload.nativeNotification === 'boolean'
          ? payload.nativeNotification
          : current.nativeNotification,
        duplicateAction:
          payload.duplicateAction === 'ask'
          || payload.duplicateAction === 'skip'
          || payload.duplicateAction === 'rename'
          || payload.duplicateAction === 'always_download'
            ? payload.duplicateAction
            : current.duplicateAction,
        // remoteAccess e secrets nunca vêm do cliente remoto
        remoteAccess: current.remoteAccess,
      }
      await options.persistSettings(next)
      jsonResponse(res, 200, {
        maxConcurrentDownloads: next.maxConcurrentDownloads,
        speedLimitKib: next.speedLimitKib,
        maxRetriesPerDownload: next.maxRetriesPerDownload,
        nativeNotification: next.nativeNotification,
        duplicateAction: next.duplicateAction,
        outputDir: next.outputDir,
      })
      return
    }

    // Detecção de provedor + prévia de arquivo/pasta — só LEITURA, sem
    // superfície de configuração, seguro pro cliente remoto.
    if (req.method === 'GET' && pathname === '/api/modules/detect') {
      const target = url.searchParams.get('url') ?? ''
      const result = await backendJsonAny<unknown>(`/detect?url=${encodeURIComponent(target)}`).catch(() => null)
      jsonResponse(res, 200, result)
      return
    }
    if (req.method === 'GET' && pathname === '/api/modules/file-info') {
      const target = url.searchParams.get('url') ?? ''
      try {
        const result = await backendJsonAny<unknown>(`/file-info?url=${encodeURIComponent(target)}`)
        jsonResponse(res, 200, result)
      } catch (error) {
        jsonResponse(res, 502, { error: error instanceof Error ? error.message : String(error) })
      }
      return
    }
    if (req.method === 'GET' && pathname === '/api/modules/cached-file-info') {
      const target = url.searchParams.get('url') ?? ''
      const result = await backendJsonAny<unknown>(`/file-info/cache?url=${encodeURIComponent(target)}`).catch(() => null)
      jsonResponse(res, 200, result)
      return
    }

    // Texto bruto do Capturador de Links — mesma rota do app principal, só
    // repassando pro Go (único dono desse estado).
    if (pathname === '/api/capture-state') {
      if (req.method === 'GET') {
        const result = await backendJsonAny<unknown>('/capture-state').catch(() => ({ text: '' }))
        jsonResponse(res, 200, result)
        return
      }
      if (req.method === 'PUT') {
        const body = await readBody(req)
        await backendFetchAny('/capture-state', {
          method: 'PUT',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(body),
        }).catch(() => undefined)
        jsonResponse(res, 200, { ok: true })
        return
      }
    }

    if (req.method === 'GET' && pathname === '/api/system/disk-space') {
      const target = url.searchParams.get('path') ?? ''
      const result = await backendJson<unknown>(`/system/disk?path=${encodeURIComponent(target)}`).catch(() => null)
      jsonResponse(res, 200, result)
      return
    }

    if (req.method === 'GET' && pathname === '/api/providers') {
      const result = await backendJsonAny<unknown>('/providers').catch(() => [])
      jsonResponse(res, 200, result)
      return
    }

    if (req.method === 'GET' && pathname === '/api/sessions') {
      jsonResponse(res, 200, activeSessions(sessionFromRequest(req)?.id))
      return
    }

    const sessionMatch = pathname.match(/^\/api\/sessions\/([^/]+)$/)
    if (req.method === 'DELETE' && sessionMatch) {
      const [, id] = sessionMatch
      const current = sessionFromRequest(req)
      if (current?.id === id) {
        jsonResponse(res, 409, { error: 'Confirme no app principal antes de encerrar a sessão atual.' })
        return
      }
      sessions.delete(id)
      noContent(res)
      return
    }

    if (req.method === 'POST' && pathname === '/api/downloads') {
      const payload = await readBody(req) as {
        url?: string
        destDir?: string
        duplicateAction?: string
        selectedChildren?: string[]
        expectedHash?: { algorithm: string; value: string }
        filename?: string
        torRequired?: boolean
        parallelParts?: number
      }
      const current = options.getSettings()
      try {
        const created = await backendJson<unknown>('/downloads', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            url: payload.url,
            dest_dir: payload.destDir || current.outputDir,
            filename: payload.filename?.trim() || undefined,
            max_retries: current.infiniteRetries
              ? 4294967295
              : Math.max(0, Number(current.maxRetriesPerDownload ?? 1) - 1),
            speed_limit_kib: 0,
            parallel_parts: payload.parallelParts ?? current.parallelPartsPerDownload ?? 1,
            selected_children: payload.selectedChildren,
            expected_hash: payload.expectedHash,
            duplicate_action: payload.duplicateAction || current.duplicateAction || 'ask',
            tor_required: payload.torRequired ?? false,
          }),
        })
        jsonResponse(res, 201, created)
      } catch (error) {
        jsonResponse(res, 409, { error: error instanceof Error ? error.message : String(error) })
      }
      return
    }

    if (req.method === 'POST' && pathname === '/api/downloads/known-urls') {
      const payload = await readBody(req) as { urls?: string[] }
      const result = await backendJson<{ known?: Record<string, unknown> }>('/downloads/known-urls', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ urls: payload.urls ?? [] }),
      }).catch(() => ({ known: {} }))
      jsonResponse(res, 200, result.known ?? {})
      return
    }

    const actionMatch = pathname.match(/^\/api\/downloads\/([^/]+)\/(pause|resume|retry|restart|force|pin|remove|remove-with-files)$/)
    if (req.method === 'POST' && actionMatch) {
      const [, id, action] = actionMatch
      // Apagar arquivos do disco pela LAN é perigoso demais — só no app principal.
      if (action === 'remove-with-files') {
        jsonResponse(res, 403, {
          error: 'Remover com arquivos só está disponível no app principal por segurança.',
        })
        return
      }
      const method = action === 'remove' ? 'DELETE' : 'POST'
      const backendPath =
        action === 'remove'
          ? `/downloads/${encodeURIComponent(id)}`
          : `/downloads/${encodeURIComponent(id)}/${action === 'pin' ? 'pin' : action}`
      await proxyNoContent(backendPath, { method })
      noContent(res)
      return
    }

    jsonResponse(res, 404, { error: 'Rota remota não encontrada' })
  }

  async function handleRequest(req: IncomingMessage, res: ServerResponse): Promise<void> {
    const settings = activeSettings
    const url = new URL(req.url ?? '/', `http://${req.headers.host ?? 'localhost'}`)
    const ip = requestIp(req)

    if (isAuthBlocked(ip)) {
      res.writeHead(429, {
        'Content-Type': 'text/plain; charset=utf-8',
        'Retry-After': '60',
        'Cache-Control': 'no-store',
      })
      res.end('Muitas tentativas de autenticação. Tente novamente em 1 minuto.')
      return
    }

    if (req.method === 'GET' && url.pathname === '/auth') {
      const token = url.searchParams.get('t') ?? ''
      const expiresAt = loginTokens.get(token) ?? 0
      loginTokens.delete(token)
      if (!token || expiresAt < Date.now()) {
        recordAuthFailure(ip)
        authRequired(res)
        return
      }
      clearAuthFailures(ip)
      const session = createSession(req)
      res.writeHead(302, {
        Location: '/',
        // Sem Secure: HTTP local. HttpOnly + SameSite=Lax + Path=/
        'Set-Cookie': `gdl_remote_session=${encodeURIComponent(session.id)}; HttpOnly; SameSite=Lax; Path=/; Max-Age=604800`,
        'Cache-Control': 'no-store',
      })
      res.end()
      return
    }

    const session = sessionFromRequest(req)
    if (!session) {
      if (!isAuthorized(req, settings)) {
        recordAuthFailure(ip)
        authRequired(res)
        return
      }
    }
    clearAuthFailures(ip)

    try {
      // Servir o arquivo baixado de volta para o dispositivo (celular), com suporte a
      // Range/resume — assim o navegador retoma sozinho se o roteador oscilar.
      const fileMatch = url.pathname.match(/^\/api\/downloads\/([^/]+)\/file$/)
      if ((req.method === 'GET' || req.method === 'HEAD') && fileMatch) {
        await serveDownloadFile(req, res, decodeURIComponent(fileMatch[1]))
        return
      }

      if (url.pathname.startsWith('/api/')) {
        await handleApi(req, res, url)
        return
      }

      if (req.method === 'GET') {
        await serveFrontend(res, url.pathname)
        return
      }

      jsonResponse(res, 404, { error: 'Página não encontrada' })
    } catch (error) {
      jsonResponse(res, 500, { error: error instanceof Error ? error.message : String(error) })
    }
  }

  async function start(settings: RemoteAccessSettings): Promise<void> {
    activeSettings = settings
    lastError = ''
    if (server) return

    if (credentialsAreInsecure(settings)) {
      lastError =
        'Credenciais inseguras ou ausentes. Gere uma senha forte nas configurações antes de ativar o acesso remoto.'
      logMain('remote-access', lastError)
      return
    }

    const host = bindHostFor(settings)
    server = createServer((req, res) => {
      void handleRequest(req, res)
    })

    await new Promise<void>((resolve, reject) => {
      const activeServer = server!
      activeServer.once('error', reject)
      activeServer.listen(settings.port, host, () => {
        activeServer.off('error', reject)
        resolve()
      })
    }).catch((error) => {
      server?.close()
      server = null
      lastError = error instanceof Error ? error.message : String(error)
      throw error
    })

    logMain('remote-access', 'Servidor remoto local iniciado', {
      bind: host,
      allowLan: Boolean(settings.allowLan),
      url: buildUrl(settings, displayHostFor(settings, getLanIp())),
      username: settings.username,
    })
  }

  async function stop(): Promise<void> {
    if (!server) return
    const closing = server
    server = null
    await new Promise<void>((resolve) => closing.close(() => resolve()))
    logMain('remote-access', 'Servidor remoto local parado')
  }

  async function configure(rawSettings: PersistedSettings): Promise<void> {
    const next = normalizeRemoteAccess(rawSettings)
    const needsRestart =
      server
      && (next.port !== activeSettings.port
        || Boolean(next.allowLan) !== Boolean(activeSettings.allowLan))
    activeSettings = next
    if (!next.enabled) {
      await stop()
      lastError = ''
      return
    }
    if (needsRestart) await stop()
    try {
      await start(next)
    } catch (error) {
      logMain('remote-access', 'Falha ao iniciar servidor remoto local', error)
    }
  }

  async function info(rawSettings = options.getSettings()): Promise<RemoteAccessInfo> {
    const settings = normalizeRemoteAccess(rawSettings)
    const lanIp = getLanIp()
    const host = displayHostFor(settings, lanIp)
    const url = buildUrl(settings, host)
    // Link de login = token de uso único (nunca user:senha na URL).
    const tokenUrl = buildTokenUrl(settings, host, createLoginToken())
    const enabled = Boolean(settings.enabled)
    return {
      enabled,
      running: Boolean(server),
      allowLan: Boolean(settings.allowLan),
      lanIp,
      bindHost: bindHostFor(settings),
      port: settings.port,
      username: settings.username,
      password: settings.password,
      url,
      credentialUrl: tokenUrl,
      qrCodeDataUrl: enabled && !credentialsAreInsecure(settings)
        ? await qrCodeDataUrl(tokenUrl).catch(() => undefined)
        : undefined,
      sessions: activeSessions(),
      insecureCredentials: credentialsAreInsecure(settings),
      error: lastError || undefined,
    }
  }

  function revokeSession(id: string): boolean {
    return sessions.delete(id)
  }

  return {
    configure,
    stop,
    info,
    revokeSession,
  }
}

function clampInt(value: unknown, fallback: number, min: number, max: number): number {
  const n = Math.trunc(Number(value))
  if (!Number.isFinite(n)) return fallback
  return Math.min(max, Math.max(min, n))
}

