import { appendFile, mkdir, rename, stat } from 'fs/promises'
import { join } from 'path'

import { app } from 'electron'

function resolveLogPath(): string {
  try {
    return join(app.getPath('userData'), 'logs', 'electron.log')
  } catch {
    return join(process.cwd(), 'logs', 'electron.log')
  }
}

const MAX_QUEUED_LOG_BYTES = 512 * 1024
// Sem rotação esse arquivo cresce pra sempre (visto na prática: 2.6GB depois de
// dias de driveInterval do Katfile logando a cada ~1.8s). Gira pra .old ao
// passar de 20MB; mantém só a geração anterior, sem acumular histórico.
const MAX_LOG_FILE_BYTES = 20 * 1024 * 1024
const pendingLines: string[] = []
let pendingBytes = 0
let flushScheduled = false
let flushing = false
// Bytes escritos desde a última checagem de rotação. Evita um stat() a cada
// flush (que pode disparar várias vezes por segundo em picos de log) sem
// deixar a sessão crescer sem limite: reavalia a cada ~MAX_LOG_FILE_BYTES
// escritos, então mesmo uma sessão de dias ligados acaba rotacionando.
let bytesSinceRotationCheck = MAX_LOG_FILE_BYTES + 1

async function rotateIfNeeded(logPath: string, writtenBytes: number): Promise<void> {
  bytesSinceRotationCheck += writtenBytes
  if (bytesSinceRotationCheck < MAX_LOG_FILE_BYTES) return
  bytesSinceRotationCheck = 0
  try {
    const info = await stat(logPath)
    if (info.size > MAX_LOG_FILE_BYTES) {
      await rename(logPath, `${logPath}.old`)
    }
  } catch {
    // Arquivo ainda não existe — nada a rotacionar.
  }
}

function normalizePayload(payload: unknown): string {
  if (payload === undefined) {
    return ''
  }

  if (payload instanceof Error) {
    return JSON.stringify({
      name: payload.name,
      message: payload.message,
      stack: payload.stack,
    })
  }

  try {
    return JSON.stringify(payload)
  } catch {
    return String(payload)
  }
}

function scheduleFlush(): void {
  if (flushScheduled || flushing) return
  flushScheduled = true
  setImmediate(() => {
    flushScheduled = false
    void flushLogs()
  })
}

async function flushLogs(): Promise<void> {
  if (flushing || pendingLines.length === 0) return
  flushing = true
  const lines = pendingLines.splice(0)
  pendingBytes = 0

  try {
    const logPath = resolveLogPath()
    const body = lines.join('')
    await mkdir(join(logPath, '..'), { recursive: true })
    await rotateIfNeeded(logPath, Buffer.byteLength(body, 'utf8'))
    await appendFile(logPath, body, 'utf8')
  } catch {
    // Log é best effort; nunca deve atrasar a thread principal.
  } finally {
    flushing = false
    if (pendingLines.length > 0) scheduleFlush()
  }
}

export function logMain(scope: string, message: string, payload?: unknown): void {
  const timestamp = new Date().toISOString()
  const suffix = normalizePayload(payload)
  const line = `${timestamp} [${scope}] ${message}${suffix ? ` ${suffix}` : ''}\n`

  // Evita que uma rajada excepcional de eventos cresça a memória sem limite.
  // Os eventos mais recentes são os mais úteis para diagnóstico.
  if (pendingBytes + Buffer.byteLength(line, 'utf8') > MAX_QUEUED_LOG_BYTES) {
    pendingLines.splice(0, Math.max(1, Math.floor(pendingLines.length / 2)))
    pendingBytes = Buffer.byteLength(pendingLines.join(''), 'utf8')
  }
  pendingLines.push(line)
  pendingBytes += Buffer.byteLength(line, 'utf8')
  scheduleFlush()
}
