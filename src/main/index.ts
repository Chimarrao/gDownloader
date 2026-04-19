import { app, BrowserWindow, clipboard, dialog, ipcMain, Notification, shell } from 'electron'
import { basename, dirname, extname, join } from 'path'
import { spawn, ChildProcess } from 'child_process'
import { existsSync, lstatSync, mkdirSync, readFileSync, writeFileSync } from 'fs'
import { constants as cryptoConstants, createDecipheriv, createHash, publicEncrypt } from 'crypto'
import { electronApp, optimizer, is } from '@electron-toolkit/utils'

// Caminho para os arquivos de dados persistidos
const settingsPath = join(process.cwd(), 'settings.json')
const historyPath = join(app.getPath('userData'), 'history.json')
const defaultSettings = {
  theme: 'dark-purple',
  locale: 'pt-BR',
  outputDir: '~/Downloads',
  maxConcurrentDownloads: 3,
  maxRetriesPerDownload: 3,
  speedLimitKib: 0,
  parallelPartsPerDownload: 4,
  fontSize: 14,
  fontFamily: 'Inter',
  uiZoom: 1,
  nativeNotification: true,
  accounts: {}
}

interface RootSettings {
  theme: string
  locale: string
  outputDir: string
  maxConcurrentDownloads: number
  maxRetriesPerDownload: number
  speedLimitKib: number
  parallelPartsPerDownload: number
  fontSize: number
  fontFamily: string
  uiZoom: number
  nativeNotification: boolean
  accounts: {
    terabox?: {
      email: string
      password: string
      cookies?: string[]
      verifiedAt?: string
    }
  }
}

interface TeraboxVerifyResult {
  cookies: string[]
  verifiedAt: string
}

// Processo filho do backend Rust
let rustBackend: ChildProcess | null = null
// Porta em que o backend Rust está rodando (lida do stdout do binário)
let rustPort: number | null = null

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
  const suffixes = ['.tar.gz', '.tar.bz2', '.tar.xz', '.tar.zst', '.tgz', '.tbz2', '.txz', '.zip', '.rar', '.7z', '.tar', '.gz', '.bz2', '.xz', '.zst']
  const matched = suffixes.find((suffix) => lower.endsWith(suffix))
  const name = matched ? base.slice(0, base.length - matched.length) : base.slice(0, base.length - extname(base).length)
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
    }) => Promise<{ extract: (options?: Record<string, never>) => { files: Iterable<unknown> } }>
  }

  const wasmBinary = toArrayBuffer(readFileSync(require.resolve('node-unrar-js/dist/js/unrar.wasm')))
  const extractor = await unrar.createExtractorFromFile({
    filepath: archivePath,
    targetPath: outputDir,
    wasmBinary
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
    printErr: (line: string) => errors.push(line)
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
        `Expand-Archive -LiteralPath '${archivePath.replace(/'/g, "''")}' -DestinationPath '${outputDir.replace(/'/g, "''")}' -Force`
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
        console.warn('[Electron] Falha no extrator embutido de RAR, tentando fallback:', error)
      }
    }

    try {
      return await extractWith7zWasm(archivePath, outputDir)
    } catch (error) {
      console.warn('[Electron] Falha no extrator embutido de 7z/RAR, tentando fallback:', error)
    }

    const tool = await findFirstCommand(['7z', '7za', 'unar'])
    if (!tool) {
      throw new Error('Não foi possível extrair este arquivo com os extratores embutidos e nenhuma ferramenta externa foi encontrada')
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

function readSettingsFromDisk(): RootSettings {
  if (!existsSync(settingsPath)) return { ...defaultSettings }
  try {
    const parsed = JSON.parse(readFileSync(settingsPath, 'utf8')) as Partial<RootSettings>
    return {
      ...defaultSettings,
      ...parsed,
      accounts: {
        ...defaultSettings.accounts,
        ...(parsed.accounts ?? {}),
      },
    }
  } catch {
    return { ...defaultSettings }
  }
}

function extractTeraboxJsToken(html: string): string {
  const match = html.match(/fn%28%22([^"]+)/)
  if (!match?.[1]) {
    throw new Error('Não foi possível extrair o jsToken do Terabox')
  }
  return match[1]
}

function extractTeraboxPcftoken(html: string): string {
  const match = html.match(/"pcftoken":"([^"]+)"/)
  if (!match?.[1]) {
    throw new Error('Não foi possível extrair o pcftoken do Terabox')
  }
  return match[1]
}

function base64UrlToBase64(value: string): string {
  return value.replace(/_/g, '/').replace(/-/g, '+')
}

function decryptTeraboxPublicKey(pp1: string, pp2: string): string {
  const cipherText = base64UrlToBase64(pp1)
  const iv = Buffer.from(cipherText.slice(0, 16), 'utf8')
  const key = Buffer.from(base64UrlToBase64(pp2), 'utf8')
  const decipher = createDecipheriv('aes-128-cbc', key, iv)
  let publicKey = decipher.update(cipherText.slice(16), 'base64', 'utf8')
  publicKey += decipher.final('utf8')
  return publicKey
}

function encryptTeraboxPassword(password: string, publicKey: string): string {
  const md5 = createHash('md5').update(password).digest('hex')
  const prepared = md5 + String(md5.length).padStart(2, '0')
  return publicEncrypt(
    {
      key: publicKey,
      padding: cryptoConstants.RSA_PKCS1_PADDING,
    },
    Buffer.from(prepared, 'utf8')
  )
    .toString('base64')
    .replace(/\+/g, '-')
    .replace(/\//g, '_')
}

function responseCookies(response: Response): string[] {
  const headers = response.headers as Headers & { getSetCookie?: () => string[] }
  if (typeof headers.getSetCookie === 'function') {
    return headers
      .getSetCookie()
      .map((cookie) => cookie.split(';')[0])
      .filter(Boolean)
  }

  const combined = response.headers.get('set-cookie')
  if (!combined) return []
  return combined
    .split(/,(?=[^;]+=[^;]+)/)
    .map((cookie) => cookie.split(';')[0].trim())
    .filter(Boolean)
}

async function verifyTeraboxAccount(email: string, password: string): Promise<TeraboxVerifyResult> {
  const loginPageUrl = 'https://www.1024tera.com/portuguese/login'
  const userAgent = 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/136 Safari/537.36'

  const loginPage = await fetch(loginPageUrl, {
    headers: {
      'User-Agent': userAgent,
    },
  })
  const loginHtml = await loginPage.text()
  const jsToken = extractTeraboxJsToken(loginHtml)
  const pcftoken = extractTeraboxPcftoken(loginHtml)
  const baseCookies = responseCookies(loginPage)

  const ajaxHeaders = {
    'User-Agent': userAgent,
    'Content-Type': 'application/x-www-form-urlencoded; charset=UTF-8',
    'Origin': 'https://www.1024tera.com',
    'Referer': loginPageUrl,
    'X-Requested-With': 'XMLHttpRequest',
    'Accept': 'application/json, text/javascript, */*; q=0.01',
    'Cookie': baseCookies.join('; '),
  }

  const publicKeyResponse = await fetch('https://www.1024tera.com/passport/getpubkey', {
    headers: {
      'User-Agent': userAgent,
      'Referer': loginPageUrl,
      'X-Requested-With': 'XMLHttpRequest',
      'Cookie': baseCookies.join('; '),
    },
  })
  const publicKeyJson = await publicKeyResponse.json().catch(() => ({})) as Record<string, unknown>
  const publicKeyData = (publicKeyJson.data ?? {}) as Record<string, unknown>
  if (!publicKeyData.pp1 || !publicKeyData.pp2) {
    throw new Error('Não foi possível obter a chave pública do Terabox para validar a conta.')
  }
  const publicKey = decryptTeraboxPublicKey(
    String(publicKeyData.pp1 ?? ''),
    String(publicKeyData.pp2 ?? '')
  )
  const encryptedPassword = encryptTeraboxPassword(password, publicKey)

  const preloginParams = new URLSearchParams({
    app_id: '250528',
    web: '1',
    channel: 'dubox',
    clienttype: '0',
    jsToken,
    'dp-logid': `${Date.now()}${Math.floor(Math.random() * 1000)}`,
  })
  const preloginBody = new URLSearchParams({
    client: 'web',
    pass_version: '2.8',
    lang: 'pt',
    clientfrom: 'h5',
    pcftoken,
    email: email.trim(),
    pwd: encryptedPassword,
  })

  const preloginResponse = await fetch(`https://www.1024tera.com/passport/prelogin?${preloginParams.toString()}`, {
    method: 'POST',
    headers: ajaxHeaders,
    body: preloginBody,
  })
  const preloginJson = await preloginResponse.json().catch(() => ({})) as Record<string, unknown>
  const preloginCode = Number(preloginJson.code ?? preloginJson.errno ?? -1)
  const preloginMsg = String(preloginJson.errmsg ?? preloginJson.msg ?? '')
  if (preloginCode !== 0) {
    if (preloginCode === 10 || preloginMsg.toLowerCase().includes('email format')) {
      throw new Error('O Terabox rejeitou o formato do e-mail informado.')
    }
    throw new Error(preloginMsg || `Falha no prelogin do Terabox (code ${preloginCode})`)
  }

  const preloginData = (preloginJson.data ?? {}) as Record<string, unknown>
  const loginParams = new URLSearchParams({
    app_id: '250528',
    web: '1',
    channel: 'dubox',
    clienttype: '0',
    jsToken,
    'dp-logid': `${Date.now()}${Math.floor(Math.random() * 1000)}`,
  })
  const loginBody = new URLSearchParams({
    client: 'web',
    pass_version: '2.8',
    lang: 'pt',
    clientfrom: 'h5',
    pcftoken,
    email: email.trim(),
    pwd: encryptedPassword,
    seval: String(preloginData.seval ?? ''),
    random: String(preloginData.random ?? ''),
    timestamp: String(preloginData.timestamp ?? ''),
  })

  const response = await fetch(`https://www.1024tera.com/passport/login?${loginParams.toString()}`, {
    method: 'POST',
    headers: ajaxHeaders,
    body: loginBody,
  })

  const json = await response.json().catch(() => ({})) as Record<string, unknown>
  const code = Number(json.code ?? json.errno ?? -1)
  const errmsg = String(json.errmsg ?? json.msg ?? '')

  if (code === 0) {
    return {
      cookies: Array.from(new Set([...baseCookies, ...responseCookies(response)])),
      verifiedAt: new Date().toISOString(),
    }
  }

  if (code === 460020 || errmsg.includes('need verify')) {
    throw new Error('O Terabox exigiu verificação adicional para confirmar esta conta.')
  }
  if (code === 18 || errmsg.toLowerCase().includes('wrong login password')) {
    throw new Error('Senha do Terabox incorreta.')
  }
  if (code === 10 || errmsg.toLowerCase().includes('email format')) {
    throw new Error('O Terabox rejeitou o formato do e-mail informado.')
  }
  if (code === 2) {
    throw new Error('O Terabox retornou erro interno ao validar esta conta. O fluxo do host ainda exige mais ajustes.')
  }

  throw new Error(errmsg || `Falha ao verificar a conta do Terabox (code ${code})`)
}

/**
 * Abre um BrowserWindow modal com o site de login do Terabox.
 * Injeta CSS para mostrar apenas o formulário de login.
 * Quando detectar os cookies de sessão, fecha a janela e retorna os cookies.
 */
function loginTeraboxWithBrowser(parentWin: BrowserWindow): Promise<string[]> {
  const { session } = require('electron') as typeof import('electron')
  return new Promise((resolve, reject) => {
    const loginWin = new BrowserWindow({
      parent: parentWin,
      modal: true,
      width: 480,
      height: 580,
      title: 'Entrar no Terabox',
      autoHideMenuBar: true,
      webPreferences: {
        nodeIntegration: false,
        contextIsolation: true,
        partition: 'persist:terabox',
      },
    })

    // Esconde tudo exceto o formulário de login
    loginWin.webContents.on('dom-ready', () => {
      loginWin.webContents.insertCSS(`
        header, footer, nav, .nav, .sidebar, .banner, .ad-wrap,
        [class*="header"]:not([class*="login"]),
        [class*="footer"], [class*="navbar"], [class*="banner"],
        [class*="promotion"], [class*="download-app"], [class*="top-bar"] {
          display: none !important;
        }
        body { background: #14131f !important; }
      `).catch(() => {})
    })

    // Poll de cookies a cada 800ms
    const tbSession = session.fromPartition('persist:terabox')
    const pollInterval = setInterval(async () => {
      const cookies = await tbSession.cookies.get({ domain: '.terabox.com' })
        .catch(() => tbSession.cookies.get({ domain: '.1024tera.com' }).catch(() => []))
      const sessionCookies = cookies.filter(c =>
        ['ndus', 'ndut', 'BDUSS', 'STOKEN', 'csrfToken'].includes(c.name)
      )
      if (sessionCookies.length >= 2) {
        clearInterval(pollInterval)
        const cookieHeader = sessionCookies.map(c => `${c.name}=${c.value}`).join('; ')
        if (!loginWin.isDestroyed()) loginWin.close()
        resolve([cookieHeader])
      }
    }, 800)

    loginWin.on('closed', () => {
      clearInterval(pollInterval)
      reject(new Error('Login cancelado pelo usuário'))
    })

    loginWin.loadURL('https://www.terabox.com/login')
  })
}

function getRustBinaryName(): string {
  return process.platform === 'win32' ? 'gdownloader-backend.exe' : 'gdownloader-backend'
}

async function syncBackendConfig(maxConcurrentDownloads: number): Promise<void> {
  if (!rustPort) return

  try {
    await fetch(`http://127.0.0.1:${rustPort}/config/downloads`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        max_concurrent_downloads: Math.max(1, Number(maxConcurrentDownloads) || 1)
      })
    })
  } catch (error) {
    console.warn('[Electron] Falha ao sincronizar configuração do backend:', error)
  }
}

// Determina o caminho do binário Rust conforme o ambiente
// Em desenvolvimento: usa o binário compilado em debug
// Em produção: usa o binário empacotado junto com o Electron em resources/
function getRustBinaryPath(): string {
  const binaryName = getRustBinaryName()
  if (is.dev) {
    return join(__dirname, '../../backend/target/debug', binaryName)
  }
  return join(process.resourcesPath, binaryName)
}

// Spawna o processo Rust e lê a porta do stdout
// Retorna uma Promise que resolve com a porta quando o backend estiver pronto
function startRustBackend(): Promise<number> {
  return new Promise((resolve, reject) => {
    const binaryPath = getRustBinaryPath()

    // spawn() inicia um processo filho — como child_process.spawn() no Node.js
    // stdio: 'pipe' captura stdout/stderr para podermos ler
    const dbPath = join(app.getPath('userData'), 'downloads.db')
    rustBackend = spawn(binaryPath, [dbPath], {
      stdio: ['ignore', 'pipe', 'pipe']
    })

    // Lê a porta do stdout do Rust
    // O Rust imprime "PORT:XXXXX" quando o servidor está pronto
    rustBackend.stdout?.on('data', (data: Buffer) => {
      const text = data.toString()
      const match = text.match(/PORT:(\d+)/)
      if (match) {
        rustPort = parseInt(match[1], 10)
        console.log(`[Electron] Backend Rust rodando na porta ${rustPort}`)
        resolve(rustPort)
      }
    })

    // Redireciona logs do Rust para o console do Electron (útil para debug)
    rustBackend.stderr?.on('data', (data: Buffer) => {
      console.log('[Rust]', data.toString().trim())
    })

    rustBackend.on('error', (err) => {
      console.error('[Electron] Falha ao iniciar backend Rust:', err)
      reject(err)
    })

    rustBackend.on('exit', (code) => {
      console.log(`[Electron] Backend Rust encerrou com código ${code}`)
      rustBackend = null
    })

    // Timeout de 15 segundos — se o backend não iniciar neste tempo, rejeita
    setTimeout(() => reject(new Error('Timeout: backend Rust não iniciou em 15 segundos')), 15_000)
  })
}

// Para o processo Rust ao fechar o Electron
function stopRustBackend(): void {
  if (rustBackend) {
    rustBackend.kill()
    rustBackend = null
  }
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
      sandbox: false
    }
  })

  win.on('ready-to-show', () => win.show())

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
  ipcMain.handle('system:notify', (_e, title: string, body?: string) => {
    if (!Notification.isSupported()) return false
    new Notification({ title, body }).show()
    return true
  })
  ipcMain.handle('archive:extract', async (_e, archivePath: string) => {
    return extractArchive(archivePath)
  })
  ipcMain.handle('dialog:chooseDirectory', async () => {
    const window = BrowserWindow.getFocusedWindow() ?? BrowserWindow.getAllWindows()[0]
    const result = await dialog.showOpenDialog(window, {
      properties: ['openDirectory', 'createDirectory']
    })
    if (result.canceled || result.filePaths.length === 0) return ''
    return result.filePaths[0]
  })

  // IPC: settings (lê/escreve JSON em userData)
  ipcMain.handle('settings:load', () => {
    return readSettingsFromDisk()
  })
  ipcMain.handle('settings:save', async (_e, s: unknown) => {
    const current = readSettingsFromDisk()
    const next = (s as Partial<RootSettings>) ?? {}
    const merged: RootSettings = {
      ...defaultSettings,
      ...current,
      ...next,
      accounts: {
        ...defaultSettings.accounts,
        ...(current.accounts ?? {}),
        ...(next.accounts ?? {}),
      },
    }
    mkdirSync(dirname(settingsPath), { recursive: true })
    writeFileSync(settingsPath, JSON.stringify(merged, null, 2))
    await syncBackendConfig(merged.maxConcurrentDownloads)
    return merged
  })

  ipcMain.handle('auth:isLoggedIn', (_e, moduleId: string) => {
    const settings = readSettingsFromDisk()
    if (moduleId.toLowerCase() !== 'terabox') return false
    return Boolean(
      settings.accounts?.terabox?.email &&
      settings.accounts?.terabox?.verifiedAt &&
      settings.accounts?.terabox?.cookies?.length
    )
  })
  ipcMain.handle('auth:accountInfo', (_e, moduleId: string) => {
    const settings = readSettingsFromDisk()
    if (moduleId.toLowerCase() !== 'terabox') return null
    const account = settings.accounts?.terabox
    if (!account?.email) return null
    return {
      email: account.email,
      verifiedAt: account.verifiedAt,
    }
  })
  ipcMain.handle('auth:login', async (_e, moduleId: string, _params: Record<string, string>) => {
    if (moduleId.toLowerCase() !== 'terabox') {
      throw new Error('Módulo sem suporte a conta')
    }

    const parentWin = BrowserWindow.getFocusedWindow() ?? BrowserWindow.getAllWindows()[0]
    const cookies = await loginTeraboxWithBrowser(parentWin)

    const settings = readSettingsFromDisk()
    settings.accounts = settings.accounts ?? {}
    settings.accounts.terabox = {
      email: '',
      password: '',
      cookies,
      verifiedAt: new Date().toISOString(),
    }
    mkdirSync(dirname(settingsPath), { recursive: true })
    writeFileSync(settingsPath, JSON.stringify(settings, null, 2))
    return true
  })
  ipcMain.handle('auth:logout', (_e, moduleId: string) => {
    const settings = readSettingsFromDisk()
    if (moduleId.toLowerCase() !== 'terabox') return false
    settings.accounts = settings.accounts ?? {}
    delete settings.accounts.terabox
    mkdirSync(dirname(settingsPath), { recursive: true })
    writeFileSync(settingsPath, JSON.stringify(settings, null, 2))
    return true
  })

  // IPC: histórico de downloads
  ipcMain.handle('history:load', () => {
    if (!existsSync(historyPath)) return []
    try { return JSON.parse(readFileSync(historyPath, 'utf8')) } catch { return [] }
  })
  ipcMain.handle('history:save', (_e, items: unknown) => {
    writeFileSync(historyPath, JSON.stringify(items, null, 2))
  })
  ipcMain.handle('history:clear', () => {
    writeFileSync(historyPath, '[]')
  })

  // Inicia o backend Rust antes de abrir a janela
  try {
    await startRustBackend()
    const settings = readSettingsFromDisk()
    await syncBackendConfig(settings.maxConcurrentDownloads)
  } catch (err) {
    // Se o backend não iniciar, abre a janela mesmo assim
    // A UI mostrará um indicador de erro de conexão
    console.error('[Electron] Backend não pôde ser iniciado:', err)
  }

  createWindow()

  app.on('activate', () => {
    // macOS: recria a janela ao clicar no ícone do dock se não houver janelas abertas
    if (BrowserWindow.getAllWindows().length === 0) createWindow()
  })
})

// Para o backend Rust quando todas as janelas são fechadas
app.on('window-all-closed', () => {
  stopRustBackend()
  if (process.platform !== 'darwin') app.quit()
})

// Garante que o backend para mesmo se o Electron fechar inesperadamente
app.on('before-quit', () => {
  stopRustBackend()
})
