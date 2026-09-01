import { randomUUID } from 'crypto'
import { existsSync, mkdirSync, rmSync } from 'fs'
import { dirname } from 'path'

import { BrowserWindow, session } from 'electron'
import { delay, HOSTER_BROWSER_USER_AGENT, parseHumanSize } from './browser-helper-common'
import { logMain } from './debug-log'

const TERABOX_PARTITION = 'persist:terabox'
const TERABOX_LOGIN_URL = 'https://www.terabox.com/portuguese/login'
const TERABOX_HOME_URL = 'https://www.terabox.com/portuguese/main?category=all&path=%2F'

export interface TeraboxStoredAccount {
  email: string
  password: string
  cookies?: string[]
  verifiedAt?: string
}

interface TeraboxDownloadJob {
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
  expectedUrl?: string
  lastBytes: number
  lastTickAt: number
  lastProgressAt: number
  smoothedSpeedBps: number
  shouldCleanupCloudCopy?: boolean
}

interface TeraboxShareDomEntry {
  filename: string
  sizeText: string
  isDir: boolean
}

interface CreateTeraboxServiceOptions {
  readAccount: () => TeraboxStoredAccount | null | undefined
  saveAccount: (account: TeraboxStoredAccount | null) => void
}

function customBase64(value: string): string {
  const chars = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/'
  let out = ''
  let i = 0
  while (i < value.length) {
    const a = value.charCodeAt(i++) & 255
    if (i === value.length) {
      out += chars.charAt(a >> 2)
      out += chars.charAt((a & 3) << 4)
      out += '=='
      break
    }
    const b = value.charCodeAt(i++)
    if (i === value.length) {
      out += chars.charAt(a >> 2)
      out += chars.charAt(((a & 3) << 4) | ((b & 240) >> 4))
      out += chars.charAt((b & 15) << 2)
      out += '='
      break
    }
    const c = value.charCodeAt(i++)
    out += chars.charAt(a >> 2)
    out += chars.charAt(((a & 3) << 4) | ((b & 240) >> 4))
    out += chars.charAt(((b & 15) << 2) | ((c & 192) >> 6))
    out += chars.charAt(c & 63)
  }
  return out
}

function extractQueryValue(url: string, key: string): string | null {
  try {
    const parsed = new URL(url)
    const value = parsed.searchParams.get(key)
    return value ? decodeURIComponent(value.replace(/\+/g, ' ')) : null
  } catch {
    return null
  }
}

function extractDir(url: string): string | null {
  return extractQueryValue(url, 'dir') ?? extractQueryValue(url, 'path')
}

function extractSyntheticFileName(url: string): string | null {
  return extractQueryValue(url, 'gdl_file')
}

function cleanTeraboxUrl(url: string): string {
  try {
    const parsed = new URL(url)
    parsed.searchParams.delete('gdl_file')
    return parsed.toString()
  } catch {
    return url
  }
}

function buildShareUrl(url: string, path?: string | null, fileName?: string | null): string {
  const parsed = new URL(cleanTeraboxUrl(url))
  if (path) {
    parsed.searchParams.set('path', path)
  } else {
    parsed.searchParams.delete('path')
  }
  if (fileName) {
    parsed.searchParams.set('gdl_file', fileName)
  } else {
    parsed.searchParams.delete('gdl_file')
  }
  return parsed.toString()
}

function joinSharePath(basePath: string | null, childName: string): string {
  if (!basePath || basePath === '/') {
    return `/${childName}`
  }
  return `${basePath.replace(/\/+$/, '')}/${childName}`
}

function basenamePath(path: string | null): string {
  return path?.split('/').filter(Boolean).pop() ?? ''
}

function parseByteSize(sizeText: string): number {
  const normalized = sizeText.trim().replace(/\s+/g, '').toUpperCase()
  if (!normalized || normalized === '-') {
    return 0
  }
  return parseHumanSize(normalized)
}

function extractDownloadKey(url?: string): string | null {
  if (!url) return null
  try {
    const parsed = new URL(url)
    return parsed.searchParams.get('fid') || parsed.searchParams.get('fsid')
  } catch {
    return null
  }
}

// eslint-disable-next-line @typescript-eslint/explicit-function-return-type
export function createTeraboxService(options: CreateTeraboxServiceOptions) {
  const jobs = new Map<string, TeraboxDownloadJob>()
  const pendingDownloads = new Map<string, string>()
  const pendingDownloadKeys = new Map<string, string>()
  let helperWindow: BrowserWindow | null = null
  let authPromise: Promise<void> | null = null
  let sessionWired = false

  function refreshHelperThrottling(): void {
    if (!helperWindow || helperWindow.isDestroyed()) return
    const hasPendingBrowserWork = [...jobs.values()].some((job) => job.status === 'pending')
    helperWindow.webContents.setBackgroundThrottling(!hasPendingBrowserWork)
  }

  // eslint-disable-next-line @typescript-eslint/explicit-function-return-type
  function getSession() {
    return session.fromPartition(TERABOX_PARTITION)
  }

  function currentAccount(): TeraboxStoredAccount | null {
    return options.readAccount() ?? null
  }

  function persistAccount(next: TeraboxStoredAccount | null): void {
    options.saveAccount(next)
  }

  async function snapshotSessionCookies(): Promise<string[]> {
    const cookies = await getSession().cookies.get({})
    const teraboxCookies = cookies.filter((cookie) => {
      const domain = cookie.domain ?? ''
      return domain.includes('terabox') || domain.includes('1024tera')
    })
    const header = teraboxCookies.map((cookie) => `${cookie.name}=${cookie.value}`).join('; ')
    return header ? [header] : []
  }

  async function persistSessionAccount(email?: string, password?: string): Promise<void> {
    const current = currentAccount()
    const cookies = await snapshotSessionCookies()
    persistAccount({
      email: email ?? current?.email ?? '',
      password: password ?? current?.password ?? '',
      cookies,
      verifiedAt: new Date().toISOString(),
    })
  }

  function getWindow(): BrowserWindow {
    if (helperWindow && !helperWindow.isDestroyed()) {
      refreshHelperThrottling()
      return helperWindow
    }

    helperWindow = new BrowserWindow({
      show: false,
      width: 1400,
      height: 1000,
      autoHideMenuBar: true,
      webPreferences: {
        partition: TERABOX_PARTITION,
        contextIsolation: true,
        sandbox: false,
        backgroundThrottling: true,
      },
    })
    helperWindow.webContents.setUserAgent(HOSTER_BROWSER_USER_AGENT)
    refreshHelperThrottling()
    return helperWindow
  }

  async function typeText(text: string): Promise<void> {
    const win = getWindow()
    for (const ch of text) {
      win.webContents.sendInputEvent({ type: 'char', keyCode: ch })
      await delay(30)
    }
  }

  async function waitFor<T>(
    evaluator: string,
    timeoutMs = 20_000,
    intervalMs = 250
  ): Promise<T | null> {
    const win = getWindow()
    const deadline = Date.now() + timeoutMs
    while (Date.now() < deadline) {
      try {
        const value = (await win.webContents.executeJavaScript(evaluator, true)) as T | null
        if (value) {
          return value
        }
      } catch {
        // keep polling while the page is settling
      }
      await delay(intervalMs)
    }
    return null
  }

  async function clickByText(candidates: string[]): Promise<boolean> {
    const win = getWindow()
    const escaped = JSON.stringify(candidates.map((value) => value.trim().toLowerCase()))
    const clicked = await win.webContents.executeJavaScript(
      `(() => {
        const candidates = ${escaped}
        const nodes = Array.from(document.querySelectorAll('button, a, div, span'))
        const target = nodes.find((node) => candidates.includes((node.innerText || '').trim().toLowerCase()))
        if (!target) return false
        target.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }))
        target.dispatchEvent(new MouseEvent('mouseup', { bubbles: true }))
        target.dispatchEvent(new MouseEvent('click', { bubbles: true }))
        return true
      })()`,
      true
    )
    return Boolean(clicked)
  }

  async function waitForShareItems(timeoutMs = 25_000): Promise<void> {
    const state = await waitFor<string>(
      `(() => {
        const hasItems = document.querySelectorAll('.common-file-item').length > 0
        const emptyState = /nenhum arquivo|no files/i.test(document.body.innerText || '')
        const body = document.body.innerText || ''
        const needsExtractionCode =
          /código de extração|codigo de extracao|extraction code|please enter the extraction code/i.test(body) ||
          Boolean(document.querySelector('input[placeholder*="código"], input[placeholder*="code"], input[class*="extract"]'))
        if (needsExtractionCode) return 'extraction-code'
        if (hasItems) return 'items'
        if (emptyState) return 'empty'
        return ''
      })()`,
      timeoutMs
    )

    if (state === 'extraction-code') {
      throw new Error('Este link do TeraBox exige código de extração. Cole um link que já inclua o código ou abra o link no navegador integrado para liberar o compartilhamento.')
    }

    if (!state) {
      throw new Error('O TeraBox não carregou a lista desta pasta a tempo.')
    }
  }

  async function loadSharePage(url: string, path?: string | null): Promise<void> {
    const win = getWindow()
    await ensureAuthenticated()
    await win.loadURL(buildShareUrl(url, path ?? null, null))
    await waitForShareItems()
  }

  async function readCurrentShareEntries(): Promise<{
    shareName: string
    items: TeraboxShareDomEntry[]
  }> {
    const win = getWindow()
    const result = await win.webContents.executeJavaScript(
      `(() => {
        const shareName =
          (document.querySelector('.file-name-info .file-name')?.innerText || '').trim() ||
          (document.querySelector('.file-name')?.innerText || '').trim()
        const items = Array.from(document.querySelectorAll('.common-file-item'))
          .map((row) => {
            const className = row.className || ''
            const filename =
              (row.querySelector('.file-item-name-link')?.innerText || '').trim() ||
              (row.querySelector('.file-item-name-text')?.innerText || '').trim() ||
              (row.querySelector('.file-item-name')?.innerText || '').trim()
            const sizeText = (row.querySelector('.file-item-size')?.innerText || '').trim()
            return {
              filename,
              sizeText,
              isDir: sizeText === '-' && !/common-file-video/i.test(className),
            }
          })
          .filter((item) => item.filename)
        return { shareName, items }
      })()`,
      true
    )

    const shareName = typeof result?.shareName === 'string' ? result.shareName.trim() : ''
    const rawItems = Array.isArray(result?.items) ? result.items : []

    return {
      shareName,
      items: rawItems.map((item) => {
        const record = item as Record<string, unknown>
        return {
          filename: String(record.filename ?? '').trim(),
          sizeText: String(record.sizeText ?? '').trim(),
          isDir: Boolean(record.isDir),
        }
      }),
    }
  }

  async function openSharedFileFromCurrentFolder(filename: string): Promise<void> {
    const win = getWindow()
    const clicked = await win.webContents.executeJavaScript(
      `(() => {
        const target = Array.from(document.querySelectorAll('.common-file-item'))
          .find((row) => ((row.querySelector('.file-item-name-link')?.innerText || '').trim()) === ${JSON.stringify(filename)})
        if (!target) return false
        const clickable = target.querySelector('.file-item-name-link') || target
        clickable.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }))
        clickable.dispatchEvent(new MouseEvent('mouseup', { bubbles: true }))
        clickable.dispatchEvent(new MouseEvent('click', { bubbles: true }))
        return true
      })()`,
      true
    )

    if (!clicked) {
      throw new Error(`O arquivo "${filename}" não apareceu na pasta compartilhada do TeraBox.`)
    }

    const opened = await waitFor<boolean>(
      `location.href.includes('/sharing/videoPlay') || location.href.includes('fsid=')`,
      25_000
    )

    if (!opened) {
      throw new Error(`O TeraBox não abriu o arquivo "${filename}" a partir da pasta compartilhada.`)
    }
  }

  async function sessionLooksAuthenticated(): Promise<boolean> {
    const win = getWindow()
    await win.loadURL(TERABOX_HOME_URL)
    await delay(4000)
    const url = win.webContents.getURL()
    if (url.includes('/login') || url.includes('/passport')) {
      return false
    }
    const loginInput = await win.webContents.executeJavaScript(
      `Boolean(document.querySelector('input.email-input') || document.querySelector('input.pwd-input'))`,
      true
    )
    return !loginInput
  }

  async function ensureAuthenticated(): Promise<void> {
    if (authPromise) {
      return authPromise
    }

    authPromise = (async () => {
      if (await sessionLooksAuthenticated()) {
        await persistSessionAccount()
        return
      }

      const account = currentAccount()
      if (!account?.email || !account.password) {
        throw new Error('A conta do TeraBox precisa ter e-mail e senha salvos para liberar este download.')
      }

      const win = getWindow()
      await win.loadURL(TERABOX_LOGIN_URL)
      const ready = await waitFor<boolean>(
        `Boolean(document.querySelector('input.email-input') && document.querySelector('input.pwd-input'))`,
        25_000
      )
      if (!ready) {
        throw new Error('Não foi possível abrir a tela de login do TeraBox.')
      }

      await win.webContents.executeJavaScript(
        `(() => {
          const email = document.querySelector('input.email-input')
          const pwd = document.querySelector('input.pwd-input')
          if (email) email.value = ''
          if (pwd) pwd.value = ''
        })()`,
        true
      )

      await win.webContents.executeJavaScript(`document.querySelector('input.email-input')?.focus()`, true)
      await delay(300)
      await typeText(account.email)
      await win.webContents.executeJavaScript(`document.querySelector('input.pwd-input')?.focus()`, true)
      await delay(300)
      await typeText(account.password)
      await delay(500)
      await win.webContents.executeJavaScript(
        `(() => {
          const email = document.querySelector('input.email-input')
          const pwd = document.querySelector('input.pwd-input')
          for (const el of [email, pwd]) {
            if (!el) continue
            el.dispatchEvent(new Event('input', { bubbles: true }))
            el.dispatchEvent(new Event('change', { bubbles: true }))
            el.dispatchEvent(new Event('blur', { bubbles: true }))
          }
          document.querySelector('.login-submit-btn')?.click()
        })()`,
        true
      )

      const logged = await waitFor<boolean>(
        `!location.href.includes('/login') &&
         !location.href.includes('/passport') &&
         !document.querySelector('input.email-input') &&
         !document.querySelector('input.pwd-input')`,
        30_000
      )

      if (!logged) {
        const message = await win.webContents.executeJavaScript(
          `(() => {
            const candidates = Array.from(document.querySelectorAll('[class*=error], [class*=tip], [class*=message]'))
              .map((node) => (node.textContent || '').trim())
              .filter(Boolean)
            return candidates[0] || document.body.innerText.slice(0, 400)
          })()`,
          true
        )
        throw new Error(
          typeof message === 'string' && message.trim()
            ? message.trim()
            : 'O login do TeraBox não foi concluído.'
        )
      }

      await persistSessionAccount(account.email, account.password)
    })().finally(() => {
      authPromise = null
    })

    return authPromise
  }

  function wireDownloadSession(): void {
    if (sessionWired) {
      return
    }
    sessionWired = true

    const teraboxSession = getSession()
    teraboxSession.on('will-download', (_event, item) => {
      const sourceUrl = item.getURL()
      const downloadKey = extractDownloadKey(sourceUrl)
      const filename = item.getFilename()
      const jobId =
        pendingDownloads.get(sourceUrl) ||
        (downloadKey ? pendingDownloadKeys.get(downloadKey) : undefined) ||
        Array.from(jobs.values()).find((job) => {
          if (job.status !== 'pending' || !job.expectedUrl) {
            return false
          }
          const expectedKey = extractDownloadKey(job.expectedUrl)
          if (downloadKey && expectedKey && downloadKey === expectedKey) {
            return true
          }
          return Boolean(
            filename &&
              (job.filename === filename ||
                job.destPath.endsWith(`/${filename}`) ||
                job.destPath.endsWith(`\\${filename}`))
          )
        })?.id

      if (!jobId) {
        return
      }

      for (const [key, value] of Array.from(pendingDownloads.entries())) {
        if (value === jobId) {
          pendingDownloads.delete(key)
        }
      }
      for (const [key, value] of Array.from(pendingDownloadKeys.entries())) {
        if (value === jobId) {
          pendingDownloadKeys.delete(key)
        }
      }
      const job = jobs.get(jobId)
      if (!job) {
        item.cancel()
        return
      }

      if (existsSync(job.destPath)) {
        rmSync(job.destPath, { force: true })
      }
      mkdirSync(dirname(job.destPath), { recursive: true })
      item.setSavePath(job.destPath)

      job.status = 'downloading'
      refreshHelperThrottling()
      job.filename = item.getFilename() || job.filename
      job.totalBytes = item.getTotalBytes() > 0 ? item.getTotalBytes() : job.totalBytes
      job.lastBytes = 0
      job.lastTickAt = Date.now()
      job.lastProgressAt = Date.now()
      job.smoothedSpeedBps = 0
      job.startResolve?.()
      job.startResolve = undefined
      job.startReject = undefined

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
            ? 'Download cancelado pelo navegador do TeraBox.'
            : 'O navegador do TeraBox interrompeu o download antes da conclusão.'
      })
    })
  }

  async function computeDownloadInfoFromOwnPage(): Promise<{
    dlink: string
    totalBytes: number
    filename?: string
  }> {
    const win = getWindow()
    const info = await win.webContents.executeJavaScript(
      `(() => {
        const customBase64 = ${customBase64.toString()}
        const html = document.documentElement.outerHTML
        const jsTokenMatch = html.match(/fn%28%22([A-F0-9]{32,})%22%29/i)
        const bdstokenMatch = html.match(/bdstoken":"([^"]+)"/)
        const jsToken = jsTokenMatch ? jsTokenMatch[1] : ''
        const bdstoken = bdstokenMatch ? bdstokenMatch[1] : ''
        const url = new URL(location.href)
        const fsid = url.searchParams.get('fsid') || ''
        const build = async () => {
          const homeInfoParams = new URLSearchParams({
            app_id: '250528',
            web: '1',
            channel: 'dubox',
            clienttype: '0',
            jsToken,
            'dp-logid': String(Date.now()),
          })
          const homeInfo = await fetch('/api/home/info?' + homeInfoParams.toString(), {
            credentials: 'include',
            headers: {
              Accept: 'application/json, text/plain, */*',
              'X-Requested-With': 'XMLHttpRequest',
            },
          }).then((response) => response.json())

          const data = homeInfo.data || homeInfo
          const signSource = String(data.sign2 || '').trim().replace(/;\\s*$/, '')
          let signFn
          try {
            signFn = new Function('return ' + signSource)()
          } catch {
            signFn = new Function('return (' + signSource + ')')()
          }
          const rawSign = signFn(data.sign3, data.sign1)
          const sign = customBase64(rawSign)

          const downloadParams = new URLSearchParams({
            app_id: '250528',
            web: '1',
            channel: 'dubox',
            clienttype: '0',
            jsToken,
            'dp-logid': String(Date.now() + 1),
            fidlist: JSON.stringify([fsid]),
            type: 'dlink',
            vip: '2',
            sign,
            timestamp: String(data.timestamp),
            need_speed: '0',
            bdstoken,
          })

          const downloadInfo = await fetch('/api/download?' + downloadParams.toString(), {
            credentials: 'include',
            headers: {
              Accept: 'application/json, text/plain, */*',
              'X-Requested-With': 'XMLHttpRequest',
            },
          }).then((response) => response.json())

          const dlink = downloadInfo?.dlink?.[0]?.dlink || ''
          const totalBytes = Number(downloadInfo?.dlink?.[0]?.size || downloadInfo?.file_info?.[0]?.size || 0)
          const filename =
            downloadInfo?.dlink?.[0]?.server_filename ||
            downloadInfo?.file_info?.[0]?.server_filename ||
            ''

          return { dlink, totalBytes, filename }
        }

        return build()
      })()`,
      true
    )

    if (!info?.dlink) {
      throw new Error('O TeraBox não retornou o link final de download após salvar o arquivo na conta.')
    }

    return {
      dlink: String(info.dlink),
      totalBytes: Number(info.totalBytes || 0),
      filename: typeof info.filename === 'string' && info.filename ? info.filename : undefined,
    }
  }

  async function cleanupTemporaryCloudCopy(): Promise<void> {
    const win = getWindow()
    const result = await win.webContents.executeJavaScript(
      `(() => {
        const readBdstoken = () => {
          const html = document.documentElement.outerHTML
          const match = html.match(/bdstoken":"([^"]+)"/)
          return match ? match[1] : ''
        }

        const readFsid = () => {
          try {
            return new URL(location.href).searchParams.get('fsid') || ''
          } catch {
            return ''
          }
        }

        const requestJson = async (url, init) => {
          const response = await fetch(url, {
            credentials: 'include',
            headers: {
              Accept: 'application/json, text/plain, */*',
              'X-Requested-With': 'XMLHttpRequest',
              ...(init?.headers || {}),
            },
            ...init,
          })
          return response.json()
        }

        return (async () => {
          const fsid = readFsid()
          const bdstoken = readBdstoken()
          if (!fsid || !bdstoken) {
            return { ok: false, error: 'fsid ou bdstoken ausente' }
          }

          const base = new URLSearchParams({
            app_id: '250528',
            web: '1',
            channel: 'dubox',
            clienttype: '0',
            bdstoken,
          })

          const metaParams = new URLSearchParams(base)
          metaParams.set('fsids', JSON.stringify([fsid]))
          metaParams.set('dlink', '1')

          const meta = await requestJson('/api/filemetas?' + metaParams.toString())
          const entry = meta?.info?.[0] || meta?.list?.[0] || meta?.data?.[0] || null
          const path = entry?.path || (entry?.server_filename ? '/' + entry.server_filename : '')

          if (!path) {
            return { ok: false, error: 'caminho temporário não encontrado', meta }
          }

          const deleteParams = new URLSearchParams(base)
          deleteParams.set('async', '2')
          deleteParams.set('onnest', 'fail')
          deleteParams.set('newVerify', '1')
          deleteParams.set('opera', 'delete')

          const body = new URLSearchParams({
            filelist: JSON.stringify([path]),
          })

          const deleted = await requestJson('/api/filemanager?' + deleteParams.toString(), {
            method: 'POST',
            headers: {
              'Content-Type': 'application/x-www-form-urlencoded; charset=UTF-8',
            },
            body: body.toString(),
          })

          if (deleted?.errno === 0 || deleted?.taskid || deleted?.request_id) {
            return { ok: true, path }
          }

          return { ok: false, error: deleted?.errmsg || deleted?.msg || 'remoção rejeitada', path, deleted }
        })()
      })()`,
      true
    )

    if (!result?.ok) {
      throw new Error(
        typeof result?.error === 'string' && result.error
          ? result.error
          : 'Não foi possível limpar a cópia temporária salva na conta.'
      )
    }
  }

  async function collectShareFilesFromDom(
    baseUrl: string,
    currentPath: string | null,
    relativePrefix = ''
  ): Promise<Record<string, unknown>[]> {
    await loadSharePage(baseUrl, currentPath)
    const { items } = await readCurrentShareEntries()
    const files: Record<string, unknown>[] = []

    for (const item of items) {
      if (item.isDir) {
        const nextPath = joinSharePath(currentPath, item.filename)
        const nextPrefix = relativePrefix ? `${relativePrefix}/${item.filename}` : item.filename
        files.push(...(await collectShareFilesFromDom(baseUrl, nextPath, nextPrefix)))
        continue
      }

      const relPath = relativePrefix ? `${relativePrefix}/${item.filename}` : item.filename
      files.push({
        filename: item.filename,
        size: parseByteSize(item.sizeText),
        mimeType: undefined,
        isFolder: false,
        path: relPath,
        sourceUrl: buildShareUrl(baseUrl, currentPath, item.filename),
      })
    }

    return files
  }

  async function getFileInfo(url: string): Promise<Record<string, unknown>> {
    if (url.includes('/sharing/videoPlay')) {
      const win = getWindow()
      await ensureAuthenticated()
      await win.loadURL(cleanTeraboxUrl(url))
      await delay(6000)
      const info = await win.webContents.executeJavaScript(
        `(() => {
          const filename =
            (document.querySelector('.file-name-info .file-name')?.innerText || '').trim() ||
            (document.querySelector('.file-name')?.innerText || '').trim()
          const body = document.body.innerText || ''
          const sizeMatch = body.match(/tamanho do arquivo:\\s*([\\d.,]+\\s*[kmgt]?b)/i)
          return {
            filename,
            sizeText: sizeMatch ? sizeMatch[1] : '',
          }
        })()`,
        true
      )

      const filename = typeof info?.filename === 'string' && info.filename ? info.filename : 'arquivo_terabox'
      const sizeText = typeof info?.sizeText === 'string' ? info.sizeText : ''
      return {
        filename,
        size: parseByteSize(sizeText),
        mime_type: undefined,
        is_folder: false,
        children: undefined,
      }
    }

    const requestedPath = extractDir(url)
    await loadSharePage(url, requestedPath)
    const { shareName, items } = await readCurrentShareEntries()

    if (items.length === 0) {
      throw new Error('Compartilhamento do TeraBox vazio.')
    }

    if (requestedPath) {
      const children = await collectShareFilesFromDom(url, requestedPath)
      const totalSize = children.reduce((sum, child) => sum + Number(child.size ?? 0), 0)
      return {
        filename: basenamePath(requestedPath) || shareName || 'pasta_terabox',
        size: totalSize,
        mime_type: undefined,
        is_folder: true,
        children,
      }
    }

    if (items.length === 1 && !items[0].isDir) {
      return {
        filename: items[0].filename,
        size: parseByteSize(items[0].sizeText),
        mime_type: undefined,
        is_folder: false,
        children: undefined,
      }
    }

    const rootPath = items.length === 1 && items[0].isDir ? `/${items[0].filename}` : null
    const children = await collectShareFilesFromDom(url, rootPath)
    const totalSize = children.reduce((sum, child) => sum + Number(child.size ?? 0), 0)
    const folderName = rootPath ? basenamePath(rootPath) : shareName || 'pasta_terabox'

    return {
      filename: folderName,
      size: totalSize,
      mime_type: undefined,
      is_folder: true,
      children,
    }
  }

  async function beginBrowserDownload(jobId: string, downloadUrl: string): Promise<void> {
    const win = getWindow()
    const job = jobs.get(jobId)
    if (!job) {
      throw new Error('Job do TeraBox não encontrado ao iniciar o download.')
    }

    await new Promise<void>((resolve, reject) => {
      let settled = false
      const timeout = setTimeout(() => {
        if (settled) return
        settled = true
        pendingDownloads.delete(downloadUrl)
        const key = extractDownloadKey(downloadUrl)
        if (key) {
          pendingDownloadKeys.delete(key)
        }
        reject(new Error('O TeraBox não iniciou o download no navegador dentro do tempo esperado.'))
      }, 20_000)

      job.startResolve = () => {
        if (settled) return
        settled = true
        clearTimeout(timeout)
        resolve()
      }
      job.startReject = (error: Error) => {
        if (settled) return
        settled = true
        clearTimeout(timeout)
        reject(error)
      }
      job.expectedUrl = downloadUrl
      pendingDownloads.set(downloadUrl, jobId)
      const key = extractDownloadKey(downloadUrl)
      if (key) {
        pendingDownloadKeys.set(key, jobId)
      }
      win.webContents.downloadURL(downloadUrl)
    })
  }

  async function runDownload(jobId: string, url: string): Promise<void> {
    const job = jobs.get(jobId)
    if (!job) {
      return
    }

    let createdTemporaryCloudCopy = false

    try {
      await ensureAuthenticated()
      const win = getWindow()
      const syntheticFileName = extractSyntheticFileName(url)
      const syntheticFolderPath = extractDir(url)
      await win.loadURL(buildShareUrl(url, syntheticFolderPath, null))
      await delay(6000)

      if (syntheticFileName) {
        await waitForShareItems()
        await openSharedFileFromCurrentFolder(syntheticFileName)
        await delay(4000)
      }

      let onOwnPage = win.webContents.getURL().includes('/main?')
      if (!onOwnPage) {
        const openedExisting = await clickByText(['Ver Agora', 'View Now'])
        if (!openedExisting) {
          const clickedSave = await clickByText(['Salvar', 'Save'])
          if (!clickedSave) {
            throw new Error('O botão de salvar do TeraBox não apareceu para este arquivo.')
          }

          createdTemporaryCloudCopy = true
          await delay(2500)
          await clickByText(['Confirmar', 'Confirm'])
          const ready = await waitFor<boolean>(
            `location.href.includes('/main?') ||
             Array.from(document.querySelectorAll('button, a, div, span'))
               .some((node) => ['ver agora', 'view now'].includes((node.innerText || '').trim().toLowerCase()))`,
            25_000
          )
          if (!ready) {
            throw new Error('O TeraBox não confirmou o salvamento do arquivo na conta.')
          }

          onOwnPage = win.webContents.getURL().includes('/main?')
          if (!onOwnPage) {
            const openedNow = await clickByText(['Ver Agora', 'View Now'])
            if (!openedNow) {
              throw new Error('O TeraBox não ofereceu o atalho para abrir o arquivo salvo.')
            }
          }
        }
      }

      const ownPage = await waitFor<boolean>(
        `location.href.includes('/main?') && location.href.includes('fsid=')`,
        25_000
      )
      if (!ownPage) {
        throw new Error('O TeraBox não abriu a página do arquivo salvo na conta.')
      }

      const downloadInfo = await computeDownloadInfoFromOwnPage()
      job.totalBytes = downloadInfo.totalBytes || job.totalBytes
      job.filename = downloadInfo.filename || job.filename
      job.shouldCleanupCloudCopy = createdTemporaryCloudCopy

      await beginBrowserDownload(jobId, downloadInfo.dlink)

      if (job.shouldCleanupCloudCopy) {
        try {
          await cleanupTemporaryCloudCopy()
        } catch (cleanupError) {
          logMain('terabox', 'Falha ao limpar cópia temporária da conta', cleanupError)
        } finally {
          job.shouldCleanupCloudCopy = false
        }
      }
    } catch (error) {
      if (createdTemporaryCloudCopy) {
        try {
          await cleanupTemporaryCloudCopy()
        } catch (cleanupError) {
          logMain('terabox', 'Falha ao limpar cópia temporária após erro', cleanupError)
        } finally {
          job.shouldCleanupCloudCopy = false
        }
      }

      job.status = 'error'
      refreshHelperThrottling()
      job.error = error instanceof Error ? error.message : String(error)
      job.speedBps = 0
      job.etaSecs = 0
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
    void runDownload(jobId, url)
    return jobId
  }

  async function login(params: Record<string, string>): Promise<boolean> {
    const current = currentAccount()
    const email = params.email?.trim() || current?.email || ''
    const password = params.password || current?.password || ''
    if (!email || !password) {
      throw new Error('Informe e-mail e senha do TeraBox para validar a conta.')
    }
    persistAccount({
      email,
      password,
      cookies: current?.cookies ?? [],
      verifiedAt: current?.verifiedAt,
    })
    await ensureAuthenticated()
    return true
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
    const account = currentAccount()
    if (account?.verifiedAt && account.email) {
      return true
    }
    return sessionLooksAuthenticated()
  }

  function accountInfo(): { email: string; verifiedAt?: string } | null {
    const account = currentAccount()
    if (!account?.email) {
      return null
    }
    return {
      email: account.email,
      verifiedAt: account.verifiedAt,
    }
  }

  async function handleAction(body: Record<string, unknown>): Promise<unknown> {
    if (body.action === 'terabox_file_info') {
      const url = typeof body.url === 'string' ? body.url : ''
      if (!url) {
        throw new Error('Ação do TeraBox sem URL para leitura de pasta/arquivo.')
      }
      return getFileInfo(url)
    }

    if (body.action === 'terabox_download_file') {
      const url = typeof body.url === 'string' ? body.url : ''
      const destPath = typeof body.destPath === 'string' ? body.destPath : ''
      if (!url || !destPath) {
        throw new Error('Ação do TeraBox sem URL ou destino.')
      }
      return {
        jobId: startDownload(url, destPath),
      }
    }

    if (body.action === 'terabox_job_status') {
      const jobId = typeof body.jobId === 'string' ? body.jobId : ''
      const job = jobs.get(jobId)
      if (!job) {
        return {
          status: 'error',
          error: 'Job do TeraBox não encontrado.',
        }
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

    throw new Error('Ação do proxy TeraBox não suportada.')
  }

  return {
    accountInfo,
    handleAction,
    isLoggedIn,
    login,
    logout,
  }
}
