import { randomUUID } from 'crypto'
import { existsSync, mkdirSync, rmSync } from 'fs'
import { dirname } from 'path'

import { BrowserWindow, net, session } from 'electron'
import {
  createExclusiveRunner,
  delay,
  HOSTER_BROWSER_USER_AGENT,
  looksLikeFilename,
  parseHumanSize,
  sanitizeFilename,
} from './browser-helper-common'

const BRUPLOAD_PARTITION = 'persist:brupload'
const BRUPLOAD_LOGIN_URL = 'https://www.brupload.net/login.html'
const BRUPLOAD_ACCOUNT_URL = 'https://www.brupload.net/?op=my_account'

export interface BruploadStoredAccount {
  email: string
  password: string
  cookies?: string[]
  verifiedAt?: string
}

interface CreateBruploadServiceOptions {
  readAccount: () => BruploadStoredAccount | null | undefined
  saveAccount: (account: BruploadStoredAccount | null) => void
}

interface BruploadDownloadJob {
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

interface BruploadPageSnapshot {
  url: string
  title: string
  bodyText: string
  htmlSample: string
  filenameCandidates: string[]
  sizeCandidates: string[]
  errorText?: string
}

function fallbackNameFromUrl(url: string): string {
  try {
    const parsed = new URL(url)
    const last = parsed.pathname.split('/').filter(Boolean).at(-1) || parsed.hostname
    return sanitizeFilename(last, 'arquivo_brupload')
  } catch {
    return 'arquivo_brupload'
  }
}

function parseCookiePairs(rawCookies: string[] | undefined): Array<{ name: string; value: string }> {
  const pairs: Array<{ name: string; value: string }> = []
  for (const header of rawCookies ?? []) {
    for (const part of String(header).split(';')) {
      const trimmed = part.trim()
      if (!trimmed) continue
      const eq = trimmed.indexOf('=')
      if (eq <= 0) continue
      pairs.push({
        name: trimmed.slice(0, eq).trim(),
        value: trimmed.slice(eq + 1).trim(),
      })
    }
  }
  return pairs
}

// eslint-disable-next-line @typescript-eslint/explicit-function-return-type
export function createBruploadService(options: CreateBruploadServiceOptions) {
  const jobs = new Map<string, BruploadDownloadJob>()
  let helperWindow: BrowserWindow | null = null
  let sessionWired = false
  let pendingDownloadJobId: string | null = null
  const runExclusive = createExclusiveRunner()

  // eslint-disable-next-line @typescript-eslint/explicit-function-return-type
  function getSession() {
    return session.fromPartition(BRUPLOAD_PARTITION)
  }

  function currentAccount(): BruploadStoredAccount | null {
    return options.readAccount() ?? null
  }

  function persistAccount(next: BruploadStoredAccount | null): void {
    options.saveAccount(next)
  }

  async function restoreSavedCookies(): Promise<void> {
    const account = currentAccount()
    if (!account?.cookies?.length) {
      return
    }

    for (const cookie of parseCookiePairs(account.cookies)) {
      try {
        await getSession().cookies.set({
          url: 'https://www.brupload.net',
          domain: '.brupload.net',
          path: '/',
          secure: true,
          httpOnly: false,
          sameSite: 'lax',
          name: cookie.name,
          value: cookie.value,
        })
      } catch {
        // segue restaurando os demais cookies
      }
    }
  }

  async function snapshotSessionCookies(): Promise<string[]> {
    const cookies = await getSession().cookies.get({})
    const filtered = cookies.filter((cookie) => {
      const domain = cookie.domain ?? ''
      return domain.includes('brupload')
    })
    const header = filtered.map((cookie) => `${cookie.name}=${cookie.value}`).join('; ')
    return header ? [header] : []
  }

  function getWindow(): BrowserWindow {
    if (helperWindow && !helperWindow.isDestroyed()) {
      return helperWindow
    }

    helperWindow = new BrowserWindow({
      show: false,
      width: 1280,
      height: 920,
      autoHideMenuBar: true,
      title: 'BRupload',
      webPreferences: {
        partition: BRUPLOAD_PARTITION,
        contextIsolation: true,
        sandbox: false,
        backgroundThrottling: false,
      },
    })

    helperWindow.webContents.setUserAgent(HOSTER_BROWSER_USER_AGENT)
    helperWindow.webContents.setWindowOpenHandler(() => ({ action: 'deny' }))
    helperWindow.on('closed', () => {
      helperWindow = null
    })
    return helperWindow
  }

  async function sessionTextRequest(url: string): Promise<string> {
    return new Promise<string>((resolve, reject) => {
      const request = net.request({ url, method: 'GET', session: getSession() })
      request.setHeader('User-Agent', HOSTER_BROWSER_USER_AGENT)
      request.setHeader('Accept', 'text/html,application/xhtml+xml,*/*;q=0.9')
      request.setHeader('Accept-Language', 'pt-BR,pt;q=0.9,en-US;q=0.8')
      const chunks: Buffer[] = []
      request.on('response', (response) => {
        response.on('data', (chunk) => chunks.push(chunk as Buffer))
        response.on('end', () => resolve(Buffer.concat(chunks).toString('utf8')))
        response.on('error', reject)
      })
      request.on('error', reject)
      request.end()
    })
  }

  async function sessionLooksAuthenticated(): Promise<boolean> {
    const html = await sessionTextRequest(BRUPLOAD_ACCOUNT_URL).catch(() => '')
    const lower = html.toLowerCase()
    if (!lower) {
      return false
    }
    if (lower.includes('login.html') && lower.includes('password')) {
      return false
    }
    return lower.includes('logout')
      || lower.includes('?op=logout')
      || lower.includes('my account')
      || lower.includes('meus arquivos')
      || lower.includes('minha conta')
  }

  async function readAccountEmail(): Promise<string> {
    const html = await sessionTextRequest(BRUPLOAD_ACCOUNT_URL).catch(() => '')
    const valueMatch =
      html.match(/name=["']email["'][^>]*value=["']([^"']+)["']/i)
      || html.match(/value=["']([^"']+@[^"']+)["']/i)
      || html.match(/\b[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}\b/i)

    return valueMatch?.[1]?.trim() ?? currentAccount()?.email ?? ''
  }

  async function persistSessionAccount(): Promise<void> {
    const cookies = await snapshotSessionCookies()
    const email = await readAccountEmail().catch(() => currentAccount()?.email ?? '')
    persistAccount({
      email,
      password: currentAccount()?.password ?? '',
      cookies,
      verifiedAt: new Date().toISOString(),
    })
  }

  async function readPageSnapshot(): Promise<BruploadPageSnapshot> {
    const win = getWindow()
    return (await win.webContents.executeJavaScript(
      `(() => {
        const html = document.documentElement?.outerHTML || ''
        const bodyText = (document.body?.innerText || '').replace(/\\u00a0/g, ' ').trim()
        const title = (document.title || '').trim()

        const filenameCandidates = []
        const seenNames = new Set()
        const pushName = (value) => {
          const text = String(value || '').replace(/\\s+/g, ' ').trim()
          if (!text || seenNames.has(text)) return
          seenNames.add(text)
          filenameCandidates.push(text)
        }

        pushName(document.querySelector('input[name="fname"]')?.value)
        pushName(document.querySelector('.dfilename')?.textContent)
        pushName(document.querySelector('title')?.textContent)
        pushName(document.querySelector('meta[name="description"]')?.content)
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

        pushSize(document.querySelector('.tamanho-arquivo')?.textContent)
        pushSize(document.querySelector('.statd + span')?.textContent)
        const sizeRegex = /\\b[0-9]+(?:[.,][0-9]+)?\\s*(KB|MB|GB|TB)\\b/i
        for (const line of bodyText.split(/\\n+/).map((item) => item.trim()).filter(Boolean).slice(0, 180)) {
          if (sizeRegex.test(line)) {
            pushSize(line)
          }
        }

        const errNode = document.querySelector('.err, .warning')
        const errorText = errNode ? String(errNode.textContent || '').replace(/\\s+/g, ' ').trim() : ''

        return {
          url: location.href,
          title,
          bodyText,
          htmlSample: html.slice(0, 24000),
          filenameCandidates,
          sizeCandidates,
          errorText,
        }
      })()`,
      true,
    )) as BruploadPageSnapshot
  }

  function chooseFilename(snapshot: BruploadPageSnapshot, url: string): string {
    const candidates = snapshot.filenameCandidates
      .map((value) => value.replace(/\s+/g, ' ').trim())
      .filter((value) => {
        const lower = value.toLowerCase()
        return value
          && !lower.includes('brupload')
          && !lower.includes('download gratuito')
          && !lower.includes('armazenamento')
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

  function chooseSize(snapshot: BruploadPageSnapshot): number {
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
        const selectors = [
          '#fbtn1',
          '#m_fbtn1',
          '#downloadbtn',
          '#freebtn',
          'input[name="method_free"]',
          'button[name="method_free"]',
          '.downloadbtn'
        ]

        for (const selector of selectors) {
          const target = document.querySelector(selector)
          if (!(target instanceof HTMLElement)) {
            continue
          }
          const disabled = 'disabled' in target && Boolean((target).disabled)
          if (disabled) {
            continue
          }
          target.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }))
          target.dispatchEvent(new MouseEvent('mouseup', { bubbles: true }))
          target.dispatchEvent(new MouseEvent('click', { bubbles: true }))
          if (typeof target.click === 'function') {
            target.click()
          }
          return true
        }
        return false
      })()`,
      true,
    )) as boolean
  }

  async function advanceFlow(): Promise<void> {
    const win = getWindow()
    await win.webContents.executeJavaScript(
      `(() => {
        const allTokenNodes = Array.from(document.querySelectorAll(
          'textarea[name="cf-turnstile-response"], input[name="cf-turnstile-response"], textarea[name="g-recaptcha-response"], input[name="g-recaptcha-response"], textarea[name="h-captcha-response"], input[name="h-captcha-response"]'
        ))

        const token = allTokenNodes
          .map((node) => (
            node instanceof HTMLTextAreaElement || node instanceof HTMLInputElement
              ? node.value.trim()
              : ''
          ))
          .find((value) => value.length >= 20) || ''
        const pageText = ((document.body?.innerText || '') + '\\n' + (document.title || '')).toLowerCase()

        const activeForm =
          document.querySelector('form[name="F1"]')
          || document.querySelector('form#btn_download')
          || document.querySelector('form')

        const clickableSelectors = [
          '#freebtn',
          '#downloadbtn',
          'input[name="method_free"]',
          'button[name="method_free"]',
          'button[type="submit"]',
          'input[type="submit"]',
          '.downloadbtn'
        ]

        const findClickable = () => {
          for (const selector of clickableSelectors) {
            const node = document.querySelector(selector)
            if (!(node instanceof HTMLElement)) continue
            const disabled = 'disabled' in node && Boolean((node).disabled)
            if (disabled) continue
            return node
          }
          return null
        }

        const hasCaptchaWidget = Boolean(
          document.querySelector('.g-recaptcha, .h-captcha, .cf-turnstile, iframe[src*="recaptcha"], iframe[src*="hcaptcha"], iframe[src*="turnstile"]')
        ) || /captcha|verification|verifica|cloudflare/.test(pageText)
        const countdownText = [
          document.querySelector('.tt')?.textContent || '',
          document.querySelector('.tt2')?.textContent || '',
          document.querySelector('#countdown')?.textContent || '',
        ].join(' ')
        const countdownPending = /\\b([1-9][0-9]*)\\b/.test(countdownText)
        const submittedFlag = '__gdlBruploadSubmitted'
        const clickCountKey = '__gdlBruploadClickCount'
        const clickable = findClickable()

        if (token) {
          for (const node of allTokenNodes) {
            if (node instanceof HTMLTextAreaElement || node instanceof HTMLInputElement) {
              node.value = token
            }
          }
          if (!(window)[submittedFlag]) {
            ;(window)[submittedFlag] = true
            if (clickable) {
              clickable.click()
            } else if (activeForm instanceof HTMLFormElement) {
              activeForm.submit()
            }
          }
          return
        }

        if (hasCaptchaWidget) {
          return
        }

        if (typeof (window).estimated_time === 'number' && typeof (window).es === 'function') {
          if ((window).estimated_time > 1) {
            ;(window).estimated_time = 1
          }
          ;(window).es()
          return
        }

        if (clickable && !hasCaptchaWidget && !countdownPending) {
          const text = ((clickable.textContent || '') + ' ' + ((clickable).value || '')).toLowerCase()
          if (/send|continue|download|baixar|gratuito|free/.test(text)) {
            clickable.click()
            return
          }
        }

        const entryButton =
          document.querySelector('#fbtn1')
          || document.querySelector('#m_fbtn1')
          || clickable

        if (typeof (window).adEnable !== 'undefined') {
          ;(window).adEnable = true
        }

        const clickCount = Number((window)[clickCountKey] || 0)
        if (entryButton instanceof HTMLElement && clickCount < 3) {
          ;(window)[clickCountKey] = clickCount + 1
          entryButton.click()
        }
      })()`,
      true,
    )
  }

  function clearJobTimers(job: BruploadDownloadJob): void {
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
            // segue fluxo padrão
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

        job.etaSecs =
          job.speedBps > 0 && job.totalBytes > bytesDownloaded
            ? Math.ceil((job.totalBytes - bytesDownloaded) / job.speedBps)
            : 0
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
            ? 'Download cancelado pelo navegador integrado do BRupload.'
            : 'O navegador integrado do BRupload interrompeu o download antes da conclusão.'
      })
    })
  }

  async function beginBrowserDownload(jobId: string): Promise<void> {
    pendingDownloadJobId = jobId

    const job = jobs.get(jobId)
    if (!job) {
      pendingDownloadJobId = null
      throw new Error('Job do BRupload não encontrado.')
    }

    await new Promise<void>((resolve, reject) => {
      job.startResolve = resolve
      job.startReject = reject

      job.driveInterval = setInterval(() => {
        void advanceFlow().catch(() => undefined)
      }, 1500)

      job.showTimeout = setTimeout(() => {
        const win = getWindow()
        if (!win.isVisible()) {
          win.setTitle('BRupload - conclua a etapa manual para continuar')
          win.show()
          win.focus()
        }
      }, 8000)

      job.startTimeout = setTimeout(() => {
        if (pendingDownloadJobId === jobId) {
          pendingDownloadJobId = null
          clearJobTimers(job)
          job.startResolve = undefined
          job.startReject?.(new Error(job.error || 'O BRupload não iniciou o download a tempo.'))
          job.startReject = undefined
        }
      }, 240_000)
    })
  }

  async function getFileInfo(url: string): Promise<{
    filename: string
    size: number
    mime_type: null
    is_folder: false
    children: null
  }> {
    await restoreSavedCookies()
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
      await restoreSavedCookies()
      await runExclusive(async () => {
        const win = getWindow()
        await win.loadURL(job.sourceUrl)
        await delay(1200)

        const initial = await readPageSnapshot()
        job.filename = chooseFilename(initial, job.sourceUrl)
        job.totalBytes = chooseSize(initial)

        const lowerBlob = `${initial.title}\n${initial.bodyText}\n${initial.htmlSample}`.toLowerCase()
        if (initial.errorText) {
          throw new Error(initial.errorText)
        }
        if (lowerBlob.includes('download bigger files') || lowerBlob.includes('1024 mb only')) {
          throw new Error('O BRupload gratuito sem conta não libera este arquivo. Entre com uma conta free para continuar.')
        }

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

  async function login(): Promise<boolean> {
    await restoreSavedCookies()

    const win = getWindow()
    win.setTitle('BRupload - faça login e aguarde a validação')
    await win.loadURL(BRUPLOAD_LOGIN_URL)
    win.show()
    win.focus()

    const deadline = Date.now() + 10 * 60_000
    while (!win.isDestroyed() && Date.now() < deadline) {
      if (await sessionLooksAuthenticated().catch(() => false)) {
        await persistSessionAccount()
        if (!win.isDestroyed()) {
          win.hide()
        }
        return true
      }
      await delay(1200)
    }

    throw new Error('O login do BRupload não foi concluído a tempo.')
  }

  async function logout(): Promise<boolean> {
    await getSession().clearStorageData({
      storages: ['cookies', 'localstorage', 'serviceworkers', 'cachestorage', 'indexdb'],
    })
    if (helperWindow && !helperWindow.isDestroyed()) {
      helperWindow.close()
    }
    helperWindow = null
    persistAccount(null)
    return true
  }

  async function isLoggedIn(): Promise<boolean> {
    await restoreSavedCookies().catch(() => undefined)
    const authenticated = await sessionLooksAuthenticated().catch(() => false)
    if (authenticated) {
      await persistSessionAccount().catch(() => undefined)
      return true
    }

    const account = currentAccount()
    if (account?.verifiedAt || account?.cookies?.length) {
      persistAccount(null)
    }
    return false
  }

  function accountInfo(): { email: string; verifiedAt?: string } | null {
    const account = currentAccount()
    if (!account?.verifiedAt && !account?.email) {
      return null
    }
    return {
      email: account.email,
      verifiedAt: account.verifiedAt,
    }
  }

  async function handleAction(body: Record<string, unknown>): Promise<unknown> {
    if (body.action === 'brupload_download_file') {
      const url = typeof body.url === 'string' ? body.url : ''
      const destPath = typeof body.destPath === 'string' ? body.destPath : ''
      if (!url || !destPath) {
        throw new Error('Ação do BRupload sem URL ou destino.')
      }
      return { jobId: startDownload(url, destPath) }
    }

    if (body.action === 'brupload_job_status') {
      const jobId = typeof body.jobId === 'string' ? body.jobId : ''
      const job = jobs.get(jobId)
      if (!job) {
        return { status: 'error', error: 'Job do BRupload não encontrado.' }
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

    if (body.action === 'brupload_file_info') {
      const url = typeof body.url === 'string' ? body.url : ''
      if (!url) {
        throw new Error('Ação do BRupload sem URL.')
      }
      return getFileInfo(url)
    }

    throw new Error('Ação do proxy BRupload não suportada.')
  }

  return {
    handleAction,
    login,
    logout,
    isLoggedIn,
    accountInfo,
  }
}
