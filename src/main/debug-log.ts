import { appendFileSync, mkdirSync } from 'fs'
import { join } from 'path'

import { app } from 'electron'

function resolveLogPath(): string {
  try {
    return join(app.getPath('userData'), 'logs', 'electron.log')
  } catch {
    return join(process.cwd(), 'logs', 'electron.log')
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

export function logMain(scope: string, message: string, payload?: unknown): void {
  const timestamp = new Date().toISOString()
  const suffix = normalizePayload(payload)
  const line = `${timestamp} [${scope}] ${message}${suffix ? ` ${suffix}` : ''}\n`

  try {
    const logPath = resolveLogPath()
    mkdirSync(join(logPath, '..'), { recursive: true })
    appendFileSync(logPath, line, 'utf8')
  } catch {
    // Best effort only.
  }

  try {
    if (payload === undefined) {
      console.log(`[${scope}] ${message}`)
    } else {
      console.log(`[${scope}] ${message}`, payload)
    }
  } catch {
    // ignore console failures
  }
}

