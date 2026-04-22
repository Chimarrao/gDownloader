import { randomUUID } from 'crypto'
import { existsSync, mkdirSync, rmSync } from 'fs'
import { dirname } from 'path'

import { BrowserWindow, session } from 'electron'
import {
  createExclusiveRunner,
  delay,
  HOSTER_BROWSER_USER_AGENT,
  looksLikeFilename,
  parseHumanSize,
  sanitizeFilename,
} from './browser-helper-common'

const AKIRABOX_PARTITION = 'persist:akirabox'
const CHALLENGE_HINTS = [
  'just a moment',
  'um momento',
  'performing security verification',
  'executando verificação de segurança',
  'verifica se você não é um bot',
  'enable javascript and cookies to continue',
  '/cdn-cgi/challenge-platform/',
  'window._cf_chl_opt',
]

interface AkiraboxPageSnapshot {
  url: string
  title: string
  bodyText: string
  isChallenge: boolean
  filenameCandidates: string[]
  sizeCandidates: string[]
}

interface AkiraboxDownloadJob {
  id: string
  sourceUrl: string
  destPath: string
  status: 'pending' | 'downloading' | 'complete' | 'error' | 'cancelled'
  bytesDownloaded: number
  totalBytes: number
  speedBps: number
  etaSecs: number
  filename?: string
  error?: string
  startResolve?: () => void
  startReject?: (error: Error) => void
  startTimeout?: ReturnType<typeof setTimeout>
  showTimeout?: ReturnType<typeof setTimeout>
  driveInterval?: ReturnType<typeof setInterval>
  lastBytes: number
  lastTickAt: number
  lastProgressAt: number
  smoothedSpeedBps: number
}

function fallbackNameFromUrl(url: string): string {
  try {
    const parsed = new URL(url)
    const last = parsed.pathname.split('/').filter(Boolean).at(-2) || parsed.hostname
    return sanitizeFilename(last, 'arquivo_akirabox')
  } catch {
    return 'arquivo_akirabox'
  }
}

export function createAkiraboxService() {
  const jobs = new Map<string, AkiraboxDownloadJob>()
  let helperWindow: BrowserWindow | null = null
  let sessionWired = false
  let pendingDownloadJobId: string | null = null
  const runExclusive = createExclusiveRunner()

  function getSession() {
    return session.fromPartition(AKIRABOX_PARTITION)
  }

  function getWindow(): BrowserWindow {
    if (helperWindow && !helperWindow.isDestroyed()) {
      return helperWindow
    }

    helperWindow = new BrowserWindow({
      show: false,
      width: 1280,
      height: 900,
      autoHideMenuBar: true,
      title: 'AkiraBox',
      webPreferences: {
        partition: AKIRABOX_PARTITION,
        contextIsolation: true,
        sandbox: false,
        backgroundThrottling: false,
      },
    })
    helperWindow.webContents.setUserAgent(HOSTER_BROWSER_USER_AGENT)
    helperWindow.on('closed', () => {
      helperWindow = null
    })
    return helperWindow
  }

  async function readPageSnapshot(): Promise<AkiraboxPageSnapshot> {
    const win = getWindow()
    return (await win.webContents.executeJavaScript(
      `(() => {
        const bodyText = (document.body?.innerText || '').replace(/\\u00a0/g, ' ').trim()
        const title = (document.title || '').trim()
        const htmlHead = document.documentElement.outerHTML.slice(0, 16000)
        const lowerBlob = (title + '\\n' + bodyText + '\\n' + htmlHead).toLowerCase()
        const challengeHints = ${JSON.stringify(CHALLENGE_HINTS)}
        const isChallenge =
          location.pathname.toLowerCase().startsWith('/cdn-cgi/challenge-platform/')
          || challengeHints.some((hint) => lowerBlob.includes(hint))

        const filenameCandidates = []
        const seenNames = new Set()
        const pushName = (value) => {
          const text = String(value || '').replace(/\\s+/g, ' ').trim()
          if (!text || seenNames.has(text)) return
          seenNames.add(text)
          filenameCandidates.push(text)
        }

        pushName(document.querySelector('meta[property="og:title"]')?.content)
        pushName(document.querySelector('meta[name="title"]')?.content)
        pushName(title)

        const nameNodes = Array.from(
          document.querySelectorAll(
            'h1, h2, h3, [download], [data-filename], .file-name, .filename, .download-title, [class*="file-name"], [class*="filename"], [class*="download-title"]'
          )
        )
        for (const node of nameNodes.slice(0, 80)) {
          pushName(node.getAttribute('download'))
          pushName(node.getAttribute('data-filename'))
          pushName(node.textContent)
          pushName(node.getAttribute('title'))
        }

        for (const line of bodyText.split(/\\n+/).map((item) => item.trim()).filter(Boolean).slice(0, 160)) {
          if (line.length <= 220) {
            pushName(line)
          }
        }

        const sizeCandidates = []
        const seenSizes = new Set()
        const pushSize = (value) => {
          const text = String(value || '').replace(/\\s+/g, ' ').trim()
          if (!text || seenSizes.has(text)) return
          seenSizes.add(text)
          sizeCandidates.push(text)
        }

        const sizeRegex = /\\b[0-9]+(?:[.,][0-9]+)?\\s*(KB|MB|GB|TB)\\b/i
        for (const line of bodyText.split(/\\n+/).map((item) => item.trim()).filter(Boolean).slice(0, 200)) {
          if (sizeRegex.test(line)) {
            pushSize(line)
          }
        }

        return {
          url: location.href,
          title,
          bodyText,
          isChallenge,
          filenameCandidates,
          sizeCandidates,
        }
      })()`,
      true
    )) as AkiraboxPageSnapshot
  }

  async function hasClearanceCookie(url: string): Promise<boolean> {
    try {
      const cookies = await getSession().cookies.get({ url })
      return cookies.some((cookie) => cookie.name === 'cf_clearance' && Boolean(cookie.value))
    } catch {
      return false
    }
  }

  async function waitForResolvedPage(sourceUrl: string, timeoutMs: number): Promise<AkiraboxPageSnapshot | null> {
    const deadline = Date.now() + timeoutMs
    let forcedReloadAfterClearance = false
    while (Date.now() < deadline) {
      const currentUrl = getWindow().webContents.getURL()
      if (!currentUrl || currentUrl === 'about:blank') {
        await delay(400)
        continue
      }

      try {
        const snapshot = await readPageSnapshot()
        if (!snapshot.isChallenge) {
          return snapshot
        }

        if (!forcedReloadAfterClearance && (await hasClearanceCookie(sourceUrl))) {
          forcedReloadAfterClearance = true
          await getWindow().loadURL(sourceUrl).catch(() => undefined)
          await delay(1200)
          continue
        }
      } catch {
        // keep polling while the page is still changing
      }

      await delay(800)
    }
    return null
  }

  async function ensureReadyPage(url: string): Promise<AkiraboxPageSnapshot> {
    const win = getWindow()
    await win.loadURL(url)

    let snapshot = await waitForResolvedPage(url, 12_000)
    if (snapshot) {
      if (win.isVisible()) {
        win.hide()
      }
      return snapshot
    }

    win.setTitle('AkiraBox - conclua a verificação de segurança')
    win.show()
    win.focus()

    snapshot = await waitForResolvedPage(url, 180_000)
    if (!snapshot) {
      throw new Error(
        'O AkiraBox exige concluir a verificação de segurança no navegador integrado antes de continuar.'
      )
    }

    win.hide()
    return snapshot
  }

  function chooseFilename(snapshot: AkiraboxPageSnapshot, url: string): string {
    const candidates = snapshot.filenameCandidates
      .map((value) => value.replace(/\s+/g, ' ').trim())
      .filter((value) => value && !CHALLENGE_HINTS.some((hint) => value.toLowerCase().includes(hint)))

    const withExtension = candidates.find(looksLikeFilename)
    if (withExtension) {
      return sanitizeFilename(withExtension, fallbackNameFromUrl(url))
    }

    const cleaner = candidates.find((value) => value.length >= 4 && value.length <= 160)
    if (cleaner) {
      return sanitizeFilename(cleaner, fallbackNameFromUrl(url))
    }

    return fallbackNameFromUrl(url)
  }

  function chooseSize(snapshot: AkiraboxPageSnapshot): number {
    for (const candidate of snapshot.sizeCandidates) {
      const parsed = parseHumanSize(candidate)
      if (parsed > 0) {
        return parsed
      }
    }
    return parseHumanSize(snapshot.bodyText)
  }

  async function getFileInfo(url: string): Promise<{
    filename: string
    size: number
    mime_type: null
    is_folder: false
    children: null
  }> {
    return runExclusive(async () => {
      const snapshot = await ensureReadyPage(url)
      return {
        filename: chooseFilename(snapshot, url),
        size: chooseSize(snapshot),
        mime_type: null,
        is_folder: false,
        children: null,
      }
    })
  }

  function wireDownloadSession(): void {
    if (sessionWired) {
      return
    }
    sessionWired = true

    getSession().on('will-download', (_event, item) => {
      const jobId = pendingDownloadJobId
      pendingDownloadJobId = null

      if (!jobId) {
        item.cancel()
        return
      }

      const job = jobs.get(jobId)
      if (!job) {
        item.cancel()
        return
      }

      clearJobTimers(job)

      if (existsSync(job.destPath)) {
        rmSync(job.destPath, { force: true })
      }
      mkdirSync(dirname(job.destPath), { recursive: true })
      item.setSavePath(job.destPath)

      job.status = 'downloading'
      job.filename = item.getFilename() || job.filename
      job.totalBytes = item.getTotalBytes() > 0 ? item.getTotalBytes() : job.totalBytes
      job.lastBytes = 0
      job.lastTickAt = Date.now()
      job.lastProgressAt = Date.now()
      job.smoothedSpeedBps = 0
      job.startResolve?.()
      job.startResolve = undefined
      job.startReject = undefined

      if (helperWindow && !helperWindow.isDestroyed() && helperWindow.isVisible()) {
        helperWindow.hide()
      }

      item.on('updated', (_ev, state) => {
        if (state === 'interrupted' && item.canResume()) {
          try {
            item.resume()
            return
          } catch {
            // fall through to regular bookkeeping below
          }
        }

        const now = Date.now()
        const bytesDownloaded = item.getReceivedBytes()
        const totalBytes = item.getTotalBytes()
        if (totalBytes > 0) {
          job.totalBytes = totalBytes
        }
        job.bytesDownloaded = bytesDownloaded

        const elapsed = (now - job.lastTickAt) / 1000
        if (elapsed >= 0.5) {
          const delta = Math.max(0, bytesDownloaded - job.lastBytes)
          if (delta > 0 && elapsed > 0) {
            const instantSpeed = Math.round(delta / elapsed)
            job.lastProgressAt = now
            job.smoothedSpeedBps =
              job.smoothedSpeedBps > 0
                ? Math.round(job.smoothedSpeedBps * 0.65 + instantSpeed * 0.35)
                : instantSpeed
          } else {
            const idleMs = now - job.lastProgressAt
            if (idleMs >= 4000) {
              job.smoothedSpeedBps = 0
            } else if (idleMs >= 1500) {
              job.smoothedSpeedBps = Math.round(job.smoothedSpeedBps * 0.82)
            }
          }
          job.speedBps = Math.max(0, job.smoothedSpeedBps)
          job.lastBytes = bytesDownloaded
          job.lastTickAt = now
        }

        if (job.speedBps > 0 && job.totalBytes > bytesDownloaded) {
          job.etaSecs = Math.ceil((job.totalBytes - bytesDownloaded) / job.speedBps)
        } else {
          job.etaSecs = 0
        }
      })

      item.once('done', (_ev, state) => {
        clearJobTimers(job)
        job.speedBps = 0
        job.etaSecs = 0
        if (state === 'completed') {
          job.status = 'complete'
          job.bytesDownloaded = job.totalBytes || item.getReceivedBytes()
          return
        }

        job.status = state === 'cancelled' ? 'cancelled' : 'error'
        job.error =
          state === 'cancelled'
            ? 'Download cancelado pelo navegador do AkiraBox.'
            : 'O navegador do AkiraBox interrompeu o download antes da conclusão.'
      })
    })
  }

  async function clickDownloadCandidate(): Promise<boolean> {
    const win = getWindow()
    const result = (await win.webContents.executeJavaScript(
      `(() => {
        const nodes = Array.from(document.querySelectorAll('a[href], button, input[type="submit"], input[type="button"]'))
        const candidates = nodes
          .map((node) => {
            const text =
              (node.innerText || node.textContent || node.getAttribute('value') || node.getAttribute('title') || '')
                .replace(/\\s+/g, ' ')
                .trim()
            const href = node instanceof HTMLAnchorElement ? node.href || '' : ''
            const lower = (text + ' ' + href).toLowerCase()
            let score = 0
            if (/download|baixar/.test(lower)) score += 80
            if (/free/.test(lower)) score += 18
            if (/slow/.test(lower)) score += 10
            if (/download now|start download|gerar download|iniciar download/.test(lower)) score += 24
            if (/premium|login|register|cadastre|anunciar|share|copiar|facebook|telegram|whatsapp|discord/.test(lower)) score -= 140
            if (/\\/download|download=|file\\//.test(lower)) score += 20
            if (node.hasAttribute('download')) score += 20
            if (node.closest('header, nav, footer')) score -= 80
            if (href && href === location.href) score -= 40
            return { node, text, href, score }
          })
          .filter((entry) => entry.score > 0)
          .sort((a, b) => b.score - a.score)

        const target = candidates[0]
        if (target) {
          const node = target.node
          node.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }))
          node.dispatchEvent(new MouseEvent('mouseup', { bubbles: true }))
          node.dispatchEvent(new MouseEvent('click', { bubbles: true }))
          if (typeof node.click === 'function') {
            node.click()
          }
          return true
        }

        const form = Array.from(document.querySelectorAll('form')).find((candidate) => {
          const html = candidate.outerHTML.toLowerCase()
          return html.includes('download') || html.includes('op=')
        })

        if (form) {
          form.submit()
          return true
        }

        return false
      })()`,
      true
    )) as boolean

    return Boolean(result)
  }

  function clearJobTimers(job: AkiraboxDownloadJob): void {
    if (job.startTimeout) {
      clearTimeout(job.startTimeout)
      job.startTimeout = undefined
    }
    if (job.showTimeout) {
      clearTimeout(job.showTimeout)
      job.showTimeout = undefined
    }
    if (job.driveInterval) {
      clearInterval(job.driveInterval)
      job.driveInterval = undefined
    }
  }

  async function driveDownloadStart(jobId: string): Promise<void> {
    const job = jobs.get(jobId)
    if (!job) {
      return
    }

    const win = getWindow()
    const snapshot = await readPageSnapshot().catch(() => null)
    if (!snapshot) {
      return
    }

    if (snapshot.isChallenge) {
      if (!win.isVisible()) {
        win.setTitle('AkiraBox - conclua a verificação de segurança')
        win.show()
        win.focus()
      }

      if (await hasClearanceCookie(job.sourceUrl)) {
        await win.loadURL(job.sourceUrl).catch(() => undefined)
      }
      return
    }

    await clickDownloadCandidate().catch(() => false)
  }

  async function beginBrowserDownload(jobId: string): Promise<void> {
    pendingDownloadJobId = jobId

    const job = jobs.get(jobId)
    if (!job) {
      pendingDownloadJobId = null
      throw new Error('Job do AkiraBox não encontrado.')
    }

    await new Promise<void>(async (resolve, reject) => {
      job.startResolve = resolve
      job.startReject = reject

      const clicked = await clickDownloadCandidate().catch((error) => {
        reject(error instanceof Error ? error : new Error(String(error)))
        return false
      })

      if (!clicked) {
        pendingDownloadJobId = null
        job.startResolve = undefined
        job.startReject = undefined
        reject(new Error('O AkiraBox não exibiu um botão de download utilizável nesta página.'))
        return
      }

      job.driveInterval = setInterval(() => {
        void driveDownloadStart(jobId).catch(() => undefined)
      }, 1800)

      job.showTimeout = setTimeout(() => {
        const win = getWindow()
        if (!win.isVisible()) {
          win.setTitle('AkiraBox - finalize a etapa manual para continuar')
          win.show()
          win.focus()
        }
      }, 5000)

      job.startTimeout = setTimeout(() => {
        if (pendingDownloadJobId === jobId) {
          pendingDownloadJobId = null
          clearJobTimers(job)
          job.startResolve = undefined
          job.startReject?.(new Error('O AkiraBox não iniciou o download a tempo.'))
          job.startReject = undefined
        }
      }, 180_000)
    })
  }

  async function runDownload(jobId: string): Promise<void> {
    const job = jobs.get(jobId)
    if (!job) {
      return
    }

    try {
      const snapshot = await runExclusive(async () => {
        const state = await ensureReadyPage(job.sourceUrl)
        job.filename = chooseFilename(state, job.sourceUrl)
        job.totalBytes = chooseSize(state)
        await beginBrowserDownload(jobId)
        return state
      })

      job.filename = chooseFilename(snapshot, job.sourceUrl)
      job.totalBytes = chooseSize(snapshot)
    } catch (error) {
      clearJobTimers(job)
      pendingDownloadJobId = null
      job.status = 'error'
      job.speedBps = 0
      job.etaSecs = 0
      job.error = error instanceof Error ? error.message : String(error)
      job.startReject?.(new Error(job.error))
      job.startResolve = undefined
      job.startReject = undefined
    }
  }

  function startDownload(url: string, destPath: string): string {
    wireDownloadSession()
    const jobId = randomUUID()
    jobs.set(jobId, {
      id: jobId,
      sourceUrl: url,
      destPath,
      status: 'pending',
      bytesDownloaded: 0,
      totalBytes: 0,
      speedBps: 0,
      etaSecs: 0,
      lastBytes: 0,
      lastTickAt: Date.now(),
      lastProgressAt: Date.now(),
      smoothedSpeedBps: 0,
    })
    void runDownload(jobId)
    return jobId
  }

  async function handleAction(body: Record<string, unknown>): Promise<unknown> {
    if (body.action === 'akirabox_file_info') {
      const url = typeof body.url === 'string' ? body.url : ''
      if (!url) {
        throw new Error('Ação do AkiraBox sem URL.')
      }
      return getFileInfo(url)
    }

    if (body.action === 'akirabox_download_file') {
      const url = typeof body.url === 'string' ? body.url : ''
      const destPath = typeof body.destPath === 'string' ? body.destPath : ''
      if (!url || !destPath) {
        throw new Error('Ação do AkiraBox sem URL ou destino.')
      }
      return { jobId: startDownload(url, destPath) }
    }

    if (body.action === 'akirabox_job_status') {
      const jobId = typeof body.jobId === 'string' ? body.jobId : ''
      const job = jobs.get(jobId)
      if (!job) {
        return { status: 'error', error: 'Job do AkiraBox não encontrado.' }
      }
      return {
        status: job.status,
        bytesDownloaded: job.bytesDownloaded,
        totalBytes: job.totalBytes,
        speedBps: job.speedBps,
        etaSecs: job.etaSecs,
        filename: job.filename,
        error: job.error,
      }
    }

    throw new Error('Ação do proxy AkiraBox não suportada.')
  }

  return {
    handleAction,
  }
}
