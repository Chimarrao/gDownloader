import { existsSync, mkdirSync, rmSync } from 'fs'
import { writeFileSync } from 'fs'
import { dirname } from 'path'
import { join } from 'path'
import { app, session, type BrowserWindow } from 'electron'

const configuredPartitions = new Set<string>()

function chromeMajorVersion(): string {
  return (process.versions.chrome || '120').split('.')[0]
}

export const HOSTER_BROWSER_USER_AGENT =
  `Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/${process.versions.chrome || '120.0.0.0'} Safari/537.36`

function ensureHosterPreload(): string {
  const preloadPath = join(app.getPath('userData'), 'hoster-session-preload.js')
  const chromeVersion = process.versions.chrome || '120.0.0.0'
  const chromeMajor = chromeVersion.split('.')[0]
  const source = `
(() => {
  try {
    Object.defineProperty(navigator, 'webdriver', { get: () => undefined });
    Object.defineProperty(navigator, 'languages', { get: () => ['pt-BR', 'pt', 'en-US', 'en'] });
    Object.defineProperty(navigator, 'platform', { get: () => 'MacIntel' });
    Object.defineProperty(navigator, 'userAgentData', {
      get: () => ({
        brands: [
          { brand: 'Chromium', version: '${chromeMajor}' },
          { brand: 'Not A(Brand', version: '24' }
        ],
        mobile: false,
        platform: 'macOS',
        getHighEntropyValues: async (hints) => {
          const values = {
            architecture: 'arm',
            bitness: '64',
            brands: [
              { brand: 'Chromium', version: '${chromeMajor}' },
              { brand: 'Not A(Brand', version: '24' }
            ],
            fullVersionList: [
              { brand: 'Chromium', version: '${chromeVersion}' },
              { brand: 'Not A(Brand', version: '24.0.0.0' }
            ],
            mobile: false,
            model: '',
            platform: 'macOS',
            platformVersion: '15.0.0',
            uaFullVersion: '${chromeVersion}'
          };
          return Object.fromEntries((hints || []).map((hint) => [hint, values[hint]]).filter(([, value]) => value !== undefined));
        }
      }),
      configurable: true
    });
    if (!window.chrome) {
      Object.defineProperty(window, 'chrome', { value: { runtime: {} }, configurable: true });
    }
  } catch {}
})();
`
  try {
    writeFileSync(preloadPath, source, 'utf8')
  } catch {
    // Se não conseguir gravar o preload, ainda seguimos com os headers consistentes.
  }
  return preloadPath
}

export function configureHosterSession(partition: string): void {
  if (configuredPartitions.has(partition)) {
    return
  }
  configuredPartitions.add(partition)

  const targetSession = session.fromPartition(partition)
  targetSession.setPreloads([ensureHosterPreload()])
  targetSession.webRequest.onBeforeSendHeaders((details, callback) => {
    const requestHeaders = { ...details.requestHeaders }
    requestHeaders['User-Agent'] = HOSTER_BROWSER_USER_AGENT
    requestHeaders['Accept-Language'] = 'pt-BR,pt;q=0.9,en-US;q=0.8,en;q=0.7'
    requestHeaders['sec-ch-ua'] = `"Chromium";v="${chromeMajorVersion()}", "Not A(Brand";v="24"`
    requestHeaders['sec-ch-ua-mobile'] = '?0'
    requestHeaders['sec-ch-ua-platform'] = '"macOS"'
    callback({ requestHeaders })
  })
}

export function configureHosterWindow(win: BrowserWindow, partition: string): void {
  configureHosterSession(partition)
  win.webContents.setUserAgent(HOSTER_BROWSER_USER_AGENT)
}

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
