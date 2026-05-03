import { BrowserWindow, shell } from 'electron'
import { delay, HOSTER_BROWSER_USER_AGENT } from './browser-helper-common'
import { logMain } from './debug-log'

const CAPTCHA_PARTITION = 'persist:captcha-helper'

type CaptchaProvider = 'brupload' | 'rapidgator' | 'katfile' | 'unknown'

export interface ManualCaptchaRequest {
  provider?: string
  pageUrl: string
  sourceUrl?: string
}

interface CaptchaPageState {
  token: string
  hasCaptcha: boolean
}

function normalizeProvider(raw: string | undefined): CaptchaProvider {
  switch ((raw ?? '').trim().toLowerCase()) {
    case 'brupload':
      return 'brupload'
    case 'rapidgator':
      return 'rapidgator'
    case 'katfile':
      return 'katfile'
    default:
      return 'unknown'
  }
}

function createWindow(): BrowserWindow {
  const parent =
    BrowserWindow.getFocusedWindow()
    ?? BrowserWindow.getAllWindows().find((candidate) => !candidate.isDestroyed())
    ?? undefined

  const win = new BrowserWindow({
    width: 980,
    height: 760,
    minWidth: 820,
    minHeight: 620,
    show: false,
    modal: Boolean(parent),
    parent,
    autoHideMenuBar: true,
    title: 'Resolver captcha',
    webPreferences: {
      partition: CAPTCHA_PARTITION,
      contextIsolation: true,
      sandbox: false,
      backgroundThrottling: false,
    },
  })

  win.webContents.setUserAgent(HOSTER_BROWSER_USER_AGENT)
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

      return {
        token,
        hasCaptcha:
          blob.includes('captcha')
          || blob.includes('turnstile')
          || blob.includes('cloudflare')
          || blob.includes('i am human')
          || blob.includes('robot')
          || blob.includes('verificação')
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

async function prepareBruploadPage(win: BrowserWindow): Promise<void> {
  const state = await readCaptchaState(win).catch(() => null)
  if (state?.hasCaptcha) {
    return
  }

  const clicked =
    await clickLikeHuman(win, 'input[name="method_free"], button[name="method_free"], #downloadbtn').catch(() => false)
  if (!clicked) {
    await submitForm(win, 'form').catch(() => false)
  }
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
      const tokenNode = document.querySelector('textarea[name="cf-turnstile-response"], input[name="cf-turnstile-response"]')
      const token =
        tokenNode instanceof HTMLTextAreaElement || tokenNode instanceof HTMLInputElement
          ? tokenNode.value.trim()
          : ''
      if (token.length < 20) {
        return false
      }

      const form = document.querySelector('form#_mform') || document.querySelector('form[name="F1"]')
      if (!(form instanceof HTMLFormElement)) {
        return false
      }

      const flag = '__gdlKatfileSubmitted'
      if ((window)[flag]) {
        return false
      }
      ;(window)[flag] = true
      form.submit()
      return true
    })()`,
    true
  )) as boolean
}

async function prepareProviderPage(win: BrowserWindow, request: ManualCaptchaRequest): Promise<void> {
  const provider = normalizeProvider(request.provider)
  if (provider === 'brupload') {
    await delay(700)
    await prepareBruploadPage(win)
    return
  }
  if (provider === 'katfile') {
    await delay(700)
    await prepareKatfilePage(win)
  }
}

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

    const win = createWindow()
    let closedByUser = false
    win.once('closed', () => {
      closedByUser = true
      logMain('captcha-window', 'Janela manual encerrada pelo usuário')
    })

    await win.loadURL(startUrl)
    await prepareProviderPage(win, request).catch(() => undefined)
    win.show()
    win.focus()

    const deadline = Date.now() + 8 * 60_000
    while (!closedByUser && !win.isDestroyed() && Date.now() < deadline) {
      const provider = normalizeProvider(request.provider)
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
