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
import { basename, dirname, extname, join } from 'path'
import { spawn } from 'child_process'
import { existsSync, lstatSync, mkdirSync, readFileSync, readdirSync, watch } from 'fs'
import { electronApp, optimizer, is } from '@electron-toolkit/utils'
import type { AppSettingsSnapshot } from '../shared/types'
import {
  createAppStorage,
  type HistorySearchFilters,
  type PersistedHistoryItem,
} from './app-storage'
import { createBruploadService, type BruploadStoredAccount } from './brupload-service'
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

function persistBruploadAccount(account: BruploadStoredAccount | null): void {
  void storage.persistBruploadAccount(account).catch((error) => {
    logMain('auth', 'Falha ao persistir conta do BRupload no SQLite', error)
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
let lastClipboardUrl = ''
const backendRuntime = createBackendRuntime({
  dbPath: getDatabasePath(),
  createEnv: (dbPath) => ({
    ...process.env,
    TERABOX_PROXY_PORT: String(teraboxProxyPort),
    BRUPLOAD_PROXY_PORT: String(teraboxProxyPort),
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
const bruploadService = createBruploadService({
  readAccount: () => storage.getBruploadAccount(),
  saveAccount: persistBruploadAccount,
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
  for (const _entry of extracted.files) {
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

  for (const url of extractClipboardUrls(text)) {
    if (url === lastClipboardUrl) {
      continue
    }
    const provider = await detectClipboardUrl(url)
    if (!provider?.id) {
      continue
    }

    lastClipboardUrl = url
    logMain('clipboard', 'Link suportado detectado na área de transferência', {
      provider: provider.id,
      url,
    })
    for (const window of BrowserWindow.getAllWindows()) {
      window.webContents.send('clipboard:link-detected', {
        url,
        provider: provider.id,
        providerName: provider.name ?? provider.id,
      })
    }
    return
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

  lastClipboardText = clipboard.readText().trim()
  clipboardMonitorTimer = setInterval(() => {
    void inspectClipboardForLinks()
  }, 800)
  logMain('clipboard', 'Monitor de clipboard ativado')
}

let tray: Tray | null = null
let mainWindow: BrowserWindow | null = null
const logWatchers = new Map<number, ReturnType<typeof watch>>()

function createTray(win: BrowserWindow): void {
  const iconPath = join(__dirname, '../../resources/icon.png')
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
    width: 1200,
    height: 750,
    minWidth: 900,
    minHeight: 600,
    show: false,
    autoHideMenuBar: true,
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

  ipcMain.handle('remote:info', async () => {
    await loadPublicSettings().catch(() => null)
    return remoteAccessServer.info(storage.getPublicSettings())
  })

  ipcMain.handle('remote:generateCredentials', () => {
    const current = storage.getPublicSettings().remoteAccess
    return {
      ...generateRemoteAccessCredentials(),
      enabled: Boolean(current?.enabled),
      port: current?.port ?? 9786,
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
    if (normalized === 'brupload') return bruploadService.isLoggedIn()
    return false
  })
  ipcMain.handle('auth:accountInfo', (_e, moduleId: string) => {
    const normalized = moduleId.toLowerCase()
    if (normalized === 'terabox') return teraboxService.accountInfo()
    if (normalized === 'brupload') return bruploadService.accountInfo()
    return null
  })
  ipcMain.handle('auth:login', async (_e, moduleId: string, params: Record<string, string>) => {
    const normalized = moduleId.toLowerCase()
    if (normalized === 'terabox') {
      return teraboxService.login(params)
    }
    if (normalized === 'brupload') {
      return bruploadService.login()
    }
    throw new Error('Módulo sem suporte a conta')
  })
  ipcMain.handle('auth:logout', (_e, moduleId: string) => {
    const normalized = moduleId.toLowerCase()
    if (normalized === 'terabox') return teraboxService.logout()
    if (normalized === 'brupload') return bruploadService.logout()
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
            : body.action?.startsWith('brupload_')
              ? await bruploadService.handleAction(body)
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
