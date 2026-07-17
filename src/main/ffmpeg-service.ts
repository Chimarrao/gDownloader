import { execFile } from 'child_process'
import { chmodSync, createWriteStream, existsSync, mkdirSync, readdirSync, renameSync, rmSync, statSync, unlinkSync } from 'fs'
import * as https from 'https'
import { join } from 'path'

export interface FfmpegStatus {
  version: string | null
  // 'ready'      = temos um ffmpeg utilizável (sistema, custom ou gerenciado)
  // 'downloading'= baixando/instalando o gerenciado
  // 'absent'     = não achamos nenhum ffmpeg (o usuário pode baixar o gerenciado)
  // 'error'      = falha ao baixar/instalar
  state: 'ready' | 'downloading' | 'absent' | 'error'
  source: 'system' | 'custom' | 'managed' | 'none'
  path: string | null
  error?: string
}

export interface FfmpegProgressEvent {
  bytesDownloaded: number
  totalBytes: number
}

/** Caminhos comuns onde o ffmpeg costuma estar instalado, por plataforma. */
export function commonSystemPaths(platform: string): string[] {
  if (platform === 'win32') {
    return ['C:/ffmpeg/bin/ffmpeg.exe', 'C:/Program Files/ffmpeg/bin/ffmpeg.exe']
  }
  if (platform === 'darwin') {
    return ['/opt/homebrew/bin/ffmpeg', '/usr/local/bin/ffmpeg', '/usr/bin/ffmpeg']
  }
  return ['/usr/bin/ffmpeg', '/usr/local/bin/ffmpeg', '/snap/bin/ffmpeg']
}

/** Fonte de download do build estático de ffmpeg para a plataforma atual. */
export function downloadSourceFor(
  platform: string,
  arch: string,
): { url: string; kind: 'zip' | 'tarxz' } {
  if (platform === 'win32') {
    return {
      url: 'https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip',
      kind: 'zip',
    }
  }
  if (platform === 'darwin') {
    // Zip contendo apenas o binário `ffmpeg` (x86_64; roda via Rosetta em Apple Silicon).
    return { url: 'https://evermeet.cx/ffmpeg/getrelease/ffmpeg/zip', kind: 'zip' }
  }
  const slug = arch === 'arm64' ? 'linuxarm64' : 'linux64'
  return {
    url: `https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-${slug}-gpl.tar.xz`,
    kind: 'tarxz',
  }
}

function managedDir(userDataPath: string): string {
  return join(userDataPath, 'ffmpeg')
}

function managedBinPath(userDataPath: string): string {
  const name = process.platform === 'win32' ? 'ffmpeg.exe' : 'ffmpeg'
  return join(managedDir(userDataPath), name)
}

function httpsDownload(
  url: string,
  destPath: string,
  onProgress: (e: FfmpegProgressEvent) => void,
): Promise<void> {
  return new Promise((resolve, reject) => {
    const follow = (redirectUrl: string): void => {
      https
        .get(redirectUrl, { headers: { 'User-Agent': 'gDownloader' } }, (res) => {
          if (res.statusCode === 301 || res.statusCode === 302 || res.statusCode === 307 || res.statusCode === 308) {
            const location = res.headers.location
            if (location) {
              follow(location)
              res.resume()
              return
            }
          }
          if ((res.statusCode ?? 0) < 200 || (res.statusCode ?? 0) >= 300) {
            reject(new Error(`HTTP ${res.statusCode ?? 'desconhecido'} ao baixar ffmpeg`))
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
            try { unlinkSync(destPath) } catch { /* best effort */ }
            reject(err)
          })
          res.on('error', reject)
        })
        .on('error', reject)
    }
    follow(url)
  })
}

function run(cmd: string, args: string[]): Promise<void> {
  return new Promise((resolve, reject) => {
    execFile(cmd, args, { timeout: 120_000 }, (err) => {
      if (err) reject(err)
      else resolve()
    })
  })
}

/** Roda `ffmpeg -version` e devolve a versão (ou null se não for um ffmpeg válido). */
export function probeFfmpegVersion(binPath: string): Promise<string | null> {
  return new Promise((resolve) => {
    execFile(binPath, ['-version'], { timeout: 10_000 }, (err, stdout) => {
      if (err) {
        resolve(null)
        return
      }
      // Primeira linha: "ffmpeg version 6.1.1 Copyright ..."
      const match = /ffmpeg version (\S+)/i.exec(stdout)
      resolve(match ? match[1] : stdout.trim().split('\n')[0] || null)
    })
  })
}

/** Procura recursivamente por um binário `ffmpeg`/`ffmpeg.exe` dentro de um diretório. */
function findFfmpegBinary(dir: string): string | null {
  const target = process.platform === 'win32' ? 'ffmpeg.exe' : 'ffmpeg'
  let entries: string[]
  try {
    entries = readdirSync(dir)
  } catch {
    return null
  }
  for (const entry of entries) {
    const full = join(dir, entry)
    let isDir = false
    try {
      isDir = statSync(full).isDirectory()
    } catch {
      continue
    }
    if (isDir) {
      const nested = findFfmpegBinary(full)
      if (nested) return nested
    } else if (entry === target) {
      return full
    }
  }
  return null
}

// eslint-disable-next-line @typescript-eslint/explicit-function-return-type
export function createFfmpegService(userDataPath: string) {
  let status: FfmpegStatus = { version: null, state: 'absent', source: 'none', path: null }
  let onProgressCallback: ((e: FfmpegProgressEvent) => void) | null = null
  let activeEnsurePromise: Promise<void> | null = null

  function getStatus(): FfmpegStatus {
    return { ...status }
  }

  function onProgress(cb: (e: FfmpegProgressEvent) => void): void {
    onProgressCallback = cb
  }

  /** Detecta um ffmpeg do sistema (PATH + caminhos comuns). */
  async function detectSystem(): Promise<{ path: string; version: string } | null> {
    const candidates = ['ffmpeg', ...commonSystemPaths(process.platform)]
    for (const candidate of candidates) {
      const version = await probeFfmpegVersion(candidate)
      if (version) return { path: candidate, version }
    }
    return null
  }

  async function _doEnsureReady(customBinPath: string): Promise<void> {
    const custom = customBinPath.trim()
    if (custom) {
      const version = await probeFfmpegVersion(custom)
      status = version
        ? { version, state: 'ready', source: 'custom', path: custom }
        : { version: null, state: 'error', source: 'custom', path: custom, error: 'Binário customizado inválido' }
      return
    }

    // 1) Prefere o ffmpeg do sistema (decisão: usar o instalado se houver).
    const system = await detectSystem()
    if (system) {
      status = { version: system.version, state: 'ready', source: 'system', path: system.path }
      return
    }

    // 2) Senão, usa o gerenciado se já foi baixado.
    const managed = managedBinPath(userDataPath)
    if (existsSync(managed)) {
      const version = await probeFfmpegVersion(managed)
      if (version) {
        status = { version, state: 'ready', source: 'managed', path: managed }
        return
      }
    }

    // 3) Nada encontrado — o usuário pode baixar o gerenciado sob demanda.
    status = { version: null, state: 'absent', source: 'none', path: null }
  }

  async function ensureReady(customBinPath: string): Promise<void> {
    if (activeEnsurePromise) return activeEnsurePromise
    activeEnsurePromise = _doEnsureReady(customBinPath).finally(() => {
      activeEnsurePromise = null
    })
    return activeEnsurePromise
  }

  /** Baixa e instala o build gerenciado do ffmpeg para a plataforma atual. */
  async function download(): Promise<FfmpegStatus> {
    status = { version: null, state: 'downloading', source: 'managed', path: null }
    const dir = managedDir(userDataPath)
    try {
      mkdirSync(dir, { recursive: true })
      const source = downloadSourceFor(process.platform, process.arch)
      const archivePath = join(dir, source.kind === 'zip' ? 'ffmpeg-dl.zip' : 'ffmpeg-dl.tar.xz')
      const extractDir = join(dir, 'extract')
      try { rmSync(extractDir, { recursive: true, force: true }) } catch { /* ok */ }
      mkdirSync(extractDir, { recursive: true })

      await httpsDownload(source.url, archivePath, (e) => onProgressCallback?.(e))

      // Extrai usando ferramentas do sistema (unzip/tar), sem dependências extras.
      if (source.kind === 'zip') {
        if (process.platform === 'win32') {
          await run('tar', ['-xf', archivePath, '-C', extractDir]) // bsdtar do Windows lê zip
        } else {
          await run('unzip', ['-o', archivePath, '-d', extractDir])
        }
      } else {
        await run('tar', ['-xf', archivePath, '-C', extractDir])
      }

      const extracted = findFfmpegBinary(extractDir)
      if (!extracted) throw new Error('binário ffmpeg não encontrado no pacote baixado')

      const finalPath = managedBinPath(userDataPath)
      try { rmSync(finalPath, { force: true }) } catch { /* ok */ }
      renameSync(extracted, finalPath)
      if (process.platform !== 'win32') chmodSync(finalPath, 0o755)
      try { rmSync(extractDir, { recursive: true, force: true }) } catch { /* ok */ }
      try { unlinkSync(archivePath) } catch { /* ok */ }

      const version = await probeFfmpegVersion(finalPath)
      status = { version, state: 'ready', source: 'managed', path: finalPath }
    } catch (err) {
      status = { version: null, state: 'error', source: 'managed', path: null, error: String(err) }
    }
    return getStatus()
  }

  /**
   * Caminho de ffmpeg que deve ser passado ao yt-dlp (`--ffmpeg-location`).
   * Prioridade: custom → sistema → gerenciado. Vazio quando nada foi resolvido
   * (aí o yt-dlp procura o ffmpeg sozinho no PATH).
   */
  function effectiveBinPath(customBinPath: string): string {
    const custom = customBinPath.trim()
    if (custom) return custom
    if (status.source === 'system' && status.path) return status.path
    const managed = managedBinPath(userDataPath)
    if (existsSync(managed)) return managed
    return ''
  }

  return { getStatus, onProgress, ensureReady, download, effectiveBinPath, detectSystem }
}
