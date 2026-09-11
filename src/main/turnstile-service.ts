import { execFile, spawn, type ChildProcess } from 'child_process'
import { chmodSync, createWriteStream, existsSync, mkdirSync, readFileSync, readdirSync, renameSync, rmSync, statSync, unlinkSync, writeFileSync } from 'fs'
import * as https from 'https'
import { join } from 'path'

export type TurnstileSolverId = 'ezsolver' | 'icemellow' | 'surafelabeje' | 'flaresolverr' | string

export type CaptchaType = 'turnstile' | 'recaptcha2' | 'recaptcha3' | 'hcaptcha' | 'image' | 'unknown'

export interface TurnstileSolverStatus {
  id: TurnstileSolverId
  name: string
  repo: string
  version: string | null
  latestVersion: string | null
  updateAvailable: boolean
  state: 'ready' | 'downloading' | 'error' | 'absent' | 'updating'
  error?: string
  managedPath: string
  venvPath: string
  entryPoint: string
  supportedTypes: CaptchaType[]
}

export interface TurnstileProgressEvent {
  solverId: TurnstileSolverId
  bytesDownloaded: number
  totalBytes: number
  stage: 'download' | 'extract' | 'pip'
}

export interface TurnstileSolveRequest {
  sitekey: string
  pageurl: string
  type?: CaptchaType // default 'turnstile' — universal para qualquer hoster
  proxy?: string // socks5://user:pass@127.0.0.1:9150
  timeoutMs?: number
  solverOrder?: TurnstileSolverId[]
  provider?: string // ex: 'katfile', 'rapidgator' — para logs/telemetria
}

export interface TurnstileSolveResult {
  solverId: TurnstileSolverId
  token: string
  elapsedMs: number
}

interface SolverDescriptor {
  id: TurnstileSolverId
  name: string
  repo: string
  branch: string
  entryPoint: string // relative to solver dir
  serviceEntry?: string // for HTTP service mode
  port: number
  apiType: 'ezsolver' | 'icemellow' | 'surafelabeje' | 'flaresolverr' | string
  requirements: string[]
  healthPath?: string
  supportedTypes?: CaptchaType[]
}

const DESCRIPTORS: Record<string, SolverDescriptor> = {
  ezsolver: {
    id: 'ezsolver',
    name: 'EzSolver',
    repo: 'ismoiloffS/EzSolver',
    branch: 'main',
    entryPoint: 'solver.py',
    serviceEntry: 'service.py',
    port: 8191,
    apiType: 'ezsolver',
    requirements: ['nodriver'],
    healthPath: '/health',
    supportedTypes: ['turnstile'],
  } as SolverDescriptor,
  icemellow: {
    id: 'icemellow',
    name: 'Icemellow V2',
    repo: 'icemellow-me/turnstile-solver',
    branch: 'main',
    entryPoint: 'solver-server.py',
    serviceEntry: 'solver-server-v2.py',
    port: 8878,
    apiType: 'icemellow',
    requirements: ['aiohttp>=3.14', 'nodriver>=0.50', 'camoufox>=0.4', 'playwright>=1.60'],
    healthPath: '/health',
    supportedTypes: ['turnstile'],
  } as SolverDescriptor,
  surafelabeje: {
    id: 'surafelabeje',
    name: 'Surafel Turnstile',
    repo: 'surafelabeje/Turnstilesolver',
    branch: 'main',
    entryPoint: 'api_solver.py',
    serviceEntry: 'api_solver.py',
    port: 5000,
    apiType: 'surafelabeje',
    requirements: ['patchright'],
    healthPath: '/turnstile',
    supportedTypes: ['turnstile'],
  } as SolverDescriptor,
  flaresolverr: {
    id: 'flaresolverr',
    name: 'FlareSolverr',
    repo: 'FlareSolverr/FlareSolverr',
    branch: 'master',
    entryPoint: 'src/flaresolverr.py',
    port: 8191,
    apiType: 'flaresolverr',
    requirements: [],
    healthPath: '/healthcheck',
    supportedTypes: ['turnstile', 'unknown'],
  } as SolverDescriptor,
}

// Manifesto remoto para auto-pull de novos solvers (como yt-dlp auto-update)
// Se o arquivo não existir ou falhar, mantém DESCRIPTORS locais — fallback seguro.
// Quando você adicionar novo hoster, basta adicionar entrada em resources/solver-manifest.json e
// o próximo ensureAllReady baixa automaticamente (mesmo 6h cache do yt-dlp).
const SOLVER_MANIFEST_URL = 'https://raw.githubusercontent.com/Chimarrao/gDownloader/main/resources/solver-manifest.json'
let remoteDescriptors: Record<string, SolverDescriptor> | null = null
let manifestFetchedAt = 0

async function fetchRemoteManifest(): Promise<Record<string, SolverDescriptor> | null> {
  const now = Date.now()
  if (remoteDescriptors && now - manifestFetchedAt < 6 * 60 * 60 * 1000) return remoteDescriptors
  try {
    const resp = await httpsGet(SOLVER_MANIFEST_URL)
    if (resp.statusCode !== 200) return null
    const data = JSON.parse(resp.body) as { solvers?: SolverDescriptor[] }
    if (!Array.isArray(data.solvers)) return null
    const merged: Record<string, SolverDescriptor> = { ...DESCRIPTORS }
    for (const s of data.solvers) {
      if (s.id && s.repo) merged[s.id] = s
    }
    remoteDescriptors = merged
    manifestFetchedAt = now
    return merged
  } catch {
    return null
  }
}

function allDescriptors(): Record<string, SolverDescriptor> {
  return remoteDescriptors || DESCRIPTORS
}

function managedRoot(userDataPath: string): string {
  return join(userDataPath, 'turnstile')
}

function solverDir(userDataPath: string, id: TurnstileSolverId): string {
  return join(managedRoot(userDataPath), id)
}

function venvPath(userDataPath: string, id: TurnstileSolverId): string {
  return join(solverDir(userDataPath, id), 'venv')
}

function venvPython(userDataPath: string, id: TurnstileSolverId): string {
  const base = venvPath(userDataPath, id)
  return process.platform === 'win32' ? join(base, 'Scripts', 'python.exe') : join(base, 'bin', 'python')
}

function codePath(userDataPath: string, id: TurnstileSolverId): string {
  return join(solverDir(userDataPath, id), 'code')
}

function versionPath(userDataPath: string, id: TurnstileSolverId): string {
  return join(solverDir(userDataPath, id), '.version')
}

function managedBinPath(userDataPath: string, id: TurnstileSolverId): string {
  const desc = allDescriptors()[id] || DESCRIPTORS[id]
  if (!desc) return join(codePath(userDataPath, id), 'unknown')
  return join(codePath(userDataPath, id), desc.serviceEntry || desc.entryPoint)
}

function httpsGet(url: string): Promise<{ statusCode: number; body: string; headers: Record<string, string> }> {
  return new Promise((resolve, reject) => {
    https
      .get(url, { headers: { 'User-Agent': 'gDownloader', Accept: 'application/vnd.github.v3+json' } }, (res) => {
        if (res.statusCode === 301 || res.statusCode === 302 || res.statusCode === 307 || res.statusCode === 308) {
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
        res.on('end', () => resolve({ statusCode: res.statusCode ?? 0, body, headers: res.headers as Record<string, string> }))
        res.on('error', reject)
      })
      .on('error', reject)
  })
}

function httpsDownload(url: string, destPath: string, onProgress: (e: { bytesDownloaded: number; totalBytes: number }) => void): Promise<void> {
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
            reject(new Error(`HTTP ${res.statusCode ?? 'unknown'} ao baixar ${redirectUrl}`))
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
            try { unlinkSync(destPath) } catch { /* ignore */ }
            reject(err)
          })
          res.on('error', reject)
        })
        .on('error', reject)
    }
    follow(url)
  })
}

// EzSolver roda com uma janela do Chrome visível (headless=False, pedido do
// usuário para debug) presa a um profile FIXO (/tmp/ts_profile, ou
// TS_PROFILE_DIR). O solver.py de terceiros faz `browser = await uc.start(...)`
// FORA de qualquer try/finally: se essa chamada falhar no meio (perda de
// conexão CDP, disputa pelo lock do profile, timeout), o processo Chrome que
// ela já tinha aberto fica órfão — sem ninguém chamando browser.stop(). Como
// o profile é sempre o mesmo, a próxima tentativa não abre uma janela nova:
// o Chrome já rodando (zumbi) recebe a URL como aba nova via IPC do próprio
// Chrome. Depois de muitas tentativas (ex.: madrugada inteira com filas de
// Katfile se revalidando), isso vira uma janela com 100+ abas. Isso é um bug
// no código de terceiros que não controlamos (e que pode ser sobrescrito por
// auto-update); a defesa fica aqui: mata qualquer Chrome preso a esse profile
// antes e depois de cada tentativa do EzSolver, garantindo no máximo 1 janela
// viva por vez.
const EZSOLVER_PROFILE_MARKER = 'ts_profile'
function reapStrayEzsolverChrome(): void {
  if (process.platform === 'win32') return
  try {
    require('child_process').execSync(`pkill -f "${EZSOLVER_PROFILE_MARKER}"`, { stdio: 'ignore' })
  } catch {
    // pkill sai com código != 0 quando não há processo pra matar — esperado na maioria das vezes
  }
}

function findPython(): string {
  // Prefer python3.12 as proven working for nodriver, fallback to python3
  const candidates = ['python3.12', 'python3', 'python']
  for (const cmd of candidates) {
    try {
      const out = require('child_process').execSync(`${cmd} --version`, { encoding: 'utf8', timeout: 3000 })
      if (out.includes('Python 3.')) return cmd
    } catch { /* try next */ }
  }
  return 'python3'
}

function run(cmd: string, args: string[], timeoutMs = 120_000): Promise<{ stdout: string; stderr: string }> {
  return new Promise((resolve, reject) => {
    execFile(cmd, args, { timeout: timeoutMs }, (err, stdout, stderr) => {
      if (err) reject(err)
      else resolve({ stdout: stdout.toString(), stderr: stderr.toString() })
    })
  })
}

async function fetchLatestVersion(id: TurnstileSolverId): Promise<string | null> {
  const desc = allDescriptors()[id] || DESCRIPTORS[id]
  if (!desc) return null
  try {
    // Use commits API for branch HEAD sha as version (works even without releases)
    const resp = await httpsGet(`https://api.github.com/repos/${desc.repo}/commits/${desc.branch}`)
    if (resp.statusCode !== 200) return null
    const data = JSON.parse(resp.body) as { sha?: string }
    return data.sha?.slice(0, 7) ?? null
  } catch {
    return null
  }
}

async function downloadAndExtract(id: TurnstileSolverId, userDataPath: string, onProgress: (e: TurnstileProgressEvent) => void): Promise<string> {
  const desc = allDescriptors()[id] || DESCRIPTORS[id]
  if (!desc) throw new Error(`Solver desconhecido: ${id}`)
  const dir = solverDir(userDataPath, id)
  mkdirSync(dir, { recursive: true })
  const zipPath = join(dir, `${id}.zip`)
  // GitHub archive zip
  const url = `https://github.com/${desc.repo}/archive/refs/heads/${desc.branch}.zip`
  await httpsDownload(url, zipPath, (e) => onProgress({ solverId: id, bytesDownloaded: e.bytesDownloaded, totalBytes: e.totalBytes, stage: 'download' }))
  // Extract via unzip/tar
  const extractTmp = join(dir, 'extract_tmp')
  try { rmSync(extractTmp, { recursive: true, force: true }) } catch {}
  mkdirSync(extractTmp, { recursive: true })
  onProgress({ solverId: id, bytesDownloaded: 0, totalBytes: 0, stage: 'extract' })
  if (process.platform === 'win32') {
    await run('tar', ['-xf', zipPath, '-C', extractTmp])
  } else {
    // Try unzip first, fallback to tar
    try {
      await run('unzip', ['-o', zipPath, '-d', extractTmp])
    } catch {
      await run('tar', ['-xf', zipPath, '-C', extractTmp])
    }
  }
  // Find extracted code dir (single top-level)
  const entries = readdirSync(extractTmp)
  const top = entries.find((e) => {
    try { return statSync(join(extractTmp, e)).isDirectory() } catch { return false }
  })
  if (!top) throw new Error('Arquitetura do zip inesperada')
  const src = join(extractTmp, top)
  const dest = codePath(userDataPath, id)
  try { rmSync(dest, { recursive: true, force: true }) } catch {}
  renameSync(src, dest)
  try { unlinkSync(zipPath) } catch {}
  try { rmSync(extractTmp, { recursive: true, force: true }) } catch {}
  // Fetch version sha
  const version = (await fetchLatestVersion(id)) || new Date().toISOString().slice(0, 10)
  writeFileSync(versionPath(userDataPath, id), version, 'utf8')
  return version
}

async function setupVenv(id: TurnstileSolverId, userDataPath: string, onProgress: (e: TurnstileProgressEvent) => void): Promise<void> {
  const desc = allDescriptors()[id] || DESCRIPTORS[id]
  if (!desc || desc.requirements.length === 0) return
  const venv = venvPath(userDataPath, id)
  const python = venvPython(userDataPath, id)
  if (!existsSync(python)) {
    mkdirSync(venv, { recursive: true })
    const sysPython = findPython()
    onProgress({ solverId: id, bytesDownloaded: 0, totalBytes: 0, stage: 'pip' })
    await run(sysPython, ['-m', 'venv', venv])
  }
  // pip install
  if (desc.requirements.length > 0) {
    onProgress({ solverId: id, bytesDownloaded: 0, totalBytes: 0, stage: 'pip' })
    await run(python, ['-m', 'pip', 'install', '--upgrade', 'pip'])
    await run(python, ['-m', 'pip', 'install', ...desc.requirements])
    // Special: for icemellow need playwright + camoufox fetch, for surafelabeje need patchright
    if (id === 'icemellow') {
      try { await run(python, ['-m', 'playwright', 'install', 'chromium']) } catch {}
      try { await run(python, ['-m', 'camoufox', 'fetch']) } catch {}
    }
    if (id === 'surafelabeje' || id === 'icemellow') {
      try { await run(python, ['-m', 'patchright', 'install', 'chromium']) } catch {}
    }
  }
}

export function createTurnstileService(userDataPath: string) {
  // Limpa qualquer janela zumbi de uma sessão anterior (ex.: app fechado/crashado
  // no meio de uma tentativa do EzSolver) antes de começar a usar o solver de novo.
  reapStrayEzsolverChrome()
  const statuses = new Map<TurnstileSolverId, TurnstileSolverStatus>()
  const processes = new Map<TurnstileSolverId, ChildProcess>()
  let onProgressCallback: ((e: TurnstileProgressEvent) => void) | null = null
  const activeEnsures = new Map<TurnstileSolverId, Promise<void>>()
  // Fila global: alguns solvers (ezsolver) abrem um Chrome real de verdade por
  // solve() e nao toleram chamadas concorrentes (o profile do Chrome fica
  // travado, e cada tentativa concorrente abre outra janela que nunca carrega
  // — so fica em branco, "Nova guia"). Encadeando aqui, nunca roda mais de um
  // solve() ao mesmo tempo, nao importa quantos jobs do Katfile (ou outro
  // provider) peçam ao mesmo tempo — eles esperam a vez na fila.
  let solveQueueTail: Promise<unknown> = Promise.resolve()

  // Inicializa com descriptors locais; manifesto remoto mescla depois (auto-pull)
  for (const id of Object.keys(allDescriptors()) as TurnstileSolverId[]) {
    const desc = allDescriptors()[id]
    statuses.set(id, {
      id,
      name: desc.name,
      repo: desc.repo,
      version: null,
      latestVersion: null,
      updateAvailable: false,
      state: 'absent',
      managedPath: managedBinPath(userDataPath, id),
      venvPath: venvPath(userDataPath, id),
      entryPoint: managedBinPath(userDataPath, id),
      supportedTypes: (desc.supportedTypes as CaptchaType[]) || ['turnstile'],
    })
  }

  function getStatus(id: TurnstileSolverId): TurnstileSolverStatus {
    const s = statuses.get(id)!
    // Refresh version from disk
    try {
      if (existsSync(versionPath(userDataPath, id))) {
        s.version = readFileSync(versionPath(userDataPath, id), 'utf8').trim() || null
      }
      if (existsSync(managedBinPath(userDataPath, id))) {
        if (s.state === 'absent') s.state = 'ready'
      } else if (s.version) {
        s.state = 'absent'
      }
    } catch {}
    return { ...s }
  }

  function getAllStatuses(): TurnstileSolverStatus[] {
    // Tenta mesclar manifesto remoto sem bloquear (auto-pull de novos solvers como yt-dlp)
    void fetchRemoteManifest().then((merged) => {
      if (merged) {
        for (const id of Object.keys(merged)) {
          if (!statuses.has(id as TurnstileSolverId)) {
            const desc = merged[id]
            statuses.set(id as TurnstileSolverId, {
              id: id as TurnstileSolverId,
              name: desc.name,
              repo: desc.repo,
              version: null,
              latestVersion: null,
              updateAvailable: false,
              state: 'absent',
              managedPath: managedBinPath(userDataPath, id as TurnstileSolverId),
              venvPath: venvPath(userDataPath, id as TurnstileSolverId),
              entryPoint: managedBinPath(userDataPath, id as TurnstileSolverId),
              supportedTypes: desc.supportedTypes || ['turnstile'],
            })
          }
        }
      }
    }).catch(() => null)
    return (Object.keys(allDescriptors()) as TurnstileSolverId[]).map((id) => getStatus(id))
  }

  function getDescriptor(id: TurnstileSolverId): SolverDescriptor | undefined {
    return allDescriptors()[id] || DESCRIPTORS[id]
  }
  void getDescriptor

  function onProgress(cb: (e: TurnstileProgressEvent) => void): void {
    onProgressCallback = cb
  }

  async function ensureReady(id: TurnstileSolverId, autoUpdate = true): Promise<void> {
    if (activeEnsures.has(id)) return activeEnsures.get(id)!
    const p = _doEnsureReady(id, autoUpdate)
    activeEnsures.set(id, p)
    try { await p } finally { activeEnsures.delete(id) }
    return
  }

  async function _doEnsureReady(id: TurnstileSolverId, autoUpdate: boolean): Promise<void> {
    void DESCRIPTORS[id]
    const st = statuses.get(id)!
    const codeExists = existsSync(managedBinPath(userDataPath, id))
    const versionExists = existsSync(versionPath(userDataPath, id))

    if (!codeExists || !versionExists) {
      st.state = 'downloading'
      try {
        const version = await downloadAndExtract(id, userDataPath, (e) => onProgressCallback?.(e))
        await setupVenv(id, userDataPath, (e) => onProgressCallback?.(e))
        st.version = version
        st.state = 'ready'
        st.error = undefined
        if (process.platform !== 'win32') {
          try { chmodSync(managedBinPath(userDataPath, id), 0o755) } catch {}
        }
      } catch (err) {
        st.state = 'error'
        st.error = String(err)
        throw err
      }
      return
    }

    // Already installed
    try {
      st.version = readFileSync(versionPath(userDataPath, id), 'utf8').trim() || null
    } catch {}
    st.state = 'ready'

    if (!autoUpdate) return

    // Check for update in background (non-blocking for caller, but we do it here)
    const latest = await fetchLatestVersion(id)
    if (latest && st.version && latest !== st.version) {
      st.latestVersion = latest
      st.updateAvailable = true
      // Auto-download in background if autoUpdate
      try {
        st.state = 'updating'
        const version = await downloadAndExtract(id, userDataPath, (e) => onProgressCallback?.(e))
        await setupVenv(id, userDataPath, (e) => onProgressCallback?.(e))
        st.version = version
        st.latestVersion = null
        st.updateAvailable = false
        st.state = 'ready'
      } catch (err) {
        // Keep current version, just log
        st.state = 'ready'
        st.error = String(err)
      }
    }
  }

  async function ensureAllReady(autoUpdate = true): Promise<void> {
    // Auto-pull de novos solvers via manifest (como yt-dlp) — se falhar, usa locais
    const descs = (await fetchRemoteManifest().catch(() => null)) || allDescriptors()
    // Garante que novos do manifest entrem no mapa
    for (const id of Object.keys(descs) as TurnstileSolverId[]) {
      if (!statuses.has(id)) {
        const d = descs[id]
        statuses.set(id, {
          id,
          name: d.name,
          repo: d.repo,
          version: null,
          latestVersion: null,
          updateAvailable: false,
          state: 'absent',
          managedPath: managedBinPath(userDataPath, id),
          venvPath: venvPath(userDataPath, id),
          entryPoint: managedBinPath(userDataPath, id),
          supportedTypes: (d.supportedTypes as CaptchaType[]) || ['turnstile'],
        })
      }
    }
    for (const id of Object.keys(descs) as TurnstileSolverId[]) {
      await ensureReady(id, autoUpdate).catch(() => null)
    }
  }

  async function updateSolver(id: TurnstileSolverId): Promise<TurnstileSolverStatus> {
    const st = statuses.get(id)!
    st.state = 'downloading'
    try {
      const version = await downloadAndExtract(id, userDataPath, (e) => onProgressCallback?.(e))
      await setupVenv(id, userDataPath, (e) => onProgressCallback?.(e))
      st.version = version
      st.state = 'ready'
      st.updateAvailable = false
      st.error = undefined
    } catch (err) {
      st.state = 'error'
      st.error = String(err)
    }
    return getStatus(id)
  }

  async function checkUpdate(id: TurnstileSolverId): Promise<TurnstileSolverStatus> {
    const st = statuses.get(id)!
    const latest = await fetchLatestVersion(id)
    st.latestVersion = latest
    st.updateAvailable = Boolean(latest && st.version && latest !== st.version)
    return getStatus(id)
  }

  function getSolverProcess(id: TurnstileSolverId): ChildProcess | undefined {
    return processes.get(id)
  }
  void getSolverProcess

  async function startSolver(id: TurnstileSolverId, extraArgs: string[] = []): Promise<ChildProcess> {
    const desc = allDescriptors()[id] || DESCRIPTORS[id]
    void desc
    const existing = processes.get(id)
    if (existing && !existing.killed) return existing

    await ensureReady(id, false)

    const python = venvPython(userDataPath, id)
    const entry = managedBinPath(userDataPath, id)
    if (!existsSync(entry)) throw new Error(`Solver ${id} não instalado em ${entry}`)
    const pythonExists = existsSync(python)
    const cmd = pythonExists ? python : findPython()
    const args = pythonExists ? [entry, ...extraArgs] : [entry, ...extraArgs]

    // For ezsolver surafelabeje we need to set CHROME_PATH on macOS
    const env = { ...process.env } as Record<string, string>
    if (process.platform === 'darwin') {
      env.CHROME_PATH = env.CHROME_PATH || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome'
    }
    if (id === 'ezsolver') {
      // gDownloader só manda 1 solve por vez (fila serializada acima); o default
      // do serviço (4 workers concorrentes) só multiplica o risco de disputa pelo
      // profile fixo do Chrome. Trava em 1 pra bater com o uso real.
      env.MAX_WORKERS = '1'
    }

    const child = spawn(cmd, args, {
      cwd: codePath(userDataPath, id),
      env,
      stdio: ['ignore', 'pipe', 'pipe'],
    })
    processes.set(id, child)
    child.on('exit', () => processes.delete(id))
    // Wait a bit for port to be ready
    await new Promise((r) => setTimeout(r, 1500))
    return child
  }

  async function stopSolver(id: TurnstileSolverId): Promise<void> {
    const proc = processes.get(id)
    if (proc && !proc.killed) {
      proc.kill()
      processes.delete(id)
    }
  }

  async function stopAll(): Promise<void> {
    for (const id of Object.keys(DESCRIPTORS) as TurnstileSolverId[]) {
      await stopSolver(id)
    }
  }

  // Universal solve — funciona para qualquer hoster (katfile, rapidgator, etc.)
  // Filtra por tipo suportado e tenta em ordem (auto-pull de novos solvers via manifest)
  async function solve(request: TurnstileSolveRequest): Promise<TurnstileSolveResult> {
    const run = solveQueueTail.then(() => solveInternal(request), () => solveInternal(request))
    solveQueueTail = run.catch(() => undefined)
    return run
  }

  async function solveInternal(request: TurnstileSolveRequest): Promise<TurnstileSolveResult> {
    const captchaType = request.type || 'turnstile'
    void (request.provider || 'unknown')
    // Se vier ordem custom, usa; senão monta ordem preferindo solvers que suportam o tipo
    const baseOrder = request.solverOrder || (['icemellow', 'ezsolver', 'surafelabeje', 'flaresolverr'] as TurnstileSolverId[])
    // Auto-pull: tenta atualizar manifest sem bloquear (como yt-dlp)
    void fetchRemoteManifest().catch(() => null)
    const descs = allDescriptors()
    const order = baseOrder.filter((id) => {
      const d = descs[id]
      if (!d) return false
      const supported = (d.supportedTypes as CaptchaType[]) || ['turnstile']
      // 'unknown' aceita qualquer solver como fallback
      if (captchaType === 'unknown') return true
      return supported.includes(captchaType) || supported.includes('unknown')
    })
    // Se filtro esvaziar, tenta todos
    const finalOrder = order.length > 0 ? order : baseOrder
    let lastError: unknown = null
    for (const solverId of finalOrder) {
      try {
        const desc = descs[solverId] || DESCRIPTORS[solverId]
        if (!desc) continue
        // Antes de qualquer tentativa nova do EzSolver, limpa uma janela zumbi
        // deixada por uma tentativa anterior que falhou no meio — senão a
        // próxima solicitação vira só mais uma aba na janela presa.
        if (solverId === 'ezsolver') reapStrayEzsolverChrome()
        // Ensure solver is ready (download if missing, but not auto-update during solve)
        await ensureReady(solverId, false)
        // Start solver HTTP service if not running
        const port = desc.port
        // Try health first
        const healthUrl = `http://127.0.0.1:${port}${desc.healthPath || '/health'}`
        let healthy = false
        try {
          await new Promise<{ statusCode: number }>((resolve, reject) => {
            https.get(healthUrl, (res) => resolve({ statusCode: res.statusCode ?? 0 })).on('error', reject)
          })
        } catch {}
        // Use http via require
        const http = require('http') as typeof import('http')
        const isHealthy = await new Promise<boolean>((resolve) => {
          const req = http.get(healthUrl.replace('https', 'http'), (res) => resolve((res.statusCode ?? 0) < 500))
          req.on('error', () => resolve(false))
          req.setTimeout(2000, () => { req.destroy(); resolve(false) })
        })
        if (!isHealthy) {
          // Start solver
          if (solverId === 'ezsolver') {
            await startSolver(solverId, [])
            // EzSolver service.py defaults to 8191, no args
          } else if (solverId === 'icemellow') {
            await startSolver(solverId, ['--api-key', 'gdownloader', '--port', String(port), '--max-sessions', '1'])
          } else if (solverId === 'surafelabeje') {
            await startSolver(solverId, ['--port', String(port)])
          } else if (solverId === 'flaresolverr') {
            // FlareSolverr via docker or local - not auto-started here, fallback to error
            throw new Error('FlareSolverr requer Docker, use modo manual')
          }
          // Wait for health
          for (let i = 0; i < 10; i++) {
            const ok = await new Promise<boolean>((resolve) => {
              const req = http.get(healthUrl.replace('https', 'http'), (res) => resolve((res.statusCode ?? 0) === 200))
              req.on('error', () => resolve(false))
              req.setTimeout(1000, () => { req.destroy(); resolve(false) })
            })
            if (ok) { healthy = true; break }
            await new Promise((r) => setTimeout(r, 1000))
          }
        } else {
          healthy = true
        }
        if (!healthy) throw new Error(`Solver ${solverId} não respondeu`)

        // Now solve via solver-specific API
        const start = Date.now()
        let token: string | null = null
        if (desc.apiType === 'ezsolver') {
          // POST http://127.0.0.1:8191/solve {sitekey, siteurl}
          token = await new Promise<string | null>((resolve, reject) => {
            const payload = JSON.stringify({ sitekey: request.sitekey, siteurl: request.pageurl, timeout: Math.floor((request.timeoutMs ?? 45000) / 1000) })
            const req = http.request(
              {
                hostname: '127.0.0.1',
                port,
                path: '/solve',
                method: 'POST',
                headers: { 'Content-Type': 'application/json', 'Content-Length': Buffer.byteLength(payload) },
              },
              (res) => {
                let body = ''
                res.on('data', (c) => (body += c))
                res.on('end', () => {
                  try {
                    const j = JSON.parse(body)
                    resolve(j.token || null)
                  } catch { reject(new Error(body)) }
                })
              },
            )
            req.on('error', reject)
            req.setTimeout(request.timeoutMs ?? 45000, () => { req.destroy(); reject(new Error('timeout')) })
            req.write(payload)
            req.end()
          })
        } else if (desc.apiType === 'icemellow') {
          // 2captcha compat: POST /in.php then poll /res.php
          const httpMod = require('http') as typeof import('http')
          const qs = require('querystring') as typeof import('querystring')
          const postData = qs.stringify({ key: 'gdownloader', method: 'turnstile', sitekey: request.sitekey, pageurl: request.pageurl, json: '1' })
          const taskId: string = await new Promise((resolve, reject) => {
            const req = httpMod.request(
              {
                hostname: '127.0.0.1',
                port,
                path: '/in.php',
                method: 'POST',
                headers: { 'Content-Type': 'application/x-www-form-urlencoded', 'Content-Length': Buffer.byteLength(postData) },
              },
              (res) => {
                let body = ''
                res.on('data', (c) => (body += c))
                res.on('end', () => {
                  try {
                    const j = JSON.parse(body)
                    if (j.status === 1) resolve(j.request)
                    else reject(new Error(body))
                  } catch { reject(new Error(body)) }
                })
              },
            )
            req.on('error', reject)
            req.write(postData)
            req.end()
          })
          // Poll
          token = await new Promise<string | null>((resolve, reject) => {
            let attempts = 0
            const maxAttempts = Math.floor((request.timeoutMs ?? 45000) / 2000)
            const poll = () => {
              const req = httpMod.get(`http://127.0.0.1:${port}/res.php?key=gdownloader&id=${taskId}&json=1`, (res) => {
                let body = ''
                res.on('data', (c) => (body += c))
                res.on('end', () => {
                  try {
                    const j = JSON.parse(body)
                    if (j.status === 1 && j.request && !j.request.includes('CAPCHA_NOT_READY')) {
                      resolve(j.request)
                    } else if (j.request === 'CAPCHA_NOT_READY' && attempts < maxAttempts) {
                      attempts++
                      setTimeout(poll, 2000)
                    } else if (attempts >= maxAttempts) {
                      reject(new Error('timeout'))
                    } else {
                      attempts++
                      setTimeout(poll, 2000)
                    }
                  } catch { reject(new Error(body)) }
                })
              })
              req.on('error', reject)
            }
            poll()
          })
        } else if (desc.apiType === 'surafelabeje') {
          // GET /turnstile?url=...&sitekey=...
          const url = `http://127.0.0.1:${port}/turnstile?url=${encodeURIComponent(request.pageurl)}&sitekey=${encodeURIComponent(request.sitekey)}`
          const taskId: string = await new Promise((resolve, reject) => {
            http.get(url, (res) => {
              let body = ''
              res.on('data', (c) => (body += c))
              res.on('end', () => {
                try {
                  const j = JSON.parse(body)
                  resolve(j.task_id || j.taskId || '')
                } catch { reject(new Error(body)) }
              })
            }).on('error', reject)
          })
          // Poll /result?id=
          token = await new Promise<string | null>((resolve, reject) => {
            let attempts = 0
            const maxAttempts = Math.floor((request.timeoutMs ?? 45000) / 2000)
            const poll = () => {
              http.get(`http://127.0.0.1:${port}/result?id=${taskId}`, (res) => {
                let body = ''
                res.on('data', (c) => (body += c))
                res.on('end', () => {
                  try {
                    const j = JSON.parse(body)
                    if (j.value && j.value !== 'CAPTCHA_FAIL') resolve(j.value)
                    else if (attempts < maxAttempts) { attempts++; setTimeout(poll, 2000) }
                    else reject(new Error(body))
                  } catch { reject(new Error(body)) }
                })
              }).on('error', reject)
            }
            poll()
          })
        } else {
          throw new Error(`API type ${desc.apiType} não implementado`)
        }

        if (token && token.length >= 20) {
          return { solverId, token, elapsedMs: Date.now() - start }
        }
        throw new Error(`Solver ${solverId} retornou token inválido`)
      } catch (err) {
        lastError = err
        // Try next solver in chain
        continue
      } finally {
        // Fecha a janela do EzSolver assim que a tentativa termina (sucesso ou
        // falha) em vez de confiar só no `browser.stop()` de terceiros.
        if (solverId === 'ezsolver') reapStrayEzsolverChrome()
      }
    }
    throw lastError || new Error('Todos os solvers falharam')
  }

  return {
    getStatus,
    getAllStatuses,
    onProgress,
    ensureReady,
    ensureAllReady,
    updateSolver,
    checkUpdate,
    startSolver,
    stopSolver,
    stopAll,
    solve,
    descriptors: DESCRIPTORS,
  }
}
