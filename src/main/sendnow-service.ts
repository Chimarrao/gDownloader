import { BrowserWindow, net, session } from 'electron'
import { HOSTER_BROWSER_USER_AGENT, configureHosterSession, configureHosterWindow, delay } from './browser-helper-common'

const SENDNOW_PARTITION = 'persist:sendnow'
const SENDNOW_HOME = 'https://send.now/'

function fileIdFromUrl(rawUrl: string): string {
  const url = new URL(rawUrl)
  if (!['send.now', 'www.send.now'].includes(url.hostname)) {
    throw new Error('URL do Send.now inválida')
  }
  const segments = url.pathname.split('/').filter(Boolean)
  const id = segments[0] === 'd' ? segments[1] : segments[0]
  if (!id || !/^[A-Za-z0-9]{6,}$/.test(id)) {
    throw new Error('Link unitário do Send.now inválido')
  }
  return id
}

// A página de arquivo do Send.now é protegida por Cloudflare, enquanto a pasta
// pública costuma ser acessível diretamente. Esta sessão real do Electron obtém
// a clearance uma vez e captura o redirect temporário sem expor cookies ao Rust.
export function createSendNowService() {
  let helperWindow: BrowserWindow | null = null
  let redirectListenerInstalled = false
  let pendingRedirect: ((url: string) => void) | null = null

  function ensureRedirectCapture(targetSession: Electron.Session): void {
    if (redirectListenerInstalled) return
    redirectListenerInstalled = true
    // O net.request pode seguir 302 internamente em algumas versões do
    // Electron. O WebRequest da mesma sessão vê o redirect antes disso e nos
    // entrega a URL curta do CDN sem interceptar dados do arquivo.
    targetSession.webRequest.onBeforeRedirect((details) => {
      if (!pendingRedirect || !details.url.startsWith(SENDNOW_HOME)) return
      pendingRedirect(details.redirectURL)
    })
  }

  function getWindow(): BrowserWindow {
    if (helperWindow && !helperWindow.isDestroyed()) return helperWindow
    configureHosterSession(SENDNOW_PARTITION)
    helperWindow = new BrowserWindow({
      show: false,
      width: 1180,
      height: 820,
      autoHideMenuBar: true,
      webPreferences: {
        partition: SENDNOW_PARTITION,
        nodeIntegration: false,
        contextIsolation: true,
        sandbox: true,
        backgroundThrottling: false,
      },
    })
    configureHosterWindow(helperWindow, SENDNOW_PARTITION)
    helperWindow.webContents.setWindowOpenHandler(() => ({ action: 'deny' }))
    helperWindow.on('closed', () => { helperWindow = null })
    return helperWindow
  }

  async function resolve(sourceUrl: string): Promise<{
    url?: string
    cookieHeader?: string
    userAgent: string
    downloadId: string
    rand: string
    referer: string
  }> {
    const id = fileIdFromUrl(sourceUrl)
    const win = getWindow()
    const navigation = win.loadURL(sourceUrl)
    const loaded = await Promise.race([
      navigation.then(() => true),
      delay(15_000).then(() => false),
    ])
    if (!loaded) {
      win.show()
      win.focus()
      throw new Error(
        'Send.now demorou para concluir a verificação no navegador. Conclua-a na janela aberta e tente novamente.',
      )
    }
    // O desafio do Cloudflare nas páginas de arquivo do Send.now é um Turnstile de
    // verdade (não um JS-check de 1-2s): confirmado via resposta real do host
    // (cf-mitigated: challenge, CSP liberando challenges.cloudflare.com). Uma
    // espera fixa curta lia os campos do formulário antes do desafio terminar,
    // enviava o POST com id/rand vazios e o host respondia sem redirect — isso
    // sempre virava "Send.now não retornou o link temporário", que o backend
    // trata como limite de taxa e fica reagendando pra sempre. Por isso aqui
    // fazemos polling até o desafio sumir e o campo "id" do formulário aparecer
    // preenchido, em vez de confiar num timer fixo.
    let formValues: { id?: string; rand?: string; referer?: string } = {}
    const deadline = Date.now() + 20_000
    while (Date.now() < deadline) {
      await delay(700)
      const title = await win.webContents.getTitle()
      if (/just a moment|attention required|verifying you are human/i.test(title)) {
        continue
      }
      formValues = await win.webContents.executeJavaScript(`(() => {
        const value = (name) => document.querySelector('input[name="' + name + '"]')?.value || ''
        return { id: value('id'), rand: value('rand'), referer: value('referer') }
      })()`) as { id?: string; rand?: string; referer?: string }
      if (formValues.id) break
    }
    if (!formValues.id) {
      win.show()
      win.focus()
      // Mesmo marcador usado pelo caminho de 403 abaixo: o backend Rust reconhece
      // esse prefixo e mostra "aguardando confirmação manual" (chip de captcha)
      // com o link da página, em vez de cair no balde genérico de rate-limit e
      // reagendar em loop silencioso.
      throw new Error(`SENDNOW_MANUAL_VERIFICATION_REQUIRED:${sourceUrl}`)
    }
    // O identificador da URL nem sempre é o token usado no formulário do host.
    // Lemos o valor da própria página para não supor que ambos sejam iguais.
    const downloadId = formValues.id && /^[A-Za-z0-9]{6,}$/.test(formValues.id) ? formValues.id : id
    const targetSession = session.fromPartition(SENDNOW_PARTITION)
    ensureRedirectCapture(targetSession)

    return new Promise<{
      url?: string
      cookieHeader?: string
      userAgent: string
      downloadId: string
      rand: string
      referer: string
    }>((resolvePromise, reject) => {
      let settled = false
      const settle = (action: () => void): void => {
        if (settled) return
        settled = true
        pendingRedirect = null
        action()
      }
      const request = net.request({
        url: SENDNOW_HOME,
        method: 'POST',
        session: targetSession,
      })
      const timeout = setTimeout(() => {
        request.abort()
        settle(() => reject(new Error('Send.now não retornou o link temporário a tempo. Tente novamente.')))
      }, 20_000)
      request.setHeader('Content-Type', 'application/x-www-form-urlencoded')
      request.setHeader('Referer', sourceUrl)
      pendingRedirect = (redirectUrl) => {
        request.abort()
        if (redirectUrl.startsWith('https://') || redirectUrl.startsWith('http://')) {
          settle(() => {
            clearTimeout(timeout)
            resolvePromise({
              url: redirectUrl,
              userAgent: HOSTER_BROWSER_USER_AGENT,
              downloadId,
              rand: formValues.rand || '',
              referer: formValues.referer || sourceUrl,
            })
          })
        }
      }
      request.on('redirect', (_status, _method, redirectUrl) => {
        request.abort()
        if (redirectUrl.startsWith('https://') || redirectUrl.startsWith('http://')) {
          settle(() => {
            clearTimeout(timeout)
            resolvePromise({
              url: redirectUrl,
              userAgent: HOSTER_BROWSER_USER_AGENT,
              downloadId,
              rand: formValues.rand || '',
              referer: formValues.referer || sourceUrl,
            })
          })
        } else {
          settle(() => {
            clearTimeout(timeout)
            reject(new Error('Send.now retornou um redirect inválido'))
          })
        }
      })
      request.on('response', (response) => {
        // O 403 do Send.now é a página/sessão de verificação do próprio host,
        // não uma falha transitória de rede. Exibimos a mesma janela que
        // guarda a sessão persistente para que a pessoa conclua a etapa no
        // host. O backend recebe um marcador estável e suspende a fila em vez
        // de repetir POSTs que não podem ter resultado diferente sozinhos.
        if (response.statusCode === 403) {
          win.show()
          win.focus()
          settle(() => {
            clearTimeout(timeout)
            reject(new Error('SENDNOW_MANUAL_VERIFICATION_REQUIRED'))
          })
          return
        }
        if (response.statusCode >= 400) {
          settle(() => {
            clearTimeout(timeout)
            reject(new Error(`Send.now respondeu HTTP ${response.statusCode}; pode ser necessário concluir a verificação no navegador.`))
          })
          return
        }
        // Alguns POPs seguem o redirect antes de entregar a resposta ao
        // ClientRequest. Retornamos a clearance para o backend repetir o POST.
        void targetSession.cookies.get({ url: SENDNOW_HOME }).then((cookies) => {
          const cookieHeader = cookies.map((cookie) => `${cookie.name}=${cookie.value}`).join('; ')
          settle(() => {
            clearTimeout(timeout)
            if (!cookieHeader) {
              reject(new Error('Send.now não retornou o redirect temporário nem uma sessão válida.'))
              return
            }
            resolvePromise({
              cookieHeader,
              userAgent: HOSTER_BROWSER_USER_AGENT,
              downloadId,
              rand: formValues.rand || '',
              referer: formValues.referer || sourceUrl,
            })
          })
        }).catch((error) => settle(() => {
          clearTimeout(timeout)
          reject(error)
        }))
      })
      request.on('error', (error) => settle(() => {
        clearTimeout(timeout)
        reject(error)
      }))
      request.write([
        'op=download2',
        `id=${encodeURIComponent(downloadId)}`,
        `rand=${encodeURIComponent(formValues.rand || '')}`,
        `referer=${encodeURIComponent(formValues.referer || sourceUrl)}`,
        'method_free=',
        'method_premium=',
      ].join('&'))
      request.end()
    })
  }

  async function handleAction(payload: { action?: string; url?: string }): Promise<unknown> {
    if (payload.action !== 'sendnow_resolve' || !payload.url) {
      throw new Error('Ação Send.now inválida')
    }
    return resolve(payload.url)
  }

  return { handleAction }
}
