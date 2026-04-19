import { readFileSync, writeFileSync, existsSync, mkdirSync } from 'fs'
import { join, dirname } from 'path'
import type { PersistedSettings } from '../shared/types'

const DEFAULTS: PersistedSettings = {
  theme: 'dark-purple',
  locale: 'pt-BR',
  outputDir: '~/Downloads',
  maxConcurrentDownloads: 3,
  maxRetriesPerDownload: 3,
  speedLimitKib: 0,
  parallelPartsPerDownload: 4,
  fontSize: 14,
  fontFamily: 'Inter',
  uiZoom: 1.0,
  nativeNotification: true,
  accounts: {}
}

function getPath(): string {
  return join(process.cwd(), 'settings.json')
}

export function loadSettings(): PersistedSettings {
  const p = getPath()
  if (!existsSync(p)) return { ...DEFAULTS }
  try {
    const parsed = JSON.parse(readFileSync(p, 'utf-8')) as PersistedSettings
    return {
      ...DEFAULTS,
      ...parsed,
      accounts: {
        ...(DEFAULTS.accounts ?? {}),
        ...(parsed.accounts ?? {}),
      },
    }
  } catch {
    return { ...DEFAULTS }
  }
}

export function saveSettings(s: PersistedSettings): void {
  const p = getPath()
  mkdirSync(dirname(p), { recursive: true })
  writeFileSync(p, JSON.stringify(s, null, 2), 'utf-8')
}
