import { app, BrowserWindow, net, session } from 'electron'
import { logMain } from './debug-log'
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
  let currentProxyCreds: { user: string; pass: string } | null = null
  let proxyLoginWired = false

  // Sem isso o Rust busca o arquivo de verdade (via http_client(), já usando o
  // proxy da task) mas o Electron resolvia o link temporário pela rede normal
  // (sem proxy nenhum) — o host via o link sendo gerado de um IP e baixado de
  // outro e respondia com uma página de verificação em vez do arquivo. Espelha
  // o mesmo padrão do katfile-service.ts.
  function wireProxyLogin(): void {
    if (proxyLoginWired) return
    proxyLoginWired = true
    app.on('login', (event, webContents, _details, authInfo, callback) => {
      if (!authInfo.isProxy || !currentProxyCreds) return
      if (!helperWindow || webContents !== helperWindow.webContents) return
      event.preventDefault()
      callback(currentProxyCreds.user, currentProxyCreds.pass)
    })
  }

  async function ensureBrowsingProxy(proxyUrl: string | undefined): Promise<void> {
    wireProxyLogin()
    const targetSession = session.fromPartition(SENDNOW_PARTITION)
    if (!proxyUrl) {
      currentProxyCreds = null
      await targetSession.setProxy({ proxyRules: 'direct://' }).catch(() => undefined)
      return
    }
    try {
      const parsed = new URL(proxyUrl)
      currentProxyCreds = parsed.username
        ? { user: decodeURIComponent(parsed.username), pass: decodeURIComponent(parsed.password) }
        : null
      await targetSession.setProxy({ proxyRules: `${parsed.protocol}//${parsed.host}` })
    } catch (err) {
      logMain('sendnow', 'falha ao configurar proxy da janela, seguindo sem proxy', { error: String(err) })
      currentProxyCreds = null
      await targetSession.setProxy({ proxyRules: 'direct://' }).catch(() => undefined)
    }
  }

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

  async function resolve(sourceUrl: string, proxyUrl?: string): Promise<{
    url?: string
    cookieHeader?: string
    userAgent: string
    downloadId: string
    rand: string
    referer: string
  }> {
    const id = fileIdFromUrl(sourceUrl)
    const win = getWindow()
    await ensureBrowsingProxy(proxyUrl)
    logMain('sendnow', 'resolve iniciado', { sourceUrl, usingProxy: Boolean(proxyUrl) })
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
    let submittedChallengeForm = false
    const deadline = Date.now() + 20_000
    while (Date.now() < deadline) {
      await delay(700)
      const title = await win.webContents.getTitle()
      if (/just a moment|attention required|verifying you are human/i.test(title)) {
        continue
      }
      // Antes da página de arquivo de verdade, o Send.now mostra uma página
      // intermediária "Download Challenge": um form com cf-turnstile-response
      // (Turnstile resolvido, às vezes sozinho) e um botão de submit, mas SEM
      // "rand" — esse só existe na página seguinte. Ler id/rand direto aqui
      // sempre dava rand vazio. Se o token do Turnstile já está pronto, submete
      // esse form intermediário (nativo, sem depender de handler de clique) e
      // deixa a página seguinte carregar antes de tentar ler id/rand de novo.
      if (!submittedChallengeForm) {
        const challengeSubmitted = await win.webContents.executeJavaScript(`(() => {
          const tokenInput = document.querySelector('input[name="cf-turnstile-response"]')
          const form = tokenInput ? tokenInput.closest('form') : null
          if (tokenInput instanceof HTMLInputElement && tokenInput.value.length > 10 && form instanceof HTMLFormElement) {
            form.submit()
            return true
          }
          return false
        })()`).catch(() => false) as boolean
        if (challengeSubmitted) {
          submittedChallengeForm = true
          logMain('sendnow', 'form de challenge (Turnstile resolvido) submetido')
          await delay(1500)
          continue
        }
      }
      formValues = await win.webContents.executeJavaScript(`(() => {
        const value = (name) => document.querySelector('input[name="' + name + '"]')?.value || ''
        return { id: value('id'), rand: value('rand'), referer: value('referer') }
      })()`) as { id?: string; rand?: string; referer?: string }
      // "id" costuma aparecer antes de "rand" no form (JS separado, talvez preso
      // no próprio callback do Turnstile). Submeter com rand vazio faz o host
      // devolver 200 sem redirect (nem cookie novo) em vez de erro claro — daí
      // o "não retornou o link temporário" fica em loop achando que é rate
      // limit. Só sai do polling com os dois campos preenchidos.
      if (formValues.id && formValues.rand) break
    }
    logMain('sendnow', 'form values extraídos', { hasId: Boolean(formValues.id), hasRand: Boolean(formValues.rand) })
    if (!formValues.id || !formValues.rand) {
      const pageDiagnostics = await win.webContents.executeJavaScript(`(() => {
        const inputs = Array.from(document.querySelectorAll('input')).map((input) => ({ name: input.name, type: input.type, hasValue: Boolean(input.value) }))
        const forms = Array.from(document.querySelectorAll('form')).map((form) => form.id || form.getAttribute('name') || '(sem id/name)')
        const clickable = Array.from(document.querySelectorAll('[id*="download" i], [class*="download" i], [id*="free" i], [class*="free" i], [id*="rand"], [name*="rand"]'))
          .slice(0, 20)
          .map((el) => ({ tag: el.tagName, id: el.id, cls: (el.className || '').toString().slice(0, 60), text: (el.textContent || '').trim().slice(0, 60), visible: el.getBoundingClientRect().width > 0, outerHTML: el.outerHTML.slice(0, 300) }))
        const scriptHits = Array.from(document.querySelectorAll('script:not([src])'))
          .map((s) => s.textContent || '')
          .filter((text) => /rand/i.test(text))
          .map((text) => {
            const idx = text.search(/rand/i)
            return text.slice(Math.max(0, idx - 150), idx + 150)
          })
          .slice(0, 5)
        const bigButtons = Array.from(document.querySelectorAll('a, button, div'))
          .filter((el) => {
            const r = el.getBoundingClientRect()
            return r.width > 120 && r.height > 30 && r.width < 500
          })
          .slice(0, 12)
          .map((el) => ({ tag: el.tagName, id: el.id, cls: (el.className || '').toString().slice(0, 60), text: (el.textContent || '').trim().slice(0, 60) }))
        return { title: document.title, inputs, forms, clickable, scriptHits, bigButtons, fullBodyText: (document.body?.innerText || '').slice(0, 1500) }
      })()`).catch((error) => ({ error: String(error) }))
      logMain('sendnow', 'diagnostico da pagina (rand/id ausente)', pageDiagnostics)
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
          logMain('sendnow', 'redirect temporário capturado via webRequest', { redirectUrl })
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
        logMain('sendnow', 'resposta do POST download2', { statusCode: response.statusCode })
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
              logMain('sendnow', 'POST 200 sem redirect e sem cookie novo', {})
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

  async function handleAction(payload: { action?: string; url?: string; proxy?: string }): Promise<unknown> {
    if (payload.action !== 'sendnow_resolve' || !payload.url) {
      throw new Error('Ação Send.now inválida')
    }
    return resolve(payload.url, payload.proxy)
  }

  return { handleAction }
}
