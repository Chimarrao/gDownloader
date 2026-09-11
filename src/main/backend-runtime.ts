import { spawn, type ChildProcess } from 'child_process'
import { existsSync, lstatSync, mkdirSync } from 'fs'
import { dirname, join } from 'path'

import { app } from 'electron'
import { logMain } from './debug-log'

export interface BackendRuntimeOptions {
  dbPath: string
  startupTimeoutMs?: number
  createEnv: (dbPath: string) => NodeJS.ProcessEnv
  onStdErr?: (message: string) => void
  onRestarted?: (port: number) => Promise<void> | void
}

function getRustBinaryName(): string {
  return process.platform === 'win32' ? 'gdownloader-backend.exe' : 'gdownloader-backend'
}

function getGoBinaryName(): string {
  return process.platform === 'win32' ? 'gdownloader-go.exe' : 'gdownloader-go'
}

export function getRustBinaryPath(): string {
  const binaryName = getRustBinaryName()
  if (!app.isPackaged) {
    const localCandidates = [
      join(__dirname, '../../backend/target/debug', binaryName),
      join(__dirname, '../../backend/target/release', binaryName),
    ]
      .filter((candidate) => existsSync(candidate))
      .sort((left, right) => lstatSync(right).mtimeMs - lstatSync(left).mtimeMs)

    if (localCandidates.length > 0) {
      return localCandidates[0]
    }
  }

  return join(process.resourcesPath, binaryName)
}

export function getGoBinaryPath(): string {
  const binaryName = getGoBinaryName()
  if (!app.isPackaged) {
    const localCandidates = [
      join(__dirname, '../../backend-go/bin', binaryName),
      join(__dirname, '../../backend-go', binaryName),
      join(__dirname, '../backend-go/bin', binaryName),
      join(__dirname, '../../out', binaryName),
      join(__dirname, '../out', binaryName),
      '/tmp/gdownloader-go',
    ]
      .filter((candidate) => existsSync(candidate))
      .sort((left, right) => lstatSync(right).mtimeMs - lstatSync(left).mtimeMs)

    if (localCandidates.length > 0) {
      return localCandidates[0]
    }
  }

  return join(process.resourcesPath, binaryName)
}

export function parseRustReadyPort(chunk: string): number | null {
  const readyMatch = chunk.match(/READY:(\{.+\})/)
  if (readyMatch) {
    try {
      const payload = JSON.parse(readyMatch[1]) as { port?: number }
      if (typeof payload.port === 'number' && payload.port > 0) {
        return payload.port
      }
    } catch {
      // fallback abaixo
    }
  }

  const portMatch = chunk.match(/PORT:(\d+)/)
  if (portMatch) {
    return Number.parseInt(portMatch[1], 10)
  }

  return null
}

// eslint-disable-next-line @typescript-eslint/explicit-function-return-type
export function createBackendRuntime(options: BackendRuntimeOptions) {
  let backend: ChildProcess | null = null
  let port: number | null = null
  let restartTimer: ReturnType<typeof setTimeout> | null = null
  let restartAttempts = 0
  let appIsQuitting = false
  let startPromise: Promise<number> | null = null

  function getPort(): number | null {
    return port
  }

  function markQuitting(): void {
    appIsQuitting = true
  }

  function clearRestartTimer(): void {
    if (restartTimer !== null) {
      clearTimeout(restartTimer)
      restartTimer = null
    }
  }

  function stop(): void {
    clearRestartTimer()
    startPromise = null
    if (backend) {
      logMain('backend-runtime', 'Encerrando processo backend ativo')
      backend.kill()
      backend = null
    }
    port = null
  }

  function scheduleRestart(): void {
    if (appIsQuitting || restartTimer !== null) {
      return
    }

    const delayMs = Math.min(15_000, 1000 * 2 ** Math.min(restartAttempts, 4))
    restartAttempts += 1
    logMain('backend-runtime', 'Agendando reinício do backend', { delayMs, restartAttempts })
    restartTimer = setTimeout(() => {
      restartTimer = null
      void restart()
    }, delayMs)
  }

  async function restart(): Promise<void> {
    if (appIsQuitting || backend) {
      return
    }

    try {
      logMain('backend-runtime', 'Tentando reiniciar backend')
      const nextPort = await start()
      await options.onRestarted?.(nextPort)
    } catch (error) {
      logMain('backend-runtime', 'Falha no reinício do backend', error)
      options.onStdErr?.(`[Electron] Falha ao reiniciar backend Rust: ${String(error)}`)
      scheduleRestart()
    }
  }

  async function start(): Promise<number> {
    if (port && backend) {
      return port
    }

    if (startPromise) {
      return startPromise
    }

    startPromise = new Promise((resolve, reject) => {
      const binaryPath = getRustBinaryPath()
      mkdirSync(dirname(options.dbPath), { recursive: true })
      logMain('backend-runtime', 'Iniciando backend Rust', {
        binaryPath,
        dbPath: options.dbPath,
      })
      backend = spawn(binaryPath, [options.dbPath], {
        stdio: ['ignore', 'pipe', 'pipe'],
        env: options.createEnv(options.dbPath),
      })

      let settled = false
      let stdoutBuffer = ''
      const startupTimeout = setTimeout(() => {
        if (settled) {
          return
        }
        settled = true
        reject(new Error(`Timeout: backend Rust não iniciou em ${(options.startupTimeoutMs ?? 15_000) / 1000} segundos`))
      }, options.startupTimeoutMs ?? 15_000)

      backend.stdout?.on('data', (data: Buffer) => {
        stdoutBuffer += data.toString()
        const readyPort = parseRustReadyPort(stdoutBuffer)
        if (readyPort && !settled) {
          settled = true
          port = readyPort
          restartAttempts = 0
          clearRestartTimer()
          clearTimeout(startupTimeout)
          logMain('backend-runtime', 'Backend sinalizou prontidão', { port: readyPort })
          resolve(readyPort)
        }
      })

      backend.stderr?.on('data', (data: Buffer) => {
        logMain('backend-runtime', 'stderr do backend', data.toString().trim())
        options.onStdErr?.(data.toString().trim())
      })

      backend.on('error', (error) => {
        clearTimeout(startupTimeout)
        startPromise = null
        logMain('backend-runtime', 'Processo backend emitiu erro', error)
        if (!settled) {
          settled = true
          reject(error)
        }
      })

      backend.on('exit', (code) => {
        clearTimeout(startupTimeout)
        backend = null
        port = null
        startPromise = null
        logMain('backend-runtime', 'Processo backend encerrou', { code, settled })
        if (!settled) {
          settled = true
          reject(new Error(`Backend Rust encerrou antes de sinalizar prontidão (código ${code})`))
          return
        }
        scheduleRestart()
      })
    })

    return startPromise.finally(() => {
      startPromise = null
    })
  }

  // Reinício forçado (a pedido, ex.: após instalar ffmpeg para reinjetar a env):
  // encerra o processo atual e sobe outro com o ambiente novo.
  async function forceRestart(): Promise<number> {
    stop()
    return start()
  }

  return {
    getPort,
    markQuitting,
    start,
    stop,
    forceRestart,
  }
}

// Go sidecar para os 6 módulos migrados (migrations, models, config, captcha, health, history)
// Build: cd backend-go && go build -o ../out/gdownloader-go .  (ou ../backend/target/debug/gdownloader-go)
// O Go compartilha o mesmo SQLite em WAL, então Rust+Go podem coexistir no mesmo arquivo.
export function createGoRuntime(options: BackendRuntimeOptions) {
  let backend: ChildProcess | null = null
  let port: number | null = null
  let startPromise: Promise<number> | null = null
  let appIsQuitting = false

  function getPort(): number | null {
    return port
  }
  function markQuitting(): void {
    appIsQuitting = true
  }
  function stop(): void {
    startPromise = null
    if (backend) {
      logMain('go-runtime', 'Encerrando Go sidecar')
      backend.kill()
      backend = null
    }
    port = null
  }

  async function start(): Promise<number> {
    if (port && backend) return port
    if (startPromise) return startPromise
    startPromise = new Promise((resolve, reject) => {
      const binaryPath = getGoBinaryPath()
      if (!existsSync(binaryPath)) {
        logMain('go-runtime', 'Binário Go não encontrado, tentando go run', { binaryPath })
        // Fallback: tenta go run direto (dev sem build)
        const goBinary = spawn('go', ['run', '.', options.dbPath], {
          cwd: join(__dirname, '../../backend-go'),
          stdio: ['ignore', 'pipe', 'pipe'],
          env: options.createEnv(options.dbPath),
        })
        backend = goBinary
        let settled = false
        let stdoutBuffer = ''
        const startupTimeout = setTimeout(() => {
          if (!settled) {
            settled = true
            reject(new Error(`Timeout Go sidecar`))
          }
        }, options.startupTimeoutMs ?? 15000)
        goBinary.stdout?.on('data', (data: Buffer) => {
          stdoutBuffer += data.toString()
          const readyPort = parseRustReadyPort(stdoutBuffer)
          if (readyPort && !settled) {
            settled = true
            port = readyPort
            clearTimeout(startupTimeout)
            logMain('go-runtime', 'Go sidecar pronto (go run)', { port: readyPort })
            resolve(readyPort)
          }
        })
        goBinary.stderr?.on('data', (data: Buffer) => {
          logMain('go-runtime', 'stderr Go', data.toString().trim())
        })
        goBinary.on('error', (error) => {
          clearTimeout(startupTimeout)
          startPromise = null
          if (!settled) {
            settled = true
            reject(error)
          }
        })
        goBinary.on('exit', (code) => {
          clearTimeout(startupTimeout)
          backend = null
          port = null
          startPromise = null
          if (!settled) {
            settled = true
            reject(new Error(`Go sidecar saiu antes de pronto (código ${code})`))
          }
        })
        return
      }
      logMain('go-runtime', 'Iniciando Go sidecar', { binaryPath, dbPath: options.dbPath })
      backend = spawn(binaryPath, [options.dbPath], {
        stdio: ['ignore', 'pipe', 'pipe'],
        env: options.createEnv(options.dbPath),
      })
      let settled = false
      let stdoutBuffer = ''
      const startupTimeout = setTimeout(() => {
        if (!settled) {
          settled = true
          reject(new Error(`Timeout Go sidecar`))
        }
      }, options.startupTimeoutMs ?? 15000)
      backend.stdout?.on('data', (data: Buffer) => {
        stdoutBuffer += data.toString()
        const readyPort = parseRustReadyPort(stdoutBuffer)
        if (readyPort && !settled) {
          settled = true
          port = readyPort
          clearTimeout(startupTimeout)
          logMain('go-runtime', 'Go sidecar pronto', { port: readyPort })
          resolve(readyPort)
        }
      })
      backend.stderr?.on('data', (data: Buffer) => {
        logMain('go-runtime', 'stderr Go', data.toString().trim())
        options.onStdErr?.(data.toString().trim())
      })
      backend.on('error', (error) => {
        clearTimeout(startupTimeout)
        startPromise = null
        if (!settled) {
          settled = true
          reject(error)
        }
      })
      backend.on('exit', (code) => {
        clearTimeout(startupTimeout)
        backend = null
        port = null
        startPromise = null
        if (!settled) {
          settled = true
          reject(new Error(`Go sidecar encerrou antes de pronto (código ${code})`))
          return
        }
        if (!appIsQuitting) {
          logMain('go-runtime', 'Go sidecar encerrou, não reinicia automático (Rust é primário)')
        }
      })
    })
    return startPromise.finally(() => {
      startPromise = null
    })
  }

  async function forceRestart(): Promise<number> {
    stop()
    return start()
  }

  return {
    getPort,
    markQuitting,
    start,
    stop,
    forceRestart,
  }
}
