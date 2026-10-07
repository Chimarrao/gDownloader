// Tentativa automática de resolver reCAPTCHA v2 / hCaptcha pelo desafio de
// ÁUDIO (acessibilidade), transcrito localmente (ver audio-captcha-service.ts)
// — sem API paga, sem solver de terceiros. Roda ANTES de mostrar a janela
// manual pro usuário; se falhar em qualquer passo, o fluxo manual existente
// em captcha-window-service.ts assume normalmente (nada quebra).
//
// Tecnicamente precisa de BrowserWindow.webContents.mainFrame.framesInSubtree
// pra executar JS DIRETO dentro dos iframes do widget (checkbox e desafio são
// cross-origin — google.com / hcaptcha.com — um executeJavaScript comum no
// frame principal não alcança o conteúdo deles).
import type { BrowserWindow, WebFrameMain } from 'electron'
import { createWriteStream, unlinkSync } from 'fs'
import * as http from 'http'
import * as https from 'https'
import { tmpdir } from 'os'
import { join } from 'path'
import { transcribeAudio } from './audio-captcha-service'

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

function findFrame(win: BrowserWindow, matches: (url: string) => boolean): WebFrameMain | null {
  try {
    const frames = win.webContents.mainFrame.framesInSubtree
    return frames.find((f) => matches(f.url)) ?? null
  } catch {
    return null
  }
}

function downloadToFile(url: string, destPath: string): Promise<void> {
  return new Promise((resolve, reject) => {
    const follow = (currentUrl: string, redirectsLeft: number): void => {
      if (redirectsLeft <= 0) {
        reject(new Error('Muitos redirecionamentos ao baixar áudio do captcha'))
        return
      }
      const mod = currentUrl.startsWith('https:') ? https : http
      mod
        .get(currentUrl, { headers: { 'User-Agent': 'Mozilla/5.0 (Windows NT 10.0; Win64; x64)' } }, (res) => {
          const status = res.statusCode ?? 0
          if ([301, 302, 303, 307, 308].includes(status) && res.headers.location) {
            res.resume()
            follow(new URL(res.headers.location, currentUrl).toString(), redirectsLeft - 1)
            return
          }
          if (status < 200 || status >= 300) {
            res.resume()
            reject(new Error(`HTTP ${status} ao baixar áudio do captcha`))
            return
          }
          const file = createWriteStream(destPath)
          res.pipe(file)
          file.on('finish', () => file.close(() => resolve()))
          file.on('error', reject)
        })
        .on('error', reject)
    }
    follow(url, 5)
  })
}

function normalizeTranscript(text: string): string {
  return text
    .toLowerCase()
    .replace(/[^a-z0-9 ]/g, '')
    .replace(/\s+/g, ' ')
    .trim()
}

async function solveViaAudio(opts: {
  win: BrowserWindow
  userDataPath: string
  checkboxFrame: () => WebFrameMain | null
  challengeFrame: () => Promise<WebFrameMain | null>
  checkboxClickJs: string
  checkboxCheckedJs: string
  audioButtonClickJs: string
  audioSourceJs: string
  answerInputJs: (value: string) => string
  verifyClickJs: string
}): Promise<boolean> {
  const anchor = opts.checkboxFrame()
  if (!anchor) return false

  await anchor.executeJavaScript(opts.checkboxClickJs).catch(() => undefined)
  await delay(2000)

  const alreadyChecked = await anchor.executeJavaScript(opts.checkboxCheckedJs).catch(() => false)
  if (alreadyChecked) {
    // Passou sem desafio — o token já deve estar no textarea principal;
    // quem chamou (captcha-window-service) vai pegar no próximo poll.
    return true
  }

  let challenge: WebFrameMain | null = null
  for (let i = 0; i < 12 && !challenge; i++) {
    challenge = await opts.challengeFrame()
    if (!challenge) await delay(500)
  }
  if (!challenge) return false

  const clickedAudio = await challenge.executeJavaScript(opts.audioButtonClickJs).catch(() => false)
  if (!clickedAudio) return false
  await delay(1800)

  let audioUrl: string | null = null
  for (let i = 0; i < 12 && !audioUrl; i++) {
    audioUrl = (await challenge.executeJavaScript(opts.audioSourceJs).catch(() => null)) as string | null
    if (!audioUrl) await delay(500)
  }
  if (!audioUrl) return false

  const audioPath = join(tmpdir(), `gdl-captcha-audio-${Date.now()}.mp3`)
  try {
    await downloadToFile(audioUrl, audioPath)
    const transcript = await transcribeAudio(opts.userDataPath, audioPath)
    if (!transcript) return false
    const answer = normalizeTranscript(transcript)
    if (!answer) return false

    await challenge.executeJavaScript(opts.answerInputJs(answer)).catch(() => undefined)
    await delay(300)
    await challenge.executeJavaScript(opts.verifyClickJs).catch(() => false)
    await delay(2000)
    return true
  } finally {
    try { unlinkSync(audioPath) } catch { /* arquivo temporário, falha de limpeza não é crítica */ }
  }
}

export async function tryAutoSolveRecaptcha(win: BrowserWindow, userDataPath: string): Promise<boolean> {
  const isAnchor = (u: string): boolean => u.includes('/recaptcha/api2/anchor') || u.includes('/recaptcha/enterprise/anchor')
  const isBframe = (u: string): boolean => u.includes('/recaptcha/api2/bframe') || u.includes('/recaptcha/enterprise/bframe')

  return solveViaAudio({
    win,
    userDataPath,
    checkboxFrame: () => findFrame(win, isAnchor),
    challengeFrame: async () => findFrame(win, isBframe),
    checkboxClickJs: `document.querySelector('#recaptcha-anchor')?.click()`,
    checkboxCheckedJs: `document.querySelector('#recaptcha-anchor')?.getAttribute('aria-checked') === 'true'`,
    audioButtonClickJs: `
      (() => {
        const btn = document.querySelector('#recaptcha-audio-button')
        if (btn instanceof HTMLElement) { btn.click(); return true }
        return false
      })()
    `,
    audioSourceJs: `
      (() => {
        const src = document.querySelector('#audio-source')
        return src instanceof HTMLAudioElement ? src.src : null
      })()
    `,
    answerInputJs: (value) => `
      (() => {
        const input = document.querySelector('#audio-response')
        if (input instanceof HTMLInputElement) {
          input.value = ${JSON.stringify(value)}
          input.dispatchEvent(new Event('input', { bubbles: true }))
          input.dispatchEvent(new Event('change', { bubbles: true }))
        }
      })()
    `,
    verifyClickJs: `
      (() => {
        const btn = document.querySelector('#recaptcha-verify-button')
        if (btn instanceof HTMLElement) { btn.click(); return true }
        return false
      })()
    `,
  })
}

// hCaptcha: selectors best-effort — a estrutura do desafio deles é menos
// documentada/estável publicamente que a do reCAPTCHA (que praticamente não
// muda há anos). Não testado contra uma instância real ainda (nenhuma
// pendente no momento desta implementação); se os seletores não baterem,
// a função simplesmente retorna false e o fluxo manual assume normalmente.
export async function tryAutoSolveHcaptcha(win: BrowserWindow, userDataPath: string): Promise<boolean> {
  const isCheckboxFrame = (u: string): boolean => u.includes('hcaptcha.com') && u.includes('frame=checkbox')
  const isChallengeFrame = (u: string): boolean => u.includes('hcaptcha.com') && u.includes('frame=challenge')

  return solveViaAudio({
    win,
    userDataPath,
    checkboxFrame: () => findFrame(win, isCheckboxFrame),
    challengeFrame: async () => findFrame(win, isChallengeFrame),
    checkboxClickJs: `document.querySelector('#checkbox')?.click()`,
    checkboxCheckedJs: `document.querySelector('#checkbox')?.getAttribute('aria-checked') === 'true'`,
    audioButtonClickJs: `
      (() => {
        const btn = document.querySelector('.challenge-icon[aria-label*="audio" i], .accessibility-challenge-button, button[aria-label*="audio" i]')
        if (btn instanceof HTMLElement) { btn.click(); return true }
        return false
      })()
    `,
    audioSourceJs: `
      (() => {
        const el = document.querySelector('audio#sound-clip, audio.audio-challenge-player, audio')
        return el instanceof HTMLAudioElement ? el.src : null
      })()
    `,
    answerInputJs: (value) => `
      (() => {
        const input = document.querySelector('#audio-response, input[name="audio-response"], .challenge-input input')
        if (input instanceof HTMLInputElement) {
          input.value = ${JSON.stringify(value)}
          input.dispatchEvent(new Event('input', { bubbles: true }))
          input.dispatchEvent(new Event('change', { bubbles: true }))
        }
      })()
    `,
    verifyClickJs: `
      (() => {
        const btn = document.querySelector('.button-submit, button[aria-label*="verify" i], .submit')
        if (btn instanceof HTMLElement) { btn.click(); return true }
        return false
      })()
    `,
  })
}
