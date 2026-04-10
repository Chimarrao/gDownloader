import { app } from 'electron'
import { readFileSync, writeFileSync, existsSync, mkdirSync } from 'fs'
import { join, dirname } from 'path'
import type { PersistedDownloadItem } from '../shared/types'

export type { PersistedDownloadItem }

function getPath(): string {
  return join(app.getPath('userData'), 'downloads-state.json')
}

export function loadDownloads(): PersistedDownloadItem[] {
  const p = getPath()
  if (!existsSync(p)) return []
  try {
    const parsed = JSON.parse(readFileSync(p, 'utf-8'))
    return Array.isArray(parsed) ? parsed : []
  } catch {
    return []
  }
}

export function saveDownloads(items: PersistedDownloadItem[]): void {
  const p = getPath()
  mkdirSync(dirname(p), { recursive: true })
  writeFileSync(p, JSON.stringify(items, null, 2), 'utf-8')
}
