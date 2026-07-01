import { execFile } from 'child_process'
import { chmodSync, createWriteStream, existsSync, mkdirSync, renameSync } from 'fs'
import * as https from 'https'
import { join } from 'path'

export interface YtdlpStatus {
  version: string | null
  updateAvailable: boolean
  state: 'ready' | 'downloading' | 'error'
  error?: string
}

export interface YtdlpProgressEvent {
  bytesDownloaded: number
  totalBytes: number
}

export function assetNameForPlatform(platform: string, arch: string): string {
  if (platform === 'win32') return 'yt-dlp.exe'
  if (platform === 'darwin') return 'yt-dlp_macos'
  if (arch === 'arm64') return 'yt-dlp_linux_aarch64'
  return 'yt-dlp'
}

export function resolvedBinPath(customPath: string, managedPath: string): string {
  return customPath.trim() ? customPath.trim() : managedPath
}

function managedDir(userDataPath: string): string {
  return join(userDataPath, 'ytdlp')
}

function managedBinPath(userDataPath: string): string {
  const name = process.platform === 'win32' ? 'yt-dlp.exe' : 'yt-dlp'
  return join(managedDir(userDataPath), name)
}

function httpsGet(url: string): Promise<{ statusCode: number; body: string }> {
  return new Promise((resolve, reject) => {
    https
      .get(url, { headers: { 'User-Agent': 'gDownloader' } }, (res) => {
        if (res.statusCode === 301 || res.statusCode === 302) {
          const location = res.headers.location
          if (location) {
            resolve(httpsGet(location))
            res.resume()
            return
          }
        }
        let body = ''
        res.on('data', (chunk: Buffer) => {
          body += chunk.toString()
        })
        res.on('end', () => resolve({ statusCode: res.statusCode ?? 0, body }))
        res.on('error', reject)
      })
      .on('error', reject)
  })
}

function httpsDownload(
  url: string,
  destPath: string,
  onProgress: (e: YtdlpProgressEvent) => void,
): Promise<void> {
  return new Promise((resolve, reject) => {
    const follow = (redirectUrl: string): void => {
      https
        .get(redirectUrl, { headers: { 'User-Agent': 'gDownloader' } }, (res) => {
          if (res.statusCode === 301 || res.statusCode === 302) {
            const location = res.headers.location
            if (location) {
              follow(location)
              res.resume()
              return
            }
          }
          if ((res.statusCode ?? 0) < 200 || (res.statusCode ?? 0) >= 300) {
            reject(new Error(`HTTP ${res.statusCode ?? 'unknown'} ao baixar yt-dlp`))
            res.resume()
            return
          }
          const total = Number(res.headers['content-length'] ?? 0)
          let received = 0
          const file = createWriteStream(destPath)
          res.on('data', (chunk: Buffer) => {
            received += chunk.length
            onProgress({ bytesDownloaded: received, totalBytes: total })
          })
          res.pipe(file)
          file.on('finish', () => file.close(() => resolve()))
          file.on('error', (err) => {
            file.close()
            reject(err)
          })
          res.on('error', reject)
        })
        .on('error', reject)
    }
    follow(url)
  })
}

function getLocalVersion(binPath: string): Promise<string | null> {
  return new Promise((resolve) => {
    execFile(binPath, ['--version'], { timeout: 10_000 }, (err, stdout) => {
      if (err) {
        resolve(null)
        return
      }
      resolve(stdout.trim() || null)
    })
  })
}

// eslint-disable-next-line @typescript-eslint/explicit-function-return-type
export function createYtdlpService(userDataPath: string) {
  let status: YtdlpStatus = { version: null, updateAvailable: false, state: 'downloading' }
  let cachedLatest: { version: string; fetchedAt: number } | null = null
  let onProgressCallback: ((e: YtdlpProgressEvent) => void) | null = null

  function getStatus(): YtdlpStatus {
    return { ...status }
  }

  function onProgress(cb: (e: YtdlpProgressEvent) => void): void {
    onProgressCallback = cb
  }

  async function fetchLatestVersion(): Promise<string | null> {
    const now = Date.now()
    if (cachedLatest && now - cachedLatest.fetchedAt < 6 * 60 * 60 * 1000) {
      return cachedLatest.version
    }
    try {
      const resp = await httpsGet(
        'https://api.github.com/repos/yt-dlp/yt-dlp/releases/latest',
      )
      if (resp.statusCode !== 200) return null
      const data = JSON.parse(resp.body) as { tag_name?: string }
      const version = data.tag_name?.trim() ?? null
      if (version) cachedLatest = { version, fetchedAt: now }
      return version
    } catch {
      return null
    }
  }

  async function downloadBin(version: string): Promise<void> {
    const asset = assetNameForPlatform(process.platform, process.arch)
    const url = `https://github.com/yt-dlp/yt-dlp/releases/download/${encodeURIComponent(version)}/${asset}`
    const dir = managedDir(userDataPath)
    mkdirSync(dir, { recursive: true })
    const tmpPath = join(dir, 'yt-dlp.tmp')
    await httpsDownload(url, tmpPath, (e) => onProgressCallback?.(e))
    if (process.platform !== 'win32') chmodSync(tmpPath, 0o755)
    renameSync(tmpPath, managedBinPath(userDataPath))
  }

  async function ensureReady(customBinPath: string, autoUpdate: boolean): Promise<void> {
    const managed = managedBinPath(userDataPath)
    const effective = resolvedBinPath(customBinPath, managed)
    const usingManaged = effective === managed

    if (!usingManaged) {
      const version = await getLocalVersion(effective)
      status = version
        ? { version, updateAvailable: false, state: 'ready' }
        : {
            version: null,
            updateAvailable: false,
            state: 'error',
            error: 'Binário customizado não encontrado ou inválido',
          }
      return
    }

    if (!existsSync(managed)) {
      status = { version: null, updateAvailable: false, state: 'downloading' }
      try {
        const latest = await fetchLatestVersion()
        if (!latest) throw new Error('Não foi possível obter a versão mais recente do GitHub')
        await downloadBin(latest)
        status = { version: latest, updateAvailable: false, state: 'ready' }
      } catch (err) {
        status = { version: null, updateAvailable: false, state: 'error', error: String(err) }
      }
      return
    }

    const localVersion = await getLocalVersion(managed)
    if (!localVersion) {
      status = { version: null, updateAvailable: false, state: 'downloading' }
      try {
        const latest = await fetchLatestVersion()
        if (!latest) throw new Error('Não foi possível obter a versão mais recente do GitHub')
        await downloadBin(latest)
        status = { version: latest, updateAvailable: false, state: 'ready' }
      } catch (err) {
        status = { version: null, updateAvailable: false, state: 'error', error: String(err) }
      }
      return
    }

    status = { version: localVersion, updateAvailable: false, state: 'ready' }

    if (!autoUpdate) return

    const latest = await fetchLatestVersion()
    if (!latest || latest === localVersion) return

    status = { ...status, updateAvailable: true }
    try {
      await downloadBin(latest)
      status = { version: latest, updateAvailable: false, state: 'ready' }
    } catch {
      // mantém versão atual silenciosamente
    }
  }

  function effectiveBinPath(customBinPath: string): string {
    return resolvedBinPath(customBinPath, managedBinPath(userDataPath))
  }

  return { getStatus, onProgress, ensureReady, effectiveBinPath }
}
