import { app } from 'electron'
import { readFileSync, writeFileSync, existsSync, mkdirSync } from 'fs'
import { join, dirname } from 'path'
import type { PersistedSettings } from '../shared/types'

const DEFAULTS: PersistedSettings = {
  theme: 'dark-purple',
  locale: 'pt-BR',
  outputDir: '~/Downloads',
  maxConcurrentDownloads: 3,
  fontSize: 14,
  fontFamily: 'Inter',
  uiZoom: 1.0,
  nativeNotification: true
}

function getPath(): string {
  return join(app.getPath('userData'), 'settings.json')
}

export function loadSettings(): PersistedSettings {
  const p = getPath()
  if (!existsSync(p)) return { ...DEFAULTS }
  try {
    return { ...DEFAULTS, ...JSON.parse(readFileSync(p, 'utf-8')) }
  } catch {
    return { ...DEFAULTS }
  }
}

export function saveSettings(s: PersistedSettings): void {
  const p = getPath()
  mkdirSync(dirname(p), { recursive: true })
  writeFileSync(p, JSON.stringify(s, null, 2), 'utf-8')
}
