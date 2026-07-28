import { randomBytes, timingSafeEqual } from 'crypto'
import { createReadStream, statSync } from 'fs'
import { createServer, type IncomingMessage, type Server, type ServerResponse } from 'http'
import { networkInterfaces } from 'os'

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

function htmlResponse(res: ServerResponse, body: string): void {
  res.writeHead(200, {
    'Content-Type': 'text/html; charset=utf-8',
    'Cache-Control': 'no-store',
  })
  res.end(body)
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

  async function handleApi(req: IncomingMessage, res: ServerResponse, pathname: string): Promise<void> {
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
      const payload = await readBody(req) as { url?: string; destDir?: string; duplicateAction?: string }
      const current = options.getSettings()
      await backendJson('/downloads', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          url: payload.url,
          dest_dir: payload.destDir || current.outputDir,
          max_retries: current.maxRetriesPerDownload ?? 3,
          speed_limit_kib: current.speedLimitKib ?? 0,
          parallel_parts: current.parallelPartsPerDownload ?? 4,
          duplicate_action: payload.duplicateAction || current.duplicateAction || 'ask',
        }),
      })
      jsonResponse(res, 201, { ok: true })
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
        await handleApi(req, res, url.pathname)
        return
      }

      if (req.method === 'GET' && (url.pathname === '/' || url.pathname === '/index.html')) {
        htmlResponse(res, remoteHtml())
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

function remoteHtml(): string {
  return `<!doctype html>
<html lang="pt-BR">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>gDownloader Local</title>
  <style>
    :root { color-scheme: light dark; --bg:#f6f7fb; --surface:#fff; --text:#162033; --muted:#68748a; --line:#d9dee8; --accent:#2563eb; --danger:#dc2626; --ok:#16a34a; }
    @media (prefers-color-scheme: dark) { :root { --bg:#10131a; --surface:#181d27; --text:#eef2ff; --muted:#9ba7bd; --line:#293141; --accent:#60a5fa; --danger:#fb7185; --ok:#34d399; } }
    * { box-sizing:border-box; }
    body { margin:0; font-family: Inter, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; background:var(--bg); color:var(--text); }
    header { position:sticky; top:0; z-index:2; background:var(--surface); border-bottom:1px solid var(--line); padding:14px 16px; display:flex; align-items:center; justify-content:space-between; gap:12px; }
    h1 { margin:0; font-size:18px; }
    main { max-width:1040px; margin:0 auto; padding:16px; display:grid; gap:14px; }
    .tabs { display:flex; gap:8px; }
    button, input, select { font:inherit; }
    button { border:1px solid var(--line); background:var(--surface); color:var(--text); border-radius:8px; padding:8px 10px; cursor:pointer; }
    button.primary { background:var(--accent); color:white; border-color:var(--accent); }
    button.danger { color:var(--danger); }
    button:disabled { opacity:.55; cursor:not-allowed; }
    .card { background:var(--surface); border:1px solid var(--line); border-radius:8px; padding:14px; }
    .toolbar { display:flex; flex-wrap:wrap; gap:8px; align-items:center; }
    .grid { display:grid; gap:10px; }
    .settings-grid { grid-template-columns: repeat(auto-fit, minmax(230px, 1fr)); }
    label { display:grid; gap:6px; color:var(--muted); font-size:12px; }
    input, select { width:100%; border:1px solid var(--line); background:var(--surface); color:var(--text); border-radius:8px; padding:9px 10px; }
    .download { display:grid; grid-template-columns:1fr auto; gap:10px; align-items:center; padding:12px; border:1px solid var(--line); border-radius:8px; }
    .name { font-weight:700; word-break:break-word; }
    .meta { color:var(--muted); font-size:12px; margin-top:4px; display:flex; gap:8px; flex-wrap:wrap; }
    .bar { height:7px; background:color-mix(in srgb, var(--line), transparent 25%); border-radius:99px; overflow:hidden; margin-top:9px; }
    .fill { height:100%; background:var(--accent); width:0; }
    .status-complete .fill { background:var(--ok); }
    .status-error .fill, .status-corrupted .fill, .status-disk_full .fill { background:var(--danger); }
    .actions { display:flex; gap:6px; flex-wrap:wrap; justify-content:flex-end; }
    .device-dl { display:inline-flex; align-items:center; justify-content:center; gap:4px; background:var(--accent); color:#fff; border-radius:8px; padding:8px 12px; font-size:13px; font-weight:600; text-decoration:none; border:none; cursor:pointer; }
    .device-dl:hover { filter:brightness(1.06); }
    .hidden { display:none; }
    .empty { color:var(--muted); text-align:center; padding:26px; }
    .toast { position:fixed; left:50%; bottom:18px; transform:translateX(-50%); background:var(--text); color:var(--surface); padding:10px 14px; border-radius:8px; opacity:0; pointer-events:none; transition:.2s; }
    .toast.show { opacity:1; }
    @media (max-width: 720px) {
      header { padding:12px; }
      h1 { font-size:16px; }
      main { padding:10px; gap:10px; }
      .tabs { display:grid; grid-template-columns: repeat(3, 1fr); gap:6px; }
      .tabs button { padding:9px 6px; }
      .card { padding:12px; }
      .download { grid-template-columns:1fr; padding:11px; }
      .actions { justify-content:flex-start; display:grid; grid-template-columns:repeat(2, minmax(0, 1fr)); }
      .actions button, .actions .device-dl { width:100%; min-height:38px; box-sizing:border-box; }
      .device-dl { grid-column:1 / -1; }
      .meta { gap:6px; }
      .settings-grid { grid-template-columns:1fr; }
      input, select, button { min-height:40px; }
    }
  </style>
</head>
<body>
  <header>
    <h1>gDownloader Local</h1>
    <button id="refresh">Atualizar</button>
  </header>
  <main>
    <nav class="tabs">
      <button class="tab primary" data-tab="downloads">Fila</button>
      <button class="tab" data-tab="add">Adicionar</button>
      <button class="tab" data-tab="settings">Configurações</button>
    </nav>
    <section id="downloads" class="panel grid"></section>
    <section id="add" class="panel card hidden">
      <form id="addForm" class="grid">
        <label>URL<input id="downloadUrl" required placeholder="https://..." /></label>
        <label>Pasta de destino<input id="destDir" placeholder="Usar pasta padrão" /></label>
        <button class="primary" type="submit">Adicionar à fila</button>
      </form>
    </section>
    <section id="settings" class="panel card hidden">
      <form id="settingsForm" class="grid settings-grid"></form>
      <div class="toolbar" style="margin-top:12px">
        <button class="primary" type="button" id="saveSettings">Salvar configurações</button>
      </div>
    </section>
  </main>
  <div id="toast" class="toast"></div>
  <script>
    const state = { downloads: [], settings: {} };
    const panels = [...document.querySelectorAll('.panel')];
    const tabs = [...document.querySelectorAll('.tab')];
    const toast = document.getElementById('toast');
    function showToast(text) { toast.textContent = text; toast.classList.add('show'); setTimeout(() => toast.classList.remove('show'), 2200); }
    function fmtBytes(v) { if (!v) return '0 B'; const u=['B','KB','MB','GB','TB']; let i=0,n=v; while(n>=1024&&i<u.length-1){n/=1024;i++} return n.toFixed(i?1:0)+' '+u[i]; }
    function pct(d) { return d.size > 0 ? Math.min(100, Math.floor(((d.bytes_downloaded || 0) / d.size) * 100)) : 0; }
    async function api(path, options) {
      const res = await fetch(path, options);
      if (!res.ok) {
        const body = await res.json().catch(() => ({ error: 'HTTP ' + res.status }));
        throw new Error(body.error || ('HTTP ' + res.status));
      }
      if (res.status === 204) return null;
      return res.json();
    }
    function renderDownloads() {
      const root = document.getElementById('downloads');
      if (!state.downloads.length) {
        root.innerHTML = '<div class="card empty">Nenhum download na fila.</div>';
        return;
      }
      root.innerHTML = state.downloads.map(d => {
        const progress = pct(d);
        const active = ['downloading','paused','pending','rate_limited','waiting_captcha','error','corrupted'].includes(d.status);
        return '<article class="download status-' + d.status + '">' +
          '<div><div class="name">' + escapeHtml(d.filename || d.title || d.url) + '</div>' +
          '<div class="meta"><span>' + escapeHtml(d.provider || 'provider') + '</span><span>' + d.status + '</span><span>' + fmtBytes(d.bytes_downloaded || 0) + ' / ' + fmtBytes(d.size || 0) + '</span><span>' + fmtBytes(d.speed_bps || 0) + '/s</span></div>' +
          '<div class="bar"><div class="fill" style="width:' + progress + '%"></div></div></div>' +
          '<div class="actions">' +
          (d.status === 'complete' ? '<a class="device-dl" href="/api/downloads/' + encodeURIComponent(d.id) + '/file" download>⬇ Este dispositivo</a>' : '') +
          '<button data-action="pause" data-id="' + d.id + '">Pausar</button>' +
          '<button data-action="resume" data-id="' + d.id + '">Retomar</button>' +
          '<button data-action="force" data-id="' + d.id + '">Forçar</button>' +
          '<button data-action="retry" data-id="' + d.id + '">Retry</button>' +
          '<button class="danger" data-action="remove" data-id="' + d.id + '" ' + (active ? '' : '') + '>Remover</button>' +
          '</div></article>';
      }).join('');
    }
    function renderSettings() {
      const s = state.settings || {};
      document.getElementById('settingsForm').innerHTML = [
        '<p style="color:var(--muted);font-size:12px;margin:0 0 8px">Só preferências de fila. Acesso remoto, proxy e pastas do sistema ficam só no app principal.</p>',
        field('maxConcurrentDownloads', 'Downloads simultâneos', s.maxConcurrentDownloads || 3, 'number'),
        field('speedLimitKib', 'Limite KB/s', s.speedLimitKib || 0, 'number'),
        field('maxRetriesPerDownload', 'Máximo de tentativas', s.maxRetriesPerDownload || 3, 'number'),
        select('duplicateAction', 'Duplicatas', s.duplicateAction || 'ask', [['ask','Perguntar'],['skip','Ignorar'],['rename','Renomear'],['always_download','Baixar mesmo assim']]),
        checkbox('nativeNotification', 'Notificações nativas', !!s.nativeNotification)
      ].join('');
    }
    function field(id, label, value, type='text') { return '<label>' + label + '<input id="cfg_' + id + '" type="' + type + '" value="' + escapeAttr(value) + '"></label>'; }
    function checkbox(id, label, checked) { return '<label>' + label + '<select id="cfg_' + id + '"><option value="true" ' + (checked?'selected':'') + '>Ativo</option><option value="false" ' + (!checked?'selected':'') + '>Inativo</option></select></label>'; }
    function select(id, label, value, options) { return '<label>' + label + '<select id="cfg_' + id + '">' + options.map(([v,l]) => '<option value="' + v + '" ' + (v===value?'selected':'') + '>' + l + '</option>').join('') + '</select></label>'; }
    function escapeHtml(v) { return String(v).replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#039;'}[c])); }
    function escapeAttr(v) { return escapeHtml(v).replace(/"/g, '&quot;'); }
    async function load() {
      const data = await api('/api/state');
      state.downloads = data.downloads || [];
      state.settings = data.settings || {};
      renderDownloads();
      if (document.getElementById('settings').classList.contains('hidden')) renderSettings();
    }
    document.addEventListener('click', async (event) => {
      const target = event.target;
      if (target.matches('.tab')) {
        tabs.forEach(t => t.classList.toggle('primary', t === target));
        panels.forEach(p => p.classList.toggle('hidden', p.id !== target.dataset.tab));
        if (target.dataset.tab === 'settings') renderSettings();
      }
      if (target.dataset.action) {
        await api('/api/downloads/' + target.dataset.id + '/' + target.dataset.action, { method:'POST' }).catch(e => showToast(e.message));
        await load();
      }
    });
    document.getElementById('refresh').onclick = () => load().catch(e => showToast(e.message));
    document.getElementById('addForm').onsubmit = async (event) => {
      event.preventDefault();
      await api('/api/downloads', { method:'POST', headers:{'Content-Type':'application/json'}, body:JSON.stringify({ url: downloadUrl.value, destDir: destDir.value }) }).then(() => showToast('Adicionado')).catch(e => showToast(e.message));
      downloadUrl.value = '';
      await load();
    };
    document.getElementById('saveSettings').onclick = async () => {
      const patch = {
        maxConcurrentDownloads: Number(cfg_maxConcurrentDownloads.value),
        speedLimitKib: Number(cfg_speedLimitKib.value),
        maxRetriesPerDownload: Number(cfg_maxRetriesPerDownload.value),
        duplicateAction: cfg_duplicateAction.value,
        nativeNotification: cfg_nativeNotification.value === 'true'
      };
      await api('/api/settings', { method:'POST', headers:{'Content-Type':'application/json'}, body:JSON.stringify(patch) }).then(() => showToast('Configurações salvas')).catch(e => showToast(e.message));
      await load();
    };
    load().catch(e => showToast(e.message));
    setInterval(load, 2500);
  </script>
</body>
</html>`
}
