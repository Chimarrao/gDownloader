import { app, BrowserWindow, ipcMain, shell } from 'electron'
import { join } from 'path'
import { spawn, ChildProcess } from 'child_process'
import { electronApp, optimizer, is } from '@electron-toolkit/utils'

// Processo filho do backend Rust
let rustBackend: ChildProcess | null = null
// Porta em que o backend Rust está rodando (lida do stdout do binário)
let rustPort: number | null = null

// Determina o caminho do binário Rust conforme o ambiente
// Em desenvolvimento: usa o binário compilado em debug
// Em produção: usa o binário empacotado junto com o Electron em resources/
function getRustBinaryPath(): string {
  if (is.dev) {
    // __dirname aponta para out/main em dev
    // O binário Rust fica em backend/target/debug/ a partir da raiz do projeto
    return join(__dirname, '../../backend/target/debug/gdownloader-backend')
  }
  // Em produção, o binário é copiado para resources/ pelo electron-builder
  return join(process.resourcesPath, 'gdownloader-backend')
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

  // IPC: Vue renderer pede a porta do backend Rust
  // window.api.getBackendPort() no Vue chama isso via preload
  ipcMain.handle('backend:getPort', () => rustPort)

  // IPC: abrir arquivo/pasta no explorador do sistema operacional
  ipcMain.handle('shell:openPath', (_e, targetPath: string) => shell.openPath(targetPath))
  ipcMain.handle('shell:showInFolder', (_e, targetPath: string) =>
    shell.showItemInFolder(targetPath)
  )

  // Inicia o backend Rust antes de abrir a janela
  try {
    await startRustBackend()
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
