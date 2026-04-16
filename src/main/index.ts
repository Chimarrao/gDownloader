import { app, BrowserWindow, clipboard, dialog, ipcMain, Notification, shell } from 'electron'
import { basename, dirname, extname, join } from 'path'
import { spawn, ChildProcess } from 'child_process'
import { existsSync, lstatSync, mkdirSync, readFileSync, writeFileSync } from 'fs'
import { electronApp, optimizer, is } from '@electron-toolkit/utils'

// Caminho para os arquivos de dados persistidos
const settingsPath = join(app.getPath('userData'), 'settings.json')
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
  nativeNotification: true
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
    const tool = await findFirstCommand(['7z', '7za', 'unar'])
    if (!tool) {
      throw new Error('RAR/7Z exige 7z, 7za ou unar instalado no sistema')
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

function readSettingsFromDisk() {
  if (!existsSync(settingsPath)) return { ...defaultSettings }
  try {
    return { ...defaultSettings, ...JSON.parse(readFileSync(settingsPath, 'utf8')) }
  } catch {
    return { ...defaultSettings }
  }
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
    rustBackend = spawn(binaryPath, [], {
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
    const merged = { ...defaultSettings, ...(s as object) }
    writeFileSync(settingsPath, JSON.stringify(merged, null, 2))
    await syncBackendConfig(merged.maxConcurrentDownloads)
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
