import { randomUUID } from 'crypto'
import { existsSync, mkdirSync, rmSync } from 'fs'
import { dirname } from 'path'

import { BrowserWindow, session } from 'electron'
import {
  createExclusiveRunner,
  configureHosterSession,
  configureHosterWindow,
  delay,
  looksLikeFilename,
  parseHumanSize,
  sanitizeFilename,
} from './browser-helper-common'

const KATFILE_PARTITION = 'persist:katfile'

interface KatfilePageSnapshot {
  url: string
  title: string
  bodyText: string
  hasCaptcha: boolean
  hasDownloadForm: boolean
  filenameCandidates: string[]
  sizeCandidates: string[]
}

interface KatfileDownloadJob {
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
    const last = parsed.pathname.split('/').filter(Boolean).at(-1) || parsed.hostname
    return sanitizeFilename(last, 'arquivo_katfile')
  } catch {
    return 'arquivo_katfile'
  }
}

// eslint-disable-next-line @typescript-eslint/explicit-function-return-type
export function createKatfileService() {
  const jobs = new Map<string, KatfileDownloadJob>()
  let helperWindow: BrowserWindow | null = null
  let sessionWired = false
  let pendingDownloadJobId: string | null = null
  const runExclusive = createExclusiveRunner()

  function getWindow(): BrowserWindow {
    if (helperWindow && !helperWindow.isDestroyed()) {
      return helperWindow
    }

    configureHosterSession(KATFILE_PARTITION)
    helperWindow = new BrowserWindow({
      show: false,
      width: 1280,
      height: 920,
      autoHideMenuBar: true,
      title: 'Katfile',
      webPreferences: {
        partition: KATFILE_PARTITION,
        contextIsolation: true,
        sandbox: false,
        backgroundThrottling: false,
      },
    })

    configureHosterWindow(helperWindow, KATFILE_PARTITION)
    helperWindow.webContents.setWindowOpenHandler(() => ({ action: 'deny' }))
    helperWindow.on('closed', () => {
      helperWindow = null
    })
    return helperWindow
  }

  async function readPageSnapshot(): Promise<KatfilePageSnapshot> {
    const win = getWindow()
    return (await win.webContents.executeJavaScript(
      `(() => {
        const bodyText = (document.body?.innerText || '').replace(/\\u00a0/g, ' ').trim()
        const title = (document.title || '').trim()
        const lowerBlob = (title + '\\n' + bodyText + '\\n' + document.documentElement.outerHTML.slice(0, 16000)).toLowerCase()
        const hasChallengeWidget = Boolean(document.querySelector(
          '.cf-turnstile, iframe[src*="turnstile"], iframe[src*="challenges.cloudflare.com"], textarea[name="cf-turnstile-response"], textarea[name="g-recaptcha-response"], textarea[name="h-captcha-response"]'
        ))
        const hasCaptcha =
          hasChallengeWidget
          || lowerBlob.includes('just a moment')
          || lowerBlob.includes('checking your browser')
          || lowerBlob.includes('verify you are human')

        const hasDownloadForm = Boolean(
          document.querySelector('form#_mform')
          || document.querySelector('form#btn_download')
          || document.querySelector('form[name="F1"]')
        )

        const filenameCandidates = []
        const seenNames = new Set()
        const pushName = (value) => {
          const text = String(value || '').replace(/\\s+/g, ' ').trim()
          if (!text || seenNames.has(text)) return
          seenNames.add(text)
          filenameCandidates.push(text)
        }

        pushName(document.querySelector('input[name="fname"]')?.value)
        pushName(document.querySelector('meta[name="description"]')?.content)
        pushName(document.querySelector('#btn_download h2 span')?.textContent)
        pushName(document.querySelector('h1')?.textContent)
        pushName(document.querySelector('h2')?.textContent)
        pushName(title)

        for (const line of bodyText.split(/\\n+/).map((item) => item.trim()).filter(Boolean).slice(0, 140)) {
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

        pushSize(document.querySelector('#fsize')?.textContent)
        const sizeRegex = /\\b[0-9]+(?:[.,][0-9]+)?\\s*(KB|MB|GB|TB)\\b/i
        for (const line of bodyText.split(/\\n+/).map((item) => item.trim()).filter(Boolean).slice(0, 180)) {
          if (sizeRegex.test(line)) {
            pushSize(line)
          }
        }

        return {
          url: location.href,
          title,
          bodyText,
          hasCaptcha,
          hasDownloadForm,
          filenameCandidates,
          sizeCandidates,
        }
      })()`,
      true
    )) as KatfilePageSnapshot
  }

  function chooseFilename(snapshot: KatfilePageSnapshot, url: string): string {
    const candidates = snapshot.filenameCandidates
      .map((value) => value.replace(/\s+/g, ' ').trim())
      .filter((value) => {
        const lower = value.toLowerCase()
        return value
          && !lower.includes('katfile - free cloud storage')
          && !lower.includes('slow speed download')
          && !lower.includes('download type')
      })

    const withExtension = candidates.find(looksLikeFilename)
    if (withExtension) {
      return sanitizeFilename(withExtension, fallbackNameFromUrl(url))
    }

    const cleaner = candidates.find((value) => value.length >= 4 && value.length <= 180)
    if (cleaner) {
      return sanitizeFilename(cleaner, fallbackNameFromUrl(url))
    }

    return fallbackNameFromUrl(url)
  }

  function chooseSize(snapshot: KatfilePageSnapshot): number {
    for (const candidate of snapshot.sizeCandidates) {
      const parsed = parseHumanSize(candidate)
      if (parsed > 0) {
        return parsed
      }
    }
    return parseHumanSize(snapshot.bodyText)
  }

  async function clickFreeEntry(): Promise<boolean> {
    const win = getWindow()
    return (await win.webContents.executeJavaScript(
      `(() => {
        if (typeof (window).adEnable !== 'undefined') {
          ;(window).adEnable = true
        }
        const target =
          document.querySelector('#fbtn1')
          || document.querySelector('#m_fbtn1')
          || document.querySelector('input[name="method_free"]')
          || document.querySelector('button[name="method_free"]')
          || document.querySelector('form#btn_download button')
        if (!(target instanceof HTMLElement)) {
          return false
        }
        target.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }))
        target.dispatchEvent(new MouseEvent('mouseup', { bubbles: true }))
        target.dispatchEvent(new MouseEvent('click', { bubbles: true }))
        if (typeof target.click === 'function') {
          target.click()
        }
        return true
      })()`,
      true
    )) as boolean
  }

  async function advanceFlow(): Promise<void> {
    const win = getWindow()
    await win.webContents.executeJavaScript(
      `(() => {
        const tokenNodes = Array.from(document.querySelectorAll(
          'textarea[name="g-recaptcha-response"], input[name="g-recaptcha-response"], textarea[name="h-captcha-response"], input[name="h-captcha-response"]'
        ))

        const token = tokenNodes
          .map((node) => (
            node instanceof HTMLTextAreaElement || node instanceof HTMLInputElement
              ? node.value.trim()
              : ''
          ))
          .find((value) => value.length >= 20) || ''
        const pageText = ((document.body?.innerText || '') + '\\n' + (document.title || '')).toLowerCase()
        const hasChallengeWidget = Boolean(
          document.querySelector(
            '.cf-turnstile, iframe[src*="turnstile"], iframe[src*="challenges.cloudflare.com"], iframe[src*="recaptcha"], iframe[src*="hcaptcha"]'
          )
        )
        const hasCloudflareInterstitial =
          /just a moment|checking your browser|verify you are human|verifique se voce e humano|verifique se você é humano/i.test(pageText)

        const submittedFlag = '__gdlKatfileSubmitted'
        const clickCountKey = '__gdlKatfileClickCount'
        const activeForm =
          document.querySelector('form#_mform')
          || document.querySelector('form[name="F1"]')

        if (token.length >= 20) {
          for (const node of tokenNodes) {
            if (node instanceof HTMLTextAreaElement || node instanceof HTMLInputElement) {
              node.value = token
            }
          }

          const submitButton =
            document.querySelector('#freebtn')
            || document.querySelector('#Send')
            || document.querySelector('button[type="submit"]')
            || document.querySelector('input[type="submit"]')

          if (!(window)[submittedFlag]) {
            ;(window)[submittedFlag] = true
            if (submitButton instanceof HTMLElement) {
              submitButton.click()
            } else if (activeForm instanceof HTMLFormElement) {
              activeForm.submit()
            }
          }
          return
        }

        if (hasChallengeWidget || hasCloudflareInterstitial) {
          return
        }

        if (typeof (window).estimated_time === 'number' && typeof (window).es === 'function') {
          if ((window).estimated_time > 1) {
            ;(window).estimated_time = 1
          }
          ;(window).es()
          return
        }

        if (typeof (window).adEnable !== 'undefined') {
          ;(window).adEnable = true
        }

        const freeButton =
          document.querySelector('#fbtn1')
          || document.querySelector('#m_fbtn1')
          || document.querySelector('input[name="method_free"]')
          || document.querySelector('button[name="method_free"]')

        const clickCount = Number((window)[clickCountKey] || 0)
        if (freeButton instanceof HTMLElement && clickCount < 3) {
          ;(window)[clickCountKey] = clickCount + 1
          freeButton.click()
        } else if (activeForm instanceof HTMLFormElement) {
          const submitButton =
            document.querySelector('#freebtn')
            || document.querySelector('#Send')
            || document.querySelector('button[type="submit"]')
            || document.querySelector('input[type="submit"]')
          if (submitButton instanceof HTMLElement) {
            submitButton.click()
          }
        }
      })()`,
      true
    )
  }

  function clearJobTimers(job: KatfileDownloadJob): void {
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

  function wireDownloadSession(): void {
    if (sessionWired) {
      return
    }
    sessionWired = true

    session.fromPartition(KATFILE_PARTITION).on('will-download', (_event, item) => {
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
            // segue o fluxo padrão abaixo
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
            ? 'Download cancelado pelo navegador integrado do Katfile.'
            : 'O navegador integrado do Katfile interrompeu o download antes da conclusão.'
      })
    })
  }

  async function beginBrowserDownload(jobId: string): Promise<void> {
    pendingDownloadJobId = jobId

    const job = jobs.get(jobId)
    if (!job) {
      pendingDownloadJobId = null
      throw new Error('Job do Katfile não encontrado.')
    }

    await new Promise<void>((resolve, reject) => {
      job.startResolve = resolve
      job.startReject = reject

      job.driveInterval = setInterval(() => {
        void advanceFlow().catch(() => undefined)
      }, 1800)

      job.showTimeout = setTimeout(() => {
        const win = getWindow()
        if (!win.isVisible()) {
          win.setTitle('Katfile - conclua a etapa manual para continuar')
          win.show()
          win.focus()
        }
      }, 6000)

      job.startTimeout = setTimeout(() => {
        if (pendingDownloadJobId === jobId) {
          pendingDownloadJobId = null
          clearJobTimers(job)
          job.startResolve = undefined
          job.startReject?.(new Error('O Katfile não iniciou o download a tempo.'))
          job.startReject = undefined
        }
      }, 8 * 60_000)
    })
  }

  async function getFileInfo(url: string): Promise<{
    filename: string
    size: number
    mime_type: null
    is_folder: false
    children: null
  }> {
    return runExclusive(async () => {
      const win = getWindow()
      await win.loadURL(url)
      await delay(1200)
      const snapshot = await readPageSnapshot()

      return {
        filename: chooseFilename(snapshot, url),
        size: chooseSize(snapshot),
        mime_type: null,
        is_folder: false,
        children: null,
      }
    })
  }

  async function runDownload(jobId: string): Promise<void> {
    const job = jobs.get(jobId)
    if (!job) {
      return
    }

    try {
      await runExclusive(async () => {
        const win = getWindow()
        await win.loadURL(job.sourceUrl)
        await delay(1200)

        const initial = await readPageSnapshot()
        job.filename = chooseFilename(initial, job.sourceUrl)
        job.totalBytes = chooseSize(initial)

        await clickFreeEntry().catch(() => false)
        await delay(1000)
        await beginBrowserDownload(jobId)
      })
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
    if (body.action === 'katfile_download_file') {
      const url = typeof body.url === 'string' ? body.url : ''
      const destPath = typeof body.destPath === 'string' ? body.destPath : ''
      if (!url || !destPath) {
        throw new Error('Ação do Katfile sem URL ou destino.')
      }
      return { jobId: startDownload(url, destPath) }
    }

    if (body.action === 'katfile_job_status') {
      const jobId = typeof body.jobId === 'string' ? body.jobId : ''
      const job = jobs.get(jobId)
      if (!job) {
        return { status: 'error', error: 'Job do Katfile não encontrado.' }
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

    if (body.action === 'katfile_file_info') {
      const url = typeof body.url === 'string' ? body.url : ''
      if (!url) {
        throw new Error('Ação do Katfile sem URL.')
      }
      return getFileInfo(url)
    }

    throw new Error('Ação do proxy Katfile não suportada.')
  }

  return {
    handleAction,
  }
}
