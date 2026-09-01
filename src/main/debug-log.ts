import { appendFile, mkdir } from 'fs/promises'
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
const pendingLines: string[] = []
let pendingBytes = 0
let flushScheduled = false
let flushing = false

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
    await mkdir(join(logPath, '..'), { recursive: true })
    await appendFile(logPath, lines.join(''), 'utf8')
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
