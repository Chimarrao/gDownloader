import {
  app,
  BrowserWindow,
  clipboard,
  dialog,
  ipcMain,
  Menu,
  nativeImage,
  Notification,
  shell,
  net,
  session,
  Tray,
} from 'electron'
import { basename, dirname, extname, join, resolve } from 'path'
import { spawn } from 'child_process'
import { chmodSync, existsSync, lstatSync, mkdirSync, readFileSync, readdirSync, rmSync, statfsSync, watch } from 'fs'
import { Socket } from 'net'
import { electronApp, optimizer, is } from '@electron-toolkit/utils'
import type { AppSettingsSnapshot } from '../shared/types'
import {
  createAppStorage,
  type HistorySearchFilters,
  type PersistedHistoryItem,
} from './app-storage'
import { createAkiraboxService } from './akirabox-service'
import { createBackendRuntime } from './backend-runtime'
import { HOSTER_BROWSER_USER_AGENT } from './browser-helper-common'
import { createCaptchaWindowService } from './captcha-window-service'
import { logMain } from './debug-log'
import { createKatfileService } from './katfile-service'
import { createRemoteAccessServer, generateRemoteAccessCredentials } from './remote-access-server'
import { createTeraboxService, type TeraboxStoredAccount } from './terabox-service'

const legacySettingsPaths = [
  join(process.cwd(), 'settings.json'),
  join(app.getPath('userData'), 'settings.json'),
]
const legacyHistoryPaths = [
  join(app.getPath('userData'), 'history.json'),
  join(app.getPath('userData'), 'download-history.json'),
]

function getAppIconPath(): string {
  if (is.dev) {
    return join(process.cwd(), 'resources', 'icons', 'gdownloader-plus.png')
  }
  return join(process.resourcesPath, 'icons', 'gdownloader-plus.png')
}

function getDatabasePath(): string {
  if (is.dev) {
    return join(process.cwd(), 'backend', 'database', 'gdownloader.db')
  }
  return join(app.getPath('userData'), 'backend', 'database', 'gdownloader.db')
}

function getBackendLogPath(): string {
  const dbPath = getDatabasePath()
  const dbDir = dirname(dbPath)
  const logDir =
    basename(dbDir).toLowerCase() === 'database'
      ? join(dirname(dbDir), 'logs')
      : join(dbDir, 'logs')

  try {
    const candidates = readdirSync(logDir)
      .filter((name) => name === 'app.log' || name.startsWith('app.log.'))
      .map((name) => join(logDir, name))
      .filter((path) => existsSync(path))
      .sort((left, right) => lstatSync(right).mtimeMs - lstatSync(left).mtimeMs)
    if (candidates.length > 0) return candidates[0]
  } catch {
    // fallback abaixo
  }

  return join(logDir, 'app.log')
}

function expandUserPath(path: string): string {
  if (!path || path === '~') return app.getPath('home')
  if (path.startsWith('~/')) return join(app.getPath('home'), path.slice(2))
  return path
}

function directorySize(path: string): number {
  if (!existsSync(path)) return 0
  const stat = lstatSync(path)
  if (!stat.isDirectory()) return stat.size
  let total = 0
  for (const entry of readdirSync(path)) {
    total += directorySize(join(path, entry))
  }
  return total
}

function clearDirectoryContents(path: string): void {
  if (!existsSync(path)) return
  for (const entry of readdirSync(path)) {
    rmSync(join(path, entry), { recursive: true, force: true })
  }
}

function tmpCachePath(): string {
  return join(process.cwd(), 'tmp')
}

function proxyCaPath(): string {
  return join(process.cwd(), 'backend', 'database', 'proxy-ca')
}

async function localCacheStats(): Promise<{
  totalBytes: number
  items: Array<{ id: string; label: string; description: string; bytes: number; entries?: number; clearable: boolean }>
}> {
  const fileInfo = await fetchBackendConfig<{ entries: number; bytes: number }>('/file-info/cache/stats')
    .catch(() => ({ entries: 0, bytes: 0 }))
  const items = [
    {
      id: 'file-info',
      label: 'Metadados de links',
      description: 'Nomes, tamanhos e árvores de pastas já lidos no LinkGrabber.',
      bytes: fileInfo.bytes,
      entries: fileInfo.entries,
      clearable: fileInfo.entries > 0,
    },
    {
      id: 'tor-data',
      label: 'Dados temporários do Tor',
      description: 'Estado local do daemon Tor embutido. Será recriado ao conectar novamente.',
      bytes: directorySize(join(app.getPath('userData'), 'tor-data')),
      clearable: true,
    },
    {
      id: 'proxy-ca',
      label: 'Certificados/cache do proxy local',
      description: 'Arquivos auxiliares do interceptor local de navegador.',
      bytes: directorySize(proxyCaPath()),
      clearable: true,
    },
    {
      id: 'tmp',
      label: 'Temporários do projeto',
      description: 'Arquivos temporários gerados por importações, testes e integrações locais.',
      bytes: directorySize(tmpCachePath()),
      clearable: true,
    },
  ]
  return {
    totalBytes: items.reduce((sum, item) => sum + item.bytes, 0),
    items,
  }
}

async function clearLocalCache(ids: string[]): Promise<Awaited<ReturnType<typeof localCacheStats>>> {
  for (const id of ids) {
    if (id === 'file-info') {
      await deleteBackend('/file-info/cache').catch((error) => {
        logMain('cache', 'Falha ao limpar cache de metadados', error)
      })
    } else if (id === 'tor-data') {
      clearDirectoryContents(join(app.getPath('userData'), 'tor-data'))
    } else if (id === 'proxy-ca') {
      clearDirectoryContents(proxyCaPath())
    } else if (id === 'tmp') {
      clearDirectoryContents(tmpCachePath())
    }
  }
  return localCacheStats()
}

function tailLogFile(maxLines = 500): { path: string; lines: string[] } {
  const path = getBackendLogPath()
  if (!existsSync(path)) return { path, lines: [] }
  const raw = readFileSync(path, 'utf8')
  const lines = raw.split(/\r?\n/).filter(Boolean)
  return { path, lines: lines.slice(-maxLines) }
}

async function fetchBackendConfig<T>(path: string, init?: RequestInit): Promise<T> {
  if (!rustPort) {
    throw new Error('Backend Rust ainda não está disponível')
  }

  const response = await fetch(`http://127.0.0.1:${rustPort}${path}`, init)
  if (!response.ok) {
    throw new Error(`Falha ao acessar ${path}: ${response.status}`)
  }
  return response.json() as Promise<T>
}

async function postBackend(path: string, payload?: unknown): Promise<void> {
  if (!rustPort) {
    throw new Error('Backend Rust ainda não está disponível')
  }

  const response = await fetch(`http://127.0.0.1:${rustPort}${path}`, {
    method: 'POST',
    headers: payload === undefined ? undefined : { 'Content-Type': 'application/json' },
    body: payload === undefined ? undefined : JSON.stringify(payload),
  })

  if (!response.ok) {
    const body = await response
      .json()
      .catch(() => ({ error: `Falha ao acessar ${path}: ${response.status}` }))
    throw new Error(body.error ?? `Falha ao acessar ${path}: ${response.status}`)
  }
}

async function deleteBackend(path: string): Promise<void> {
  if (!rustPort) {
    throw new Error('Backend Rust ainda não está disponível')
  }

  const response = await fetch(`http://127.0.0.1:${rustPort}${path}`, {
    method: 'DELETE',
  })
  if (!response.ok) {
    const body = await response
      .json()
      .catch(() => ({ error: `Falha ao acessar ${path}: ${response.status}` }))
    throw new Error(body.error ?? `Falha ao acessar ${path}: ${response.status}`)
  }
}

const storage = createAppStorage({
  legacySettingsPaths,
  legacyHistoryPaths,
  fetchBackendConfig,
  postBackend,
  deleteBackend,
})

const remoteAccessServer = createRemoteAccessServer({
  getRustPort: () => rustPort,
  getSettings: () => storage.getPublicSettings(),
  persistSettings: async (settings) => {
    await storage.persistPublicSettings(settings)
    await syncBackendConfig(settings.maxConcurrentDownloads)
    configureClipboardMonitor(settings.clipboardMonitorEnabled)
    await remoteAccessServer.configure(settings)
  },
})

async function loadSecureSettings(): Promise<void> {
  if (!rustPort) {
    return
  }

  try {
    await storage.loadSecureSettings()
  } catch (error) {
    logMain('settings', 'Falha ao carregar credenciais locais do SQLite', error)
  }
}

async function loadPublicSettings(): Promise<void> {
  if (!rustPort) {
    return
  }

  try {
    await storage.loadPublicSettings()
  } catch (error) {
    logMain('settings', 'Falha ao carregar as configurações locais do SQLite', error)
  }
}

async function loadHistoryFromBackend(
  filters?: HistorySearchFilters,
): Promise<PersistedHistoryItem[]> {
  if (!rustPort) {
    return []
  }

  return storage.loadHistoryFromBackend(filters).catch(() => [])
}

async function saveHistoryToBackend(items: PersistedHistoryItem[]): Promise<void> {
  await storage.saveHistoryToBackend(items)
}

async function appendHistoryItemToBackend(item: PersistedHistoryItem): Promise<void> {
  await storage.appendHistoryItemToBackend(item)
}

async function loadHistoryHostsFromBackend(): Promise<string[]> {
  if (!rustPort) {
    return []
  }
  return storage.loadHistoryHostsFromBackend().catch(() => [])
}

async function removeHistoryItemInBackend(id: string): Promise<void> {
  await storage.removeHistoryItemInBackend(id)
}

async function clearHistoryInBackend(): Promise<void> {
  await storage.clearHistoryInBackend()
}

async function migrateLegacySettings(): Promise<void> {
  if (!rustPort) {
    return
  }

  await storage.migrateLegacySettingsIfNeeded().catch((error) => {
    logMain('settings', 'Falha ao migrar dados legados para o SQLite', error)
  })
}

function currentSettingsSnapshot(): AppSettingsSnapshot {
  return storage.currentSettingsSnapshot()
}

function persistTeraboxAccount(account: TeraboxStoredAccount | null): void {
  void storage.persistTeraboxAccount(account).catch((error) => {
    logMain('auth', 'Falha ao persistir conta do TeraBox no SQLite', error)
  })
}

async function solveCaptchaWithNopecha(params: {
  type: string
  sitekey: string
  pageurl: string
}): Promise<string | null> {
  const apiKey = storage.getNopechaApiKey()
  if (!apiKey) {
    logMain('nopecha', 'Nenhuma chave configurada, pulando tentativa automática', {
      type: params.type,
      pageurl: params.pageurl,
    })
    return null
  }

  logMain('nopecha', 'Iniciando tentativa automática de captcha', {
    type: params.type,
    pageurl: params.pageurl,
    hasSitekey: Boolean(params.sitekey),
  })

  try {
    const submitRes = (await fetch('https://api.nopecha.com/', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        type: params.type,
        sitekey: params.sitekey,
        url: params.pageurl,
        key: apiKey,
      }),
    })
      .then((r) => r.json())
      .catch(() => null)) as Record<string, unknown> | null

    if (!submitRes?.data) {
      logMain('nopecha', 'API não retornou task id', submitRes)
      return null
    }
    const taskId = submitRes.data as string

    for (let i = 0; i < 60; i++) {
      await new Promise((r) => setTimeout(r, 2000))
      const res = (await fetch(`https://api.nopecha.com/?id=${taskId}&key=${apiKey}`)
        .then((r) => r.json())
        .catch(() => null)) as Record<string, unknown> | null
      const data = res?.data
      if (Array.isArray(data) && data[0]) {
        logMain('nopecha', 'Captcha resolvido automaticamente', {
          type: params.type,
          pageurl: params.pageurl,
        })
        return data[0] as string
      }
    }

    logMain('nopecha', 'Tempo esgotado aguardando resposta da API', {
      type: params.type,
      pageurl: params.pageurl,
    })
    return null
  } catch (error) {
    logMain('nopecha', 'Falha ao tentar resolver captcha automaticamente', error)
    return null
  }
}

let rustPort: number | null = null
let clipboardMonitorTimer: ReturnType<typeof setInterval> | null = null
let lastClipboardText = ''
let lastClipboardSignature = ''
let managedTorProcess: ReturnType<typeof spawn> | null = null
let managedTorBootstrap = 0
let lastTorExit: { ip: string; country?: string; countryCode?: string; isTor: boolean } | null = null
const backendRuntime = createBackendRuntime({
  dbPath: getDatabasePath(),
  createEnv: (dbPath) => ({
    ...process.env,
    TERABOX_PROXY_PORT: String(teraboxProxyPort),
    AKIRABOX_PROXY_PORT: String(teraboxProxyPort),
    KATFILE_PROXY_PORT: String(teraboxProxyPort),
    GDOWNLOADER_DB_PATH: dbPath,
  }),
  onStdErr: (message) => {
    logMain('rust', 'stderr', message)
  },
  onRestarted: async (port) => {
    rustPort = port
    logMain('rust', 'Backend reiniciado', { port })
    await loadPublicSettings()
    await loadSecureSettings()
    await syncBackendConfig(storage.getPublicSettings().maxConcurrentDownloads)
    configureClipboardMonitor(storage.getPublicSettings().clipboardMonitorEnabled)
    await remoteAccessServer.configure(storage.getPublicSettings())
  },
})

const teraboxService = createTeraboxService({
  readAccount: () => storage.getTeraboxAccount(),
  saveAccount: persistTeraboxAccount,
})
const akiraboxService = createAkiraboxService({
  solveCaptcha: solveCaptchaWithNopecha,
})
const captchaWindowService = createCaptchaWindowService()
const katfileService = createKatfileService()

function runCommand(command: string, args: string[]): Promise<void> {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { stdio: ['ignore', 'pipe', 'pipe'] })
    let stderr = ''

    child.stderr?.on('data', (data: Buffer) => {
      stderr += data.toString()
    })

    child.on('error', reject)
    child.on('exit', (code) => {
      if (code === 0) {
        resolve()
        return
      }
      reject(new Error(stderr.trim() || `${command} encerrou com código ${code}`))
    })
  })
}

async function findFirstCommand(candidates: string[]): Promise<string | null> {
  const lookup = process.platform === 'win32' ? 'where' : 'which'
  for (const candidate of candidates) {
    try {
      await runCommand(lookup, [candidate])
      return candidate
    } catch {
      // segue
    }
  }
  return null
}

function getArchiveOutputDir(archivePath: string): string {
  const base = basename(archivePath)
  const dir = dirname(archivePath)
  const lower = base.toLowerCase()
  const suffixes = [
    '.tar.gz',
    '.tar.bz2',
    '.tar.xz',
    '.tar.zst',
    '.tgz',
    '.tbz2',
    '.txz',
    '.zip',
    '.rar',
    '.7z',
    '.tar',
    '.gz',
    '.bz2',
    '.xz',
    '.zst',
  ]
  const matched = suffixes.find((suffix) => lower.endsWith(suffix))
  const name = matched
    ? base.slice(0, base.length - matched.length)
    : base.slice(0, base.length - extname(base).length)
  return join(dir, name || `${base}-extraido`)
}

function toArrayBuffer(buffer: Buffer): ArrayBuffer {
  const view = new Uint8Array(buffer)
  const copy = new Uint8Array(view.byteLength)
  copy.set(view)
  return copy.buffer
}

async function extractRarEmbedded(archivePath: string, outputDir: string): Promise<string> {
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  const unrar = require('node-unrar-js') as {
    createExtractorFromFile: (options: {
      filepath: string
      targetPath: string
      wasmBinary: ArrayBuffer
    }) => Promise<{
      extract: (options?: Record<string, never>) => {
        files: Iterable<unknown>
      }
    }>
  }

  const wasmBinary = toArrayBuffer(
    readFileSync(require.resolve('node-unrar-js/dist/js/unrar.wasm')),
  )
  const extractor = await unrar.createExtractorFromFile({
    filepath: archivePath,
    targetPath: outputDir,
    wasmBinary,
  })
  const extracted = extractor.extract()
  for (const entry of extracted.files) {
    void entry
    // percorre até o fim para garantir liberação dos recursos internos
  }
  return outputDir
}

async function extractWith7zWasm(archivePath: string, outputDir: string): Promise<string> {
  const { default: SevenZip } = await import('7z-wasm')
  const wasmBinary = toArrayBuffer(readFileSync(require.resolve('7z-wasm/7zz.wasm')))
  const logs: string[] = []
  const errors: string[] = []
  const sevenZip = await SevenZip({
    wasmBinary,
    print: (line: string) => logs.push(line),
    printErr: (line: string) => errors.push(line),
  })

  const mountRoot = '/nodefs'
  const realRoot = dirname(archivePath)
  const archiveName = basename(archivePath)
  const outputName = basename(outputDir)

  try {
    sevenZip.FS.mkdir(mountRoot)
  } catch {
    // já existe
  }

  sevenZip.FS.mount(sevenZip.NODEFS, { root: realRoot }, mountRoot)
  sevenZip.FS.chdir(mountRoot)

  try {
    sevenZip.callMain(['x', archiveName, `-o${outputName}`, '-y'])
  } catch (error) {
    const detail = [...errors, ...logs].filter(Boolean).join('\n').trim()
    throw new Error(detail || (error instanceof Error ? error.message : String(error)))
  } finally {
    try {
      sevenZip.FS.chdir('/')
      sevenZip.FS.unmount(mountRoot)
    } catch {
      // ignora desmontagem
    }
  }

  return outputDir
}

async function extractArchive(archivePath: string): Promise<string> {
  if (!existsSync(archivePath)) {
    throw new Error('Arquivo não encontrado para extração')
  }

  const outputDir = getArchiveOutputDir(archivePath)
  mkdirSync(outputDir, { recursive: true })
  const lower = archivePath.toLowerCase()

  if (lower.endsWith('.zip')) {
    if (process.platform === 'win32') {
      await runCommand('powershell', [
        '-NoProfile',
        '-Command',
        `Expand-Archive -LiteralPath '${archivePath.replace(/'/g, "''")}' -DestinationPath '${outputDir.replace(/'/g, "''")}' -Force`,
      ])
      return outputDir
    }
    await runCommand('unzip', ['-o', archivePath, '-d', outputDir])
    return outputDir
  }

  if (
    lower.endsWith('.tar') ||
    lower.endsWith('.tar.gz') ||
    lower.endsWith('.tgz') ||
    lower.endsWith('.tar.bz2') ||
    lower.endsWith('.tbz2') ||
    lower.endsWith('.tar.xz') ||
    lower.endsWith('.txz') ||
    lower.endsWith('.tar.zst')
  ) {
    await runCommand('tar', ['-xf', archivePath, '-C', outputDir])
    return outputDir
  }

  if (lower.endsWith('.rar') || lower.endsWith('.7z')) {
    if (lower.endsWith('.rar')) {
      try {
        return await extractRarEmbedded(archivePath, outputDir)
      } catch (error) {
        logMain('extract', 'Falha no extrator embutido de RAR, tentando fallback', error)
      }
    }

    try {
      return await extractWith7zWasm(archivePath, outputDir)
    } catch (error) {
      logMain('extract', 'Falha no extrator embutido de 7z/RAR, tentando fallback', error)
    }

    const tool = await findFirstCommand(['7z', '7za', 'unar'])
    if (!tool) {
      throw new Error(
        'Não foi possível extrair este arquivo com os extratores embutidos e nenhuma ferramenta externa foi encontrada',
      )
    }

    if (tool === 'unar') {
      await runCommand(tool, ['-f', '-o', outputDir, archivePath])
      return outputDir
    }

    await runCommand(tool, ['x', '-y', `-o${outputDir}`, archivePath])
    return outputDir
  }

  throw new Error('Formato de arquivo não suportado para extração')
}

/**
 * Faz uma requisição HTTP usando a sessão persist:terabox do Electron.
 * Isso garante que todos os cookies e fingerprint do browser sejam usados,
 * contornando a proteção anti-scraping do Terabox na API share/list.
 */
async function teraboxNetRequest(params: {
  url: string
  method?: string
  headers?: Record<string, string>
  body?: string
}): Promise<unknown> {
  const tbSession = session.fromPartition('persist:terabox')
  return new Promise<unknown>((resolve, reject) => {
    const request = net.request({
      url: params.url,
      method: params.method ?? 'GET',
      session: tbSession,
    })
    request.setHeader('User-Agent', HOSTER_BROWSER_USER_AGENT)
    request.setHeader('Accept', 'application/json, */*')
    request.setHeader('Accept-Language', 'pt-BR,pt;q=0.9,en-US;q=0.8')
    if (params.headers) {
      for (const [k, v] of Object.entries(params.headers)) request.setHeader(k, v)
    }
    const chunks: Buffer[] = []
    request.on('response', (response) => {
      response.on('data', (chunk) => chunks.push(chunk as Buffer))
      response.on('end', () => {
        try {
          resolve(JSON.parse(Buffer.concat(chunks).toString('utf8')))
        } catch {
          resolve({ _raw: Buffer.concat(chunks).toString('utf8') })
        }
      })
      response.on('error', reject)
    })
    request.on('error', reject)
    if (params.body) request.write(params.body)
    request.end()
  })
}

/** Local HTTP proxy para o backend Rust chamar requisições Terabox via sessão Electron */
let teraboxProxyPort = 0

async function syncBackendConfig(maxConcurrentDownloads: number): Promise<void> {
  await postBackend('/config/downloads', {
    max_concurrent_downloads: Math.max(1, Number(maxConcurrentDownloads) || 1),
  }).catch((error) => {
    logMain('config', 'Falha ao sincronizar configuração do backend', error)
  })
}

function probeTcpPort(host: string, port: number, timeoutMs = 900): Promise<boolean> {
  return new Promise((resolve) => {
    const socket = new Socket()
    let settled = false
    const done = (ok: boolean): void => {
      if (settled) return
      settled = true
      socket.destroy()
      resolve(ok)
    }
    socket.setTimeout(timeoutMs)
    socket.once('connect', () => done(true))
    socket.once('timeout', () => done(false))
    socket.once('error', () => done(false))
    socket.connect(port, host)
  })
}

async function detectTorEndpoint(): Promise<{ host: string; port: number } | null> {
  for (const port of [9150, 9050]) {
    if (await probeTcpPort('127.0.0.1', port)) {
      return { host: '127.0.0.1', port }
    }
  }
  return null
}

function findTorBinary(): string | null {
  const platformCandidates =
    process.platform === 'win32'
      ? [
          'C:\\Program Files\\Tor\\tor.exe',
          'C:\\Program Files (x86)\\Tor\\tor.exe',
          join(app.getPath('home'), 'Desktop', 'Tor Browser', 'Browser', 'TorBrowser', 'Tor', 'tor.exe'),
          join(app.getPath('home'), 'Downloads', 'Tor Browser', 'Browser', 'TorBrowser', 'Tor', 'tor.exe'),
        ]
      : process.platform === 'darwin'
        ? [
            '/opt/homebrew/bin/tor',
            '/usr/local/bin/tor',
            '/usr/bin/tor',
            '/Applications/Tor Browser.app/Contents/MacOS/Tor/tor.real',
            '/Applications/Tor Browser.app/Contents/MacOS/tor.real',
          ]
        : [
            '/usr/bin/tor',
            '/usr/local/bin/tor',
            '/snap/bin/tor',
          ]
  const candidates = [
    process.env.GDOWNLOADER_TOR_PATH,
    bundledTorBinary(),
    ...platformCandidates,
  ].filter(Boolean) as string[]
  return candidates.find((candidate) => existsSync(candidate)) ?? null
}

function bundledTorDir(): string {
  const platform = process.platform === 'darwin'
    ? process.arch === 'arm64' ? 'darwin-arm64' : 'darwin-x64'
    : process.platform === 'win32'
      ? process.arch === 'ia32' ? 'win32-ia32' : 'win32-x64'
      : process.arch === 'ia32'
        ? 'linux-ia32'
        : process.arch === 'arm64'
          ? 'linux-arm64'
          : 'linux-x64'
  if (is.dev) {
    return join(process.cwd(), 'resources', 'tor', platform)
  }
  return join(process.resourcesPath, 'tor', platform)
}

function bundledTorBinary(): string {
  return join(bundledTorDir(), 'tor', process.platform === 'win32' ? 'tor.exe' : 'tor')
}

function torResourcePath(...parts: string[]): string {
  return join(bundledTorDir(), ...parts)
}

function torDataDirectory(): string {
  const path = join(app.getPath('userData'), 'tor-data')
  mkdirSync(path, { recursive: true })
  return path
}

function torArgs(): string[] {
  const args = [
    '--SocksPort',
    '127.0.0.1:9150 IsolateSOCKSAuth',
    '--ControlPort',
    '127.0.0.1:9151',
    '--DataDirectory',
    torDataDirectory(),
    '--AvoidDiskWrites',
    '1',
    '--CookieAuthentication',
    '1',
  ]
  const geoIp = torResourcePath('data', 'geoip')
  const geoIp6 = torResourcePath('data', 'geoip6')
  const defaults = torResourcePath('data', 'torrc-defaults')
  if (existsSync(geoIp)) args.push('--GeoIPFile', geoIp)
  if (existsSync(geoIp6)) args.push('--GeoIPv6File', geoIp6)
  if (existsSync(defaults)) args.push('--defaults-torrc', defaults)
  return args
}

async function tryStartTor(): Promise<void> {
  if (managedTorProcess && !managedTorProcess.killed) return
  const torBinary = findTorBinary()
  if (torBinary) {
    if (process.platform !== 'win32') {
      try {
        chmodSync(torBinary, 0o755)
      } catch {
        // System Tor paths may not be writable; spawn will report a clear error if execution fails.
      }
    }
    managedTorBootstrap = 0
    managedTorProcess = spawn(torBinary, torArgs(), {
      stdio: ['ignore', 'ignore', 'pipe'],
      cwd: dirname(torBinary),
      env: {
        ...process.env,
        ...(process.platform === 'darwin' ? { DYLD_LIBRARY_PATH: dirname(torBinary) } : {}),
        ...(process.platform === 'linux' ? { LD_LIBRARY_PATH: dirname(torBinary) } : {}),
        ...(process.platform === 'win32' ? { PATH: `${dirname(torBinary)};${process.env.PATH ?? ''}` } : {}),
      },
    })
    managedTorProcess.stderr?.on('data', (data: Buffer) => {
      const message = data.toString()
      const match = message.match(/Bootstrapped\s+(\d+)%/i)
      if (match) {
        managedTorBootstrap = Math.max(managedTorBootstrap, Number(match[1]) || 0)
      }
      logMain('tor', 'stderr', message)
    })
    managedTorProcess.once('exit', () => {
      managedTorProcess = null
      managedTorBootstrap = 0
    })
    return
  }

  const torBrowserApp = '/Applications/Tor Browser.app'
  if (existsSync(torBrowserApp)) {
    await shell.openPath(torBrowserApp).catch((error) => {
      logMain('tor', 'Falha ao abrir Tor Browser', error)
    })
    return
  }

  throw new Error(
    `Tor não foi encontrado para ${process.platform}/${process.arch}. Inclua o binário em ${bundledTorBinary()} ou configure GDOWNLOADER_TOR_PATH com o caminho do executável.`,
  )
}

async function waitForManagedTorBootstrap(timeoutMs = 32_000): Promise<boolean> {
  const deadline = Date.now() + timeoutMs
  while (Date.now() < deadline) {
    if (managedTorBootstrap >= 100) return true
    if (!managedTorProcess || managedTorProcess.killed) return false
    await new Promise((resolve) => setTimeout(resolve, 900))
  }
  return managedTorBootstrap >= 100
}

async function waitForTorEndpoint(timeoutMs = 18_000): Promise<{ host: string; port: number } | null> {
  const deadline = Date.now() + timeoutMs
  while (Date.now() < deadline) {
    const endpoint = await detectTorEndpoint()
    if (endpoint) return endpoint
    await new Promise((resolve) => setTimeout(resolve, 700))
  }
  return null
}

async function persistTorProxy(enabled: boolean, endpoint?: { host: string; port: number }): Promise<void> {
  const current = storage.getPublicSettings()
  const next = {
    ...current,
    proxyMode: enabled ? 'tor' : 'none',
    proxyHost: enabled ? endpoint?.host ?? '127.0.0.1' : '',
    proxyPort: enabled ? endpoint?.port ?? 9150 : 0,
    startTor: enabled,
  }
  await storage.persistPublicSettings(next)
  if (rustPort) {
    await postBackend('/config/public', next).catch((error) => {
      logMain('tor', 'Falha ao aplicar proxy Tor no backend', error)
    })
  }
}

async function clearStaleTorProxy(): Promise<void> {
  const settings = storage.getPublicSettings()
  if (settings.proxyMode !== 'tor') return
  if (!settings.startTor) {
    await persistTorProxy(false)
    return
  }
  const endpoint = settings.proxyHost && settings.proxyPort
    ? { host: settings.proxyHost, port: settings.proxyPort }
    : null
  if (endpoint && await probeTcpPort(endpoint.host, endpoint.port)) return
  await persistTorProxy(false)
}

async function torStatusPayload(): Promise<{
  state: 'disconnected' | 'connected'
  host: string
  port: number
  route: Array<{ role: string; country: string; code: string }>
  ip?: string
  country?: string
  countryCode?: string
  isTor?: boolean
}> {
  const settings = storage.getPublicSettings()
  if (settings.proxyMode !== 'tor' || !settings.startTor) {
    return {
      state: 'disconnected',
      host: '127.0.0.1',
      port: 9150,
      route: [],
    }
  }
  const endpoint = settings.proxyHost && settings.proxyPort
    ? { host: settings.proxyHost, port: settings.proxyPort }
    : await detectTorEndpoint()
  const connected = Boolean(endpoint && await probeTcpPort(endpoint.host, endpoint.port))
  const exit = connected ? lastTorExit : null
  return {
    state: connected ? 'connected' : 'disconnected',
    host: endpoint?.host ?? '127.0.0.1',
    port: endpoint?.port ?? 9150,
    route: connected && exit
      ? [
          { role: 'Exit', country: exit.country || exit.ip, code: exit.countryCode || exit.ip },
        ]
      : [],
    ip: exit?.ip,
    country: exit?.country,
    countryCode: exit?.countryCode,
    isTor: exit?.isTor,
  }
}

async function countryForIp(ip: string): Promise<{ country?: string; countryCode?: string }> {
  if (!ip || ip === 'unknown') return {}
  try {
    const response = await fetch(`https://ipapi.co/${encodeURIComponent(ip)}/json/`)
    if (!response.ok) return {}
    const json = await response.json() as Record<string, unknown>
    return {
      country: typeof json.country_name === 'string' ? json.country_name : undefined,
      countryCode: typeof json.country_code === 'string' ? json.country_code : undefined,
    }
  } catch {
    return {}
  }
}

async function testTorConnection(): Promise<{ ip: string; country?: string; countryCode?: string; isTor: boolean }> {
  const settings = storage.getPublicSettings()
  if (settings.proxyMode !== 'tor' || !settings.startTor) {
    throw new Error('Tor não está ativo no gDownloader.')
  }
  if (!rustPort) {
    throw new Error('Backend Rust ainda não está disponível.')
  }
  const result = await fetchBackendConfig<{ ip: string; isTor?: boolean }>('/config/test-proxy')
  const country = await countryForIp(result.ip)
  lastTorExit = {
    ip: result.ip,
    isTor: Boolean(result.isTor),
    ...country,
  }
  if (!lastTorExit.isTor) {
    throw new Error(`A conexão saiu pelo IP ${result.ip}, mas o Tor Project não reconheceu como Tor.`)
  }
  return lastTorExit
}

function controlPortForEndpoint(endpoint: { port: number }): number {
  if (endpoint.port === 9150) return 9151
  if (endpoint.port === 9050) return 9051
  return endpoint.port + 1
}

function torControlCookieHex(): string {
  const cookiePath = join(torDataDirectory(), 'control_auth_cookie')
  if (!existsSync(cookiePath)) {
    throw new Error('Cookie de controle do Tor não encontrado. Conecte usando o Tor embutido do gDownloader.')
  }
  return readFileSync(cookiePath).toString('hex')
}

function sendTorControlCommands(port: number, commands: string[]): Promise<string> {
  return new Promise((resolve, reject) => {
    const socket = new Socket()
    let output = ''
    const timer = setTimeout(() => {
      socket.destroy()
      reject(new Error('Tempo esgotado falando com o ControlPort do Tor.'))
    }, 5000)
    socket.once('error', (error) => {
      clearTimeout(timer)
      reject(error)
    })
    socket.on('data', (chunk: Buffer) => {
      output += chunk.toString('utf8')
      if (output.toLowerCase().includes('250 closing connection')) {
        clearTimeout(timer)
        socket.end()
        resolve(output)
      }
    })
    socket.connect(port, '127.0.0.1', () => {
      socket.write(commands.join('\r\n') + '\r\n')
    })
  })
}

async function signalTorNewIdentity(): Promise<void> {
  const settings = storage.getPublicSettings()
  const endpoint = settings.proxyHost && settings.proxyPort
    ? { host: settings.proxyHost, port: settings.proxyPort }
    : await detectTorEndpoint()
  if (!endpoint) {
    throw new Error('Tor não está ativo.')
  }
  const cookie = torControlCookieHex()
  const response = await sendTorControlCommands(controlPortForEndpoint(endpoint), [
    `AUTHENTICATE ${cookie}`,
    'SIGNAL NEWNYM',
    'QUIT',
  ])
  if (!response.includes('250 OK')) {
    throw new Error(`Tor recusou nova identidade: ${response.trim()}`)
  }
  lastTorExit = null
  await new Promise((resolve) => setTimeout(resolve, 3500))
}

async function pauseActiveDownloadsForNetworkSwitch(): Promise<string[]> {
  if (!rustPort) return []
  const downloads = await fetchBackendConfig<Array<{ id: string; status: string }>>('/downloads').catch(() => [])
  const activeIds = downloads
    .filter((download) => download.status === 'downloading' || download.status === 'verifying')
    .map((download) => download.id)
  for (const id of activeIds) {
    await postBackend(`/downloads/${encodeURIComponent(id)}/pause`).catch((error) => {
      logMain('tor', 'Falha ao pausar download antes de trocar rede', { id, error })
    })
  }
  return activeIds
}

async function resumeDownloadsAfterNetworkSwitch(ids: string[]): Promise<void> {
  for (const id of ids) {
    await postBackend(`/downloads/${encodeURIComponent(id)}/resume`).catch((error) => {
      logMain('tor', 'Falha ao retomar download após trocar rede', { id, error })
    })
  }
}

function extractClipboardUrls(text: string): string[] {
  const matches = text.match(/https?:\/\/[^\s"'<>\\]+/gi) ?? []
  const seen = new Set<string>()
  return matches
    .map((url) => url.replace(/[),.;\]]+$/g, ''))
    .filter((url) => {
      if (seen.has(url)) return false
      seen.add(url)
      return true
    })
}

async function detectClipboardUrl(url: string): Promise<{ id?: string; name?: string } | null> {
  if (!rustPort) return null
  try {
    const response = await fetch(
      `http://127.0.0.1:${rustPort}/detect?url=${encodeURIComponent(url)}`,
    )
    if (!response.ok) return null
    return response.json() as Promise<{ id?: string; name?: string } | null>
  } catch (error) {
    logMain('clipboard', 'Falha ao consultar provider para clipboard', {
      url,
      error,
    })
    return null
  }
}

async function inspectClipboardForLinks(): Promise<void> {
  const text = clipboard.readText().trim()
  if (!text || text === lastClipboardText) {
    return
  }
  lastClipboardText = text

  const detected: Array<{ url: string; provider: { id?: string; name?: string } }> = []
  for (const url of extractClipboardUrls(text)) {
    const provider = await detectClipboardUrl(url)
    if (!provider?.id) {
      continue
    }
    detected.push({ url, provider })
  }

  if (detected.length > 0) {
    const urls = detected.map((item) => item.url)
    const signature = urls.join('\n')
    if (signature === lastClipboardSignature) {
      return
    }
    const first = detected[0]

    lastClipboardSignature = signature
    logMain('clipboard', 'Link suportado detectado na área de transferência', {
      provider: first.provider.id,
      url: first.url,
      count: urls.length,
    })
    for (const window of BrowserWindow.getAllWindows()) {
      window.webContents.send('clipboard:link-detected', {
        url: first.url,
        urls,
        provider: first.provider.id,
        providerName: first.provider.name ?? first.provider.id,
      })
    }
  }
}

function configureClipboardMonitor(enabled: boolean): void {
  if (!enabled) {
    if (clipboardMonitorTimer) {
      clearInterval(clipboardMonitorTimer)
      clipboardMonitorTimer = null
    }
    return
  }

  if (clipboardMonitorTimer) {
    return
  }

  lastClipboardText = ''
  lastClipboardSignature = ''
  clipboardMonitorTimer = setInterval(() => {
    void inspectClipboardForLinks()
  }, 350)
  void inspectClipboardForLinks()
  logMain('clipboard', 'Monitor de clipboard ativado')
}

let tray: Tray | null = null
let mainWindow: BrowserWindow | null = null
const logWatchers = new Map<number, ReturnType<typeof watch>>()

function createTray(win: BrowserWindow): void {
  const iconPath = getAppIconPath()
  let icon: Electron.NativeImage
  try {
    icon = nativeImage.createFromPath(iconPath)
    if (icon.isEmpty()) {
      icon = nativeImage.createEmpty()
    }
  } catch {
    icon = nativeImage.createEmpty()
  }

  if (!icon.isEmpty()) {
    icon = icon.resize({ width: 16, height: 16 })
  }

  tray = new Tray(icon)
  tray.setToolTip('gDownloader')

  updateTrayMenu(win, tray, 0, '0 B/s')

  tray.on('click', () => {
    if (win.isVisible()) {
      win.focus()
    } else {
      win.show()
    }
  })
}

function updateTrayMenu(
  win: BrowserWindow,
  trayInstance: Tray,
  activeCount: number,
  speed: string,
): void {
  const contextMenu = Menu.buildFromTemplate([
    {
      label: `gDownloader — ${activeCount} baixando · ${speed}`,
      enabled: false,
    },
    { type: 'separator' },
    {
      label: 'Mostrar app',
      click: () => {
        win.show()
        win.focus()
      },
    },
    {
      label: 'Pausar tudo',
      click: () => {
        win.webContents.send('tray:pause-all')
      },
    },
    {
      label: 'Retomar tudo',
      click: () => {
        win.webContents.send('tray:resume-all')
      },
    },
    { type: 'separator' },
    {
      label: 'Limite de velocidade',
      submenu: [
        {
          label: 'Sem limite',
          click: () => win.webContents.send('tray:set-speed-limit', 0),
        },
        {
          label: '500 KB/s',
          click: () => win.webContents.send('tray:set-speed-limit', 500),
        },
        {
          label: '200 KB/s',
          click: () => win.webContents.send('tray:set-speed-limit', 200),
        },
        {
          label: '50 KB/s',
          click: () => win.webContents.send('tray:set-speed-limit', 50),
        },
      ],
    },
    { type: 'separator' },
    {
      label: 'Sair',
      click: () => {
        app.quit()
      },
    },
  ])
  trayInstance.setContextMenu(contextMenu)
}

function createWindow(): BrowserWindow {
  const win = new BrowserWindow({
    title: 'gDownloader',
    width: 1200,
    height: 750,
    minWidth: 900,
    minHeight: 600,
    show: false,
    autoHideMenuBar: true,
    icon: nativeImage.createFromPath(getAppIconPath()),
    webPreferences: {
      preload: join(__dirname, '../preload/index.js'),
      sandbox: false,
    },
  })

  win.on('ready-to-show', () => win.show())

  win.on('close', (event) => {
    if (tray) {
      event.preventDefault()
      win.hide()
    }
  })

  // Abre links externos no browser padrão do sistema
  win.webContents.setWindowOpenHandler(({ url }) => {
    shell.openExternal(url)
    return { action: 'deny' }
  })

  if (is.dev && process.env['ELECTRON_RENDERER_URL']) {
    win.loadURL(process.env['ELECTRON_RENDERER_URL'])
  } else {
    win.loadFile(join(__dirname, '../renderer/index.html'))
  }

  return win
}

app.whenReady().then(async () => {
  app.setName('gDownloader')
  electronApp.setAppUserModelId('com.gdownloader')

  app.on('browser-window-created', (_, window) => {
    optimizer.watchWindowShortcuts(window)
  })

  // IPC: porta do backend Rust
  ipcMain.handle('backend:getPort', () => rustPort)
  ipcMain.handle('terabox:getProxyPort', () => teraboxProxyPort)

  // IPC: shell
  ipcMain.handle('shell:openPath', (_e, p: string) => shell.openPath(p))
  ipcMain.handle('shell:showInFolder', (_e, p: string) => {
    try {
      if (existsSync(p) && lstatSync(p).isDirectory()) {
        return shell.openPath(p)
      }
    } catch {
      // fallback abaixo
    }
    shell.showItemInFolder(p)
    return ''
  })
  ipcMain.handle('clipboard:writeText', (_e, text: string) => {
    clipboard.writeText(text)
    return true
  })
  ipcMain.handle('logs:tail', (_e, maxLines?: number) => {
    return tailLogFile(Math.max(50, Math.min(Number(maxLines ?? 500), 2000)))
  })
  ipcMain.on('logs:watch-start', (event) => {
    const senderId = event.sender.id
    logWatchers.get(senderId)?.close()
    const logPath = getBackendLogPath()
    try {
      mkdirSync(dirname(logPath), { recursive: true })
      const watcher = watch(dirname(logPath), { persistent: false }, () => {
        event.sender.send('logs:update', tailLogFile(500))
      })
      logWatchers.set(senderId, watcher)
      event.sender.once('destroyed', () => {
        logWatchers.get(senderId)?.close()
        logWatchers.delete(senderId)
      })
    } catch {
      // polling pelo renderer ainda pode chamar logs:tail.
    }
  })
  ipcMain.on('logs:watch-stop', (event) => {
    const senderId = event.sender.id
    logWatchers.get(senderId)?.close()
    logWatchers.delete(senderId)
  })
  ipcMain.handle('system:notify', (_e, title: string, body?: string) => {
    if (!Notification.isSupported()) return false
    new Notification({ title, body }).show()
    return true
  })
  ipcMain.handle('archive:extract', async (_e, archivePath: string) => {
    return extractArchive(archivePath)
  })
  ipcMain.handle('archive:auto-extract', async (_e, archivePath: string, passwords: string[]) => {
    const { autoExtract, shouldAutoExtractFile, allPartsReady } = await import('./archive-service')
    if (!shouldAutoExtractFile(archivePath)) {
      return { success: false, error: 'not_extractable' }
    }
    // For multipart, check all parts are present first
    if (!allPartsReady(archivePath)) {
      return { success: false, error: 'parts_missing' }
    }
    const learned = await fetchBackendConfig<Array<{ password: string }>>(
      '/archive-passwords',
    ).catch(() => [])
    const mergedPasswords = [
      ...new Set([
        ...learned
          .slice(0, 20)
          .map((item) => item.password)
          .filter(Boolean),
        ...(Array.isArray(passwords) ? passwords : []),
      ]),
    ]
    const result = await autoExtract(archivePath, mergedPasswords)
    if (result.success && result.passwordUsed) {
      await postBackend('/archive-passwords/success', {
        password: result.passwordUsed,
        source: 'auto',
      }).catch((error) => {
        logMain('archive-passwords', 'Falha ao registrar senha de archive', error)
      })
    }
    return result
  })
  ipcMain.handle('archive-passwords:list', async () => {
    return fetchBackendConfig('/archive-passwords')
  })
  ipcMain.handle('archive-passwords:import', async (_e, passwords: string[]) => {
    await postBackend('/archive-passwords/import', {
      passwords,
      source: 'manual',
    })
  })
  ipcMain.handle('archive-passwords:forget', async (_e, password: string) => {
    await postBackend('/archive-passwords/delete', { password })
  })

  // Proxy HTTP via sessão persist:terabox — usa cookies reais do browser, bypass fingerprint
  ipcMain.handle(
    'terabox:net-request',
    async (
      _e,
      reqParams: {
        url: string
        method?: string
        headers?: Record<string, string>
        body?: string
      },
    ) => {
      return teraboxNetRequest(reqParams)
    },
  )
  ipcMain.handle('dialog:chooseDirectory', async () => {
    const window = BrowserWindow.getFocusedWindow() ?? BrowserWindow.getAllWindows()[0]
    const result = await dialog.showOpenDialog(window, {
      properties: ['openDirectory', 'createDirectory'],
    })
    if (result.canceled || result.filePaths.length === 0) return ''
    return result.filePaths[0]
  })

  // IPC: settings (preferências no JSON + segredos no SQLite local)
  ipcMain.handle('settings:load', async () => {
    await loadPublicSettings().catch(() => null)
    await loadSecureSettings().catch(() => null)
    return currentSettingsSnapshot()
  })
  ipcMain.handle('settings:save', async (_e, s: unknown) => {
    const currentDisk = storage.getPublicSettings()
    const next = (s as Partial<AppSettingsSnapshot>) ?? {}
    const { nopechaApiKey, ...publicPatch } = next

    const nextDisk = {
      ...currentDisk,
      ...publicPatch,
    }

    if (Object.prototype.hasOwnProperty.call(next, 'nopechaApiKey')) {
      storage.setNopechaApiKey(nopechaApiKey)
    }

    await storage.persistPublicSettings(nextDisk)
    await storage.persistSecureSettings()
    await syncBackendConfig(nextDisk.maxConcurrentDownloads)
    configureClipboardMonitor(nextDisk.clipboardMonitorEnabled)
    await remoteAccessServer.configure(nextDisk)
    return storage.currentSettingsSnapshot()
  })

  ipcMain.handle('system:disk-space', async (_event, rawPath: string) => {
    let target = resolve(expandUserPath(rawPath || storage.getPublicSettings().outputDir || app.getPath('downloads')))
    while (!existsSync(target) && dirname(target) !== target) {
      target = dirname(target)
    }
    const stats = statfsSync(target)
    return {
      path: target,
      freeBytes: Number(stats.bavail) * Number(stats.bsize),
      totalBytes: Number(stats.blocks) * Number(stats.bsize),
    }
  })

  ipcMain.handle('cache:stats', async () => localCacheStats())
  ipcMain.handle('cache:clear', async (_event, ids: string[]) => clearLocalCache(ids))

  ipcMain.handle('remote:info', async () => {
    await loadPublicSettings().catch(() => null)
    const settings = storage.getPublicSettings()
    await remoteAccessServer.configure(settings).catch(() => null)
    return remoteAccessServer.info(settings)
  })

  ipcMain.handle('remote:generateCredentials', () => {
    const current = storage.getPublicSettings().remoteAccess
    return {
      ...generateRemoteAccessCredentials(),
      enabled: Boolean(current?.enabled),
      port: current?.port ?? 9786,
    }
  })
  ipcMain.handle('remote:revokeSession', (_event, id: string) => {
    return remoteAccessServer.revokeSession(id)
  })

  ipcMain.handle('tor:status', async () => {
    return torStatusPayload()
  })
  ipcMain.handle('tor:connect', async () => {
    const pausedIds = await pauseActiveDownloadsForNetworkSwitch()
    try {
      let endpoint = await detectTorEndpoint()
      if (!endpoint) {
        await tryStartTor()
        endpoint = await waitForTorEndpoint()
        if (endpoint && managedTorProcess && managedTorBootstrap < 100) {
          const bootstrapped = await waitForManagedTorBootstrap()
          if (!bootstrapped) {
            if (managedTorProcess && !managedTorProcess.killed) {
              managedTorProcess.kill()
              managedTorProcess = null
            }
            const percent = managedTorBootstrap
            managedTorBootstrap = 0
            throw new Error(`Tor iniciou em ${endpoint.host}:${endpoint.port}, mas parou em ${percent}% do bootstrap. Verifique bloqueio de rede/firewall e tente novamente.`)
          }
        }
      }
      if (!endpoint) {
        throw new Error('Tor embutido não iniciou em 127.0.0.1:9150.')
      }
      await persistTorProxy(true, endpoint)
      await testTorConnection()
      return torStatusPayload()
    } catch (error) {
      await persistTorProxy(false).catch(() => null)
      throw error
    } finally {
      await resumeDownloadsAfterNetworkSwitch(pausedIds)
    }
  })
  ipcMain.handle('tor:disconnect', async () => {
    const pausedIds = await pauseActiveDownloadsForNetworkSwitch()
    try {
      await persistTorProxy(false)
      lastTorExit = null
      if (managedTorProcess && !managedTorProcess.killed) {
        managedTorProcess.kill()
        managedTorProcess = null
      }
      return torStatusPayload()
    } finally {
      await resumeDownloadsAfterNetworkSwitch(pausedIds)
    }
  })
  ipcMain.handle('tor:testConnection', async () => {
    await testTorConnection()
    return torStatusPayload()
  })
  ipcMain.handle('tor:newIdentity', async () => {
    const pausedIds = await pauseActiveDownloadsForNetworkSwitch()
    try {
      await signalTorNewIdentity()
      await testTorConnection()
      return torStatusPayload()
    } finally {
      await resumeDownloadsAfterNetworkSwitch(pausedIds)
    }
  })

  ipcMain.handle('config:test-proxy', async () => {
    if (!rustPort) throw new Error('Backend not available')
    const response = await fetch(`http://127.0.0.1:${rustPort}/config/test-proxy`)
    if (!response.ok) {
      const body = await response.json().catch(() => ({ error: `HTTP ${response.status}` }))
      throw new Error(body.error ?? `HTTP ${response.status}`)
    }
    return response.json()
  })

  ipcMain.handle('intercept:status', async () => {
    return fetchBackendConfig('/intercept/status')
  })
  ipcMain.handle('intercept:install-ca', async () => {
    const info = await fetchBackendConfig<{ caCertPath?: string }>('/intercept/status')
    if (!info.caCertPath) return false
    await shell.openPath(info.caCertPath)
    return true
  })
  ipcMain.handle('intercept:open-proxy-settings', async () => {
    if (process.platform === 'darwin') {
      await shell.openExternal('x-apple.systempreferences:com.apple.Network-Settings.extension')
      return true
    }
    if (process.platform === 'win32') {
      await shell.openExternal('ms-settings:network-proxy')
      return true
    }
    await shell.openExternal('x-scheme-handler/settings')
    return true
  })

  // IPC: auth
  ipcMain.handle('auth:isLoggedIn', (_e, moduleId: string) => {
    const normalized = moduleId.toLowerCase()
    if (normalized === 'terabox') return teraboxService.isLoggedIn()
    return false
  })
  ipcMain.handle('auth:accountInfo', (_e, moduleId: string) => {
    const normalized = moduleId.toLowerCase()
    if (normalized === 'terabox') return teraboxService.accountInfo()
    return null
  })
  ipcMain.handle('auth:login', async (_e, moduleId: string, params: Record<string, string>) => {
    const normalized = moduleId.toLowerCase()
    if (normalized === 'terabox') {
      return teraboxService.login(params)
    }
    throw new Error('Módulo sem suporte a conta')
  })
  ipcMain.handle('auth:logout', (_e, moduleId: string) => {
    const normalized = moduleId.toLowerCase()
    if (normalized === 'terabox') return teraboxService.logout()
    return false
  })

  // IPC: histórico de downloads
  ipcMain.handle('history:load', async (_e, filters?: HistorySearchFilters) => {
    return loadHistoryFromBackend(filters)
  })
  ipcMain.handle('history:save', async (_e, items: unknown) => {
    await saveHistoryToBackend(Array.isArray(items) ? (items as PersistedHistoryItem[]) : [])
  })
  ipcMain.handle('history:append', async (_e, item: PersistedHistoryItem) => {
    await appendHistoryItemToBackend(item)
  })
  ipcMain.handle('history:hosts', async () => {
    return loadHistoryHostsFromBackend()
  })
  ipcMain.handle('history:remove', async (_e, id: string) => {
    await removeHistoryItemInBackend(id)
  })
  ipcMain.handle('history:clear', async () => {
    await clearHistoryInBackend()
  })

  // NoPecha: auto-resolve captchas
  ipcMain.handle(
    'captcha:nopecha-solve',
    async (
      _e,
      params: {
        type: string
        sitekey: string
        pageurl: string
      },
    ) => {
      return solveCaptchaWithNopecha(params)
    },
  )

  ipcMain.handle(
    'captcha:open-window',
    async (
      _e,
      params: {
        provider?: string
        pageUrl: string
        sourceUrl?: string
      },
    ) => {
      return captchaWindowService.solve(params)
    },
  )

  // IPC: tray stats update
  ipcMain.on(
    'tray:update-stats',
    (_event, { activeCount, speed }: { activeCount: number; speed: string }) => {
      const activeTray = tray
      const activeWin = mainWindow
      if (activeTray && activeWin) {
        activeTray.setToolTip(`gDownloader — ${activeCount} baixando · ${speed}`)
        updateTrayMenu(activeWin, activeTray, activeCount, speed)
      }
    },
  )

  // Inicia o proxy local do Terabox (usa sessão browser com cookies reais)
  await new Promise<void>((resolve) => {
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    const http = require('http') as typeof import('http')
    const server = http.createServer(async (req, res) => {
      if (req.method !== 'POST') {
        res.writeHead(405)
        res.end()
        return
      }
      const chunks: Buffer[] = []
      req.on('data', (c: Buffer) => chunks.push(c))
      req.on('end', async () => {
        try {
          const body = JSON.parse(Buffer.concat(chunks).toString('utf8')) as {
            action?: string
            url?: string
            method?: string
            headers?: Record<string, string>
            destPath?: string
            jobId?: string
          }
          const result = body.action?.startsWith('terabox_')
            ? await teraboxService.handleAction(body)
            : body.action?.startsWith('akirabox_')
              ? await akiraboxService.handleAction(body)
              : body.action?.startsWith('katfile_')
                ? await katfileService.handleAction(body)
                : await teraboxNetRequest({
                    url: body.url ?? '',
                    method: body.method,
                    headers: body.headers,
                  })
          res.writeHead(200, { 'Content-Type': 'application/json' })
          res.end(JSON.stringify(result))
        } catch (e) {
          res.writeHead(500, { 'Content-Type': 'application/json' })
          res.end(JSON.stringify({ error: String(e) }))
        }
      })
    })
    server.listen(0, '127.0.0.1', () => {
      const addr = server.address() as { port: number }
      teraboxProxyPort = addr.port
      logMain('terabox-proxy', `porta ${teraboxProxyPort}`)
      resolve()
    })
  })

  // Inicia o backend Rust antes de abrir a janela
  try {
    rustPort = await backendRuntime.start()
    await loadPublicSettings()
    await loadSecureSettings()
    await migrateLegacySettings()
    await loadPublicSettings()
    await clearStaleTorProxy()
    const settings = currentSettingsSnapshot()
    await syncBackendConfig(settings.maxConcurrentDownloads)
    configureClipboardMonitor(settings.clipboardMonitorEnabled)
    await remoteAccessServer.configure(settings)
  } catch (err) {
    logMain('electron', 'Backend não pôde ser iniciado', err)
  }

  mainWindow = createWindow()
  createTray(mainWindow)

  app.on('activate', () => {
    // macOS: recria a janela ao clicar no ícone do dock se não houver janelas abertas
    if (!rustPort) {
      void (async () => {
        try {
          rustPort = await backendRuntime.start()
          await loadPublicSettings()
          await loadSecureSettings()
          await clearStaleTorProxy()
          await syncBackendConfig(storage.getPublicSettings().maxConcurrentDownloads)
          configureClipboardMonitor(storage.getPublicSettings().clipboardMonitorEnabled)
          await remoteAccessServer.configure(storage.getPublicSettings())
        } catch (error) {
          logMain('electron', 'Falha ao reativar backend Rust', error)
        }
      })()
    }
    if (BrowserWindow.getAllWindows().length === 0) {
      mainWindow = createWindow()
      if (!tray && mainWindow) createTray(mainWindow)
    }
  })
})

// Para o backend Rust quando todas as janelas são fechadas
app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    backendRuntime.markQuitting()
  }
  backendRuntime.stop()
  void remoteAccessServer.stop()
  rustPort = null
  if (process.platform !== 'darwin') app.quit()
})

// Garante que o backend para mesmo se o Electron fechar inesperadamente
app.on('before-quit', () => {
  tray?.destroy()
  tray = null
  backendRuntime.markQuitting()
  backendRuntime.stop()
  void remoteAccessServer.stop()
  rustPort = null
})
