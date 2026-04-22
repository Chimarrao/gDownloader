import { existsSync, mkdirSync, rmSync } from 'fs'
import { dirname } from 'path'

export const HOSTER_BROWSER_USER_AGENT =
  'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/136 Safari/537.36'

export function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

export function sanitizeFilename(name: string, fallback: string): string {
  const candidate = name.trim() || fallback
  return candidate
    .replace(/[\\/:*?"<>|]+/g, '_')
    .replace(/\s+/g, ' ')
    .trim()
}

export function looksLikeFilename(value: string): boolean {
  return /\.[a-z0-9]{2,5}\b/i.test(value) && value.length >= 4 && value.length <= 220
}

export function parseHumanSize(text: string): number {
  const match = text.match(/([0-9]+(?:[.,][0-9]+)?)\s*(B|KB|MB|GB|TB)/i)
  if (!match) {
    return 0
  }

  const value = Number.parseFloat(match[1].replace(',', '.'))
  if (!Number.isFinite(value)) {
    return 0
  }

  const unit = match[2].toUpperCase()
  const multiplier =
    unit === 'KB'
      ? 1024
      : unit === 'MB'
        ? 1024 ** 2
        : unit === 'GB'
          ? 1024 ** 3
          : unit === 'TB'
            ? 1024 ** 4
            : 1

  return Math.round(value * multiplier)
}

export function ensureDownloadParent(destPath: string): void {
  const parent = dirname(destPath)
  if (!existsSync(parent)) {
    mkdirSync(parent, { recursive: true })
  }
}

export function cleanupPartialDownload(destPath: string): void {
  if (existsSync(destPath)) {
    rmSync(destPath, { force: true, recursive: true })
  }
}

export function createExclusiveRunner() {
  let browserLock = Promise.resolve()

  return async function runExclusive<T>(task: () => Promise<T>): Promise<T> {
    const previous = browserLock
    let release!: () => void
    browserLock = new Promise<void>((resolve) => {
      release = resolve
    })
    await previous.catch(() => undefined)
    try {
      return await task()
    } finally {
      release()
    }
  }
}

