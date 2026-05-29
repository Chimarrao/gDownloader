import { BrowserWindow, shell } from 'electron'
import { configureHosterSession, configureHosterWindow, delay } from './browser-helper-common'
import { logMain } from './debug-log'

const CAPTCHA_PARTITION = 'persist:captcha-helper'
const KATFILE_PARTITION = 'persist:katfile'
export const BROWSER_SESSION_READY_TOKEN = '__gdownloader_browser_session_ready__'

type CaptchaProvider = 'rapidgator' | 'katfile' | 'unknown'

export interface ManualCaptchaRequest {
  provider?: string
  pageUrl: string
  sourceUrl?: string
}

interface CaptchaPageState {
  token: string
  hasCaptcha: boolean
  hasHostDownloadSurface: boolean
}

function normalizeProvider(raw: string | undefined): CaptchaProvider {
  switch ((raw ?? '').trim().toLowerCase()) {
    case 'rapidgator':
      return 'rapidgator'
    case 'katfile':
      return 'katfile'
    default:
      return 'unknown'
  }
}

function createWindow(provider: CaptchaProvider): BrowserWindow {
  const partition = provider === 'katfile' ? KATFILE_PARTITION : CAPTCHA_PARTITION
  configureHosterSession(partition)
  const parent =
    BrowserWindow.getFocusedWindow()
    ?? BrowserWindow.getAllWindows().find((candidate) => !candidate.isDestroyed())
    ?? undefined

  const win = new BrowserWindow({
    width: 520,
    height: 640,
    minWidth: 420,
    minHeight: 520,
    show: false,
    modal: Boolean(parent),
    parent,
    autoHideMenuBar: true,
    title: 'Resolver captcha',
    webPreferences: {
      partition,
      contextIsolation: true,
      sandbox: false,
      backgroundThrottling: false,
    },
  })

  configureHosterWindow(win, partition)
  win.webContents.setWindowOpenHandler(({ url }) => {
    void shell.openExternal(url)
    return { action: 'deny' }
  })

  return win
}

async function readCaptchaState(win: BrowserWindow): Promise<CaptchaPageState> {
  return (await win.webContents.executeJavaScript(
    `(() => {
      const selectors = [
        'textarea[name="g-recaptcha-response"]',
        'textarea[name="h-captcha-response"]',
        'textarea[name="cf-turnstile-response"]',
        'input[name="g-recaptcha-response"]',
        'input[name="h-captcha-response"]',
        'input[name="cf-turnstile-response"]'
      ]

      let token = ''
      for (const selector of selectors) {
        const node = document.querySelector(selector)
        const value =
          String(
            node instanceof HTMLTextAreaElement || node instanceof HTMLInputElement
              ? node.value
              : node?.textContent || ''
          )
            .trim()
        if (value.length >= 20) {
          token = value
          break
        }
      }

      const blob = (
        (document.title || '') + '\\n'
        + (document.body?.innerText || '') + '\\n'
        + Array.from(document.querySelectorAll('iframe')).map((frame) => frame.src || '').join('\\n')
      ).toLowerCase()
      const hasChallengeWidget = Boolean(document.querySelector(
        '.cf-turnstile, iframe[src*="turnstile"], iframe[src*="challenges.cloudflare.com"], iframe[src*="recaptcha"], iframe[src*="hcaptcha"]'
      ))
      const hasCloudflareInterstitial =
        /just a moment|checking your browser|verify you are human|verifique se voce e humano|verifique se você é humano/i.test(blob)

      return {
        token,
        hasCaptcha:
          hasChallengeWidget
          || hasCloudflareInterstitial
          || blob.includes('i am human')
          || blob.includes('robot'),
        hasHostDownloadSurface: Boolean(
          document.querySelector('#fbtn1, #m_fbtn1, #freebtn, #Send, form#btn_download, form#_mform, form[name="F1"]')
        )
      }
    })()`,
    true
  )) as CaptchaPageState
}

async function clickLikeHuman(win: BrowserWindow, selector: string): Promise<boolean> {
  return (await win.webContents.executeJavaScript(
    `(() => {
      const node = document.querySelector(${JSON.stringify(selector)})
      if (!(node instanceof HTMLElement)) {
        return false
      }

      node.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }))
      node.dispatchEvent(new MouseEvent('mouseup', { bubbles: true }))
      node.dispatchEvent(new MouseEvent('click', { bubbles: true }))
      if (typeof node.click === 'function') {
        node.click()
      }
      return true
    })()`,
    true
  )) as boolean
}

async function submitForm(win: BrowserWindow, selector: string): Promise<boolean> {
  return (await win.webContents.executeJavaScript(
    `(() => {
      const form = document.querySelector(${JSON.stringify(selector)})
      if (!(form instanceof HTMLFormElement)) {
        return false
      }
      form.submit()
      return true
    })()`,
    true
  )) as boolean
}

async function focusCaptchaSurface(win: BrowserWindow): Promise<void> {
  await win.webContents.executeJavaScript(
    `(() => {
      const styleId = 'gdl-captcha-focus-style'
      if (!document.getElementById(styleId)) {
        const style = document.createElement('style')
        style.id = styleId
        style.textContent = \`
          body.gdl-captcha-focus {
            min-height: 100vh !important;
            background: #f8fafc !important;
          }
          body.gdl-captcha-focus > *:not(.gdl-captcha-shell) {
            display: none !important;
          }
          .gdl-captcha-shell {
            min-height: 100vh;
            display: flex;
            align-items: center;
            justify-content: center;
            padding: 22px;
            font-family: system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
          }
          .gdl-captcha-box {
            max-width: 420px;
            width: 100%;
            display: grid;
            justify-items: center;
            gap: 12px;
          }
        \`
        document.head.appendChild(style)
      }

      const captcha =
        document.querySelector('.cf-turnstile')
        || document.querySelector('.g-recaptcha')
        || document.querySelector('.h-captcha')
        || document.querySelector('iframe[src*="turnstile"]')
        || document.querySelector('iframe[src*="challenges.cloudflare.com"]')
        || document.querySelector('iframe[src*="recaptcha"]')
        || document.querySelector('iframe[src*="hcaptcha"]')

      if (!captcha) {
        return false
      }

      let shell = document.querySelector('.gdl-captcha-shell')
      if (!shell) {
        shell = document.createElement('div')
        shell.className = 'gdl-captcha-shell'
        const box = document.createElement('div')
        box.className = 'gdl-captcha-box'
        shell.appendChild(box)
        document.body.appendChild(shell)
      }

      const box = shell.querySelector('.gdl-captcha-box') || shell
      const host = captcha.closest('.cf-turnstile, .g-recaptcha, .h-captcha') || captcha
      if (host.parentElement !== box) {
        box.appendChild(host)
      }
      document.body.classList.add('gdl-captcha-focus')
      ;(host instanceof HTMLElement ? host : box).scrollIntoView({ block: 'center', inline: 'center' })
      return true
    })()`,
    true,
  ).catch(() => false)
}

async function prepareKatfilePage(win: BrowserWindow): Promise<void> {
  const clicked =
    await clickLikeHuman(win, '#fbtn1, button#fbtn1, input[name="method_free"], button[name="method_free"]').catch(() => false)
  if (!clicked) {
    await submitForm(win, 'form#btn_download, form[name="F1"], form').catch(() => false)
  }
}

async function advanceKatfileIfSolved(win: BrowserWindow): Promise<boolean> {
  return (await win.webContents.executeJavaScript(
    `(() => {
      const tokenNodes = Array.from(document.querySelectorAll(
        'textarea[name="g-recaptcha-response"], input[name="g-recaptcha-response"], textarea[name="h-captcha-response"], input[name="h-captcha-response"], textarea[name="cf-turnstile-response"], input[name="cf-turnstile-response"]'
      ))
      const token = tokenNodes
        .map((node) => (
          node instanceof HTMLTextAreaElement || node instanceof HTMLInputElement
            ? node.value.trim()
            : ''
        ))
        .find((value) => value.length >= 20) || ''
      if (token.length < 20) {
        return false
      }

      const form = document.querySelector('form#_mform') || document.querySelector('form[name="F1"]')
      const flag = '__gdlKatfileSubmitted'
      if ((window)[flag]) {
        return false
      }
      ;(window)[flag] = true
      const submitButton =
        document.querySelector('#freebtn')
        || document.querySelector('#Send')
        || document.querySelector('#downloadbtn')
        || document.querySelector('button[type="submit"]')
        || document.querySelector('input[type="submit"]')
      if (submitButton instanceof HTMLElement) {
        submitButton.click()
      } else if (form instanceof HTMLFormElement) {
        form.submit()
      } else {
        return false
      }
      return true
    })()`,
    true
  )) as boolean
}

async function prepareProviderPage(win: BrowserWindow, request: ManualCaptchaRequest): Promise<void> {
  const provider = normalizeProvider(request.provider)
  if (provider === 'katfile') {
    await delay(700)
    await prepareKatfilePage(win)
  }
}

// eslint-disable-next-line @typescript-eslint/explicit-function-return-type
export function createCaptchaWindowService() {
  async function solve(request: ManualCaptchaRequest): Promise<string | null> {
    const startUrl = request.pageUrl || request.sourceUrl || ''
    if (!startUrl) {
      throw new Error('Captcha sem URL de origem.')
    }

    logMain('captcha-window', 'Abrindo resolvedor manual', {
      provider: request.provider,
      pageUrl: request.pageUrl,
      sourceUrl: request.sourceUrl,
    })

    const provider = normalizeProvider(request.provider)
    const win = createWindow(provider)
    let closedByUser = false
    win.once('closed', () => {
      closedByUser = true
      logMain('captcha-window', 'Janela manual encerrada pelo usuário')
    })

    await win.loadURL(startUrl)
    await prepareProviderPage(win, request).catch(() => undefined)
    await focusCaptchaSurface(win).catch(() => undefined)
    win.show()
    win.focus()

    const deadline = Date.now() + 8 * 60_000
    while (!closedByUser && !win.isDestroyed() && Date.now() < deadline) {
      if (provider === 'katfile') {
        await advanceKatfileIfSolved(win).catch(() => false)
      }
      const state = await readCaptchaState(win).catch(() => null)
      if (state?.token) {
        logMain('captcha-window', 'Token capturado manualmente', {
          provider: request.provider,
          pageUrl: request.pageUrl,
        })
        if (!win.isDestroyed()) {
          win.close()
        }
        return state.token
      }

      if (provider === 'katfile' && state && !state.hasCaptcha && state.hasHostDownloadSurface) {
        logMain('captcha-window', 'Sessão do navegador liberada pelo Cloudflare', {
          provider: request.provider,
          pageUrl: request.pageUrl,
        })
        if (!win.isDestroyed()) {
          win.close()
        }
        return BROWSER_SESSION_READY_TOKEN
      }

      await delay(500)
    }

    if (!win.isDestroyed()) {
      win.close()
    }
    logMain('captcha-window', 'Resolvedor manual encerrado sem token', {
      provider: request.provider,
      pageUrl: request.pageUrl,
    })
    return null
  }

  return { solve }
}
