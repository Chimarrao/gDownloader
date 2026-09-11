import { randomUUID } from 'crypto'
import { existsSync, mkdirSync, rmSync } from 'fs'
import { dirname } from 'path'

import { app, BrowserWindow, session } from 'electron'
import {
  createExclusiveRunner,
  configureHosterSession,
  configureHosterWindow,
  delay,
  looksLikeFilename,
  parseHumanSize,
  sanitizeFilename,
} from './browser-helper-common'
import { logMain } from './debug-log'

const KATFILE_PARTITION = 'persist:katfile'

// Deteccao de widget de captcha reusada em varios pontos do fluxo. O iframe do
// Turnstile usa "srcdoc" (HTML inline), nao um atributo src normal, entao
// iframe[src*="..."] sozinho nunca casa com ele — so a div externa
// ".cf-turnstile" e confiavel. Como fallback extra (por seguranca, caso o
// Katfile mude a marcacao ou sirva uma variante diferente do desafio pra
// sessoes "suspeitas"), tambem casamos por atributo de acessibilidade do
// iframe (title) e por texto visivel da pagina em varios idiomas.
const CHALLENGE_DETECT_EXPR = `Boolean(
          document.querySelector('.cf-turnstile, [class*="turnstile" i], [class*="recaptcha" i], [class*="hcaptcha" i], iframe[src*="turnstile"], iframe[src*="challenges.cloudflare.com"], iframe[src*="recaptcha"], iframe[src*="hcaptcha"], iframe[title*="challenge" i], iframe[title*="captcha" i], iframe[title*="human" i], iframe[title*="widget" i]')
        ) || /confirme que.{0,4}humano|verify you are human|confirm you are human|i.?m not a robot|não sou um rob[oô]|prove que não é um rob[oô]/i.test((document.body && document.body.innerText) || '')`

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
  status: 'pending' | 'downloading' | 'complete' | 'error' | 'cancelled' | 'solving_captcha'
  bytesDownloaded: number
  totalBytes: number
  speedBps: number
  etaSecs: number
  filename?: string
  error?: string
  solvingSolver?: string
  solvingStage?: string
  startResolve?: () => void
  startReject?: (error: Error) => void
  startTimeout?: ReturnType<typeof setTimeout>
  driveInterval?: ReturnType<typeof setTimeout>
  driveStopped?: boolean
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
export function createKatfileService(opts?: {
  turnstile?: {
    solve: (req: { sitekey: string; pageurl: string; proxy?: string; timeoutMs?: number; provider?: string; type?: string }) => Promise<{ token: string; solverId: string; elapsedMs: number }>
  }
  getProxy?: () => Promise<string | undefined>
}) {
  const jobs = new Map<string, KatfileDownloadJob>()
  let helperWindow: BrowserWindow | null = null
  let sessionWired = false
  let pendingDownloadJobId: string | null = null
  const runExclusive = createExclusiveRunner()
  let solverBusy = false
  let lastSolverAt = 0
  let currentProxyCreds: { user: string; pass: string } | null = null
  let proxyLoginWired = false

  // O Katfile grátis bloqueia por IP: "Delay between free downloads must be
  // not less than 120 minutes." Confirmado em teste real — o mesmo arquivo,
  // na mesma sessão, muda de "SLOW SPEED DOWNLOAD" normal para essa mensagem
  // de bloqueio assim que uma tentativa anterior já foi feita pelo mesmo IP.
  // Trocar de circuito Tor (IsolateSOCKSAuth, usuário SOCKS5 novo por job)
  // dá um IP de saída diferente e evita esse bloqueio — sem isso, só o 1º
  // item da fila consegue baixar de verdade a cada 2 horas.
  function wireProxyLogin(): void {
    if (proxyLoginWired) return
    proxyLoginWired = true
    // O evento de autenticacao de proxy (SOCKS5 IsolateSOCKSAuth) so existe em
    // `app`, nao em `session` — filtramos pelo webContents da nossa janela.
    app.on('login', (event, webContents, _details, authInfo, callback) => {
      if (!authInfo.isProxy || !currentProxyCreds) return
      if (!helperWindow || webContents !== helperWindow.webContents) return
      event.preventDefault()
      callback(currentProxyCreds.user, currentProxyCreds.pass)
    })
  }

  async function ensureBrowsingProxy(): Promise<void> {
    wireProxyLogin()
    const katfileSession = session.fromPartition(KATFILE_PARTITION)
    const proxyUrl = await opts?.getProxy?.().catch(() => undefined)
    if (!proxyUrl) {
      currentProxyCreds = null
      await katfileSession.setProxy({ proxyRules: 'direct://' }).catch(() => undefined)
      return
    }
    try {
      const parsed = new URL(proxyUrl)
      currentProxyCreds = { user: decodeURIComponent(parsed.username), pass: decodeURIComponent(parsed.password) }
      await katfileSession.setProxy({ proxyRules: `${parsed.protocol}//${parsed.host}` })
      logMain('katfile', 'proxy da janela configurado (circuito isolado)', { host: parsed.host, user: currentProxyCreds.user })
    } catch (err) {
      logMain('katfile', 'falha ao configurar proxy da janela, seguindo sem proxy', { error: String(err) })
      currentProxyCreds = null
      await katfileSession.setProxy({ proxyRules: 'direct://' }).catch(() => undefined)
    }
  }

  function refreshHelperThrottling(): void {
    if (!helperWindow || helperWindow.isDestroyed()) return
    const hasPendingBrowserWork = [...jobs.values()].some((job) => job.status === 'pending')
    helperWindow.webContents.setBackgroundThrottling(!hasPendingBrowserWork)
  }

  function getWindow(): BrowserWindow {
    if (helperWindow && !helperWindow.isDestroyed()) {
      refreshHelperThrottling()
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
        backgroundThrottling: true,
      },
    })

    configureHosterWindow(helperWindow, KATFILE_PARTITION)
    // O Katfile tem anúncios que tentam abrir popups/abas (kfSlowPopAd) e,
    // ocasionalmente, redirecionam a própria página para sites de anúncio.
    // Bloqueia os dois: nenhuma aba/janela nova, e nenhuma navegação de
    // primeiro nível pra fora do domínio do Katfile (a página em si e o
    // widget do Turnstile continuam funcionando normalmente).
    helperWindow.webContents.setWindowOpenHandler(() => ({ action: 'deny' }))
    helperWindow.webContents.on('will-navigate', (event, url) => {
      try {
        const hostname = new URL(url).hostname
        if (!hostname.includes('katfile')) {
          logMain('katfile', 'will-navigate bloqueado (fora do dominio Katfile)', { url })
          event.preventDefault()
        }
      } catch {
        event.preventDefault()
      }
    })
    helperWindow.webContents.on('did-fail-load', (_e, code, desc, url) => {
      logMain('katfile', 'did-fail-load', { code, desc, url })
    })
    helperWindow.webContents.on('did-finish-load', () => {
      logMain('katfile', 'did-finish-load', { url: helperWindow?.webContents.getURL() })
    })
    helperWindow.on('closed', () => {
      helperWindow = null
    })
    refreshHelperThrottling()
    return helperWindow
  }

  async function readPageSnapshot(): Promise<KatfilePageSnapshot> {
    const win = getWindow()
    if (win.isDestroyed()) throw new Error('Janela Katfile destruída')
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

  async function extractTurnstileParams(): Promise<{ sitekey: string; pageurl: string } | null> {
    const win = getWindow()
    if (win.isDestroyed()) return null
    return (await win.webContents.executeJavaScript(
      `(() => {
        const el = document.querySelector('[data-sitekey]');
        let sitekey = el?.getAttribute('data-sitekey') || '';
        if (!sitekey) {
          const m = document.documentElement.outerHTML.match(/data-sitekey=["']([^"']+)["']/);
          if (m) sitekey = m[1];
        }
        if (!sitekey) {
          const iframe = document.querySelector('iframe[src*="turnstile"]');
          const src = iframe?.getAttribute('src') || '';
          const m2 = src.match(/[?&]k=([^&]+)/);
          if (m2) sitekey = decodeURIComponent(m2[1]);
        }
        return sitekey ? { sitekey, pageurl: location.href } : null;
      })()`,
      true,
    )) as { sitekey: string; pageurl: string } | null
  }

  // So injeta o token nos campos do Turnstile e dispara os eventos — quem clica
  // no botao real de envio (com o gate anti-adblock de 2 cliques) e o proximo
  // ciclo do advanceFlow (branch "token_submit_click"), para nao duplicar essa
  // logica em dois lugares e arriscar ela divergir.
  async function injectTurnstileToken(token: string): Promise<void> {
    const win = getWindow()
    if (win.isDestroyed()) return
    const result = (await win.webContents.executeJavaScript(
      `(() => {
        const token = ${JSON.stringify(token)};
        const nodes = Array.from(document.querySelectorAll('textarea[name="cf-turnstile-response"], input[name="cf-turnstile-response"]'));
        for (const n of nodes) {
          if (n instanceof HTMLTextAreaElement || n instanceof HTMLInputElement) {
            n.value = token;
            n.dispatchEvent(new Event('input', { bubbles: true }));
            n.dispatchEvent(new Event('change', { bubbles: true }));
          }
        }
        if (typeof (window).turnstile !== 'undefined') {
          try { (window).turnstile.getResponse = () => token; } catch {}
        }
        // Cloudflare pode injetar um desafio de borda (site-wide, nao o
        // captcha proprio do Katfile) em QUALQUER pagina quando o trafego
        // parece automatizado — inclusive na pagina inicial "Choose download
        // type", antes mesmo do method_free=1 ser enviado. Resolver esse
        // desafio de borda nao equivale a resolver o captcha do download; se
        // ainda estivermos na pagina inicial (form#btn_download existe),
        // reenviamos o method_free=1 direto pra continuar o fluxo real.
        const initialForm = document.querySelector('form#btn_download')
        if (initialForm instanceof HTMLFormElement) {
          let mf = initialForm.querySelector('input[name="method_free"]')
          if (!(mf instanceof HTMLInputElement)) {
            mf = document.createElement('input')
            mf.type = 'hidden'
            mf.name = 'method_free'
            initialForm.appendChild(mf)
          }
          mf.value = '1'
          initialForm.submit()
          return { tokenNodesFound: nodes.length, resubmittedInitialForm: true }
        }
        return { tokenNodesFound: nodes.length }
      })()`,
      true,
    )) as unknown
    logMain('katfile', 'injectTurnstileToken', result as Record<string, unknown>)
  }

  async function tryAutoTurnstile(): Promise<boolean> {
    if (!opts?.turnstile || solverBusy) return false
    if (Date.now() - lastSolverAt < 5000) return false
    const params = await extractTurnstileParams()
    if (!params?.sitekey) return false
    const pendingJob = [...jobs.values()].find((j) => j.status === 'pending' || j.status === 'solving_captcha')
    if (pendingJob) {
      pendingJob.status = 'solving_captcha'
      pendingJob.solvingSolver = 'auto'
      pendingJob.solvingStage = `Resolvendo captcha com solver...`
      pendingJob.error = `Resolvendo captcha Turnstile...`
    }
    solverBusy = true
    lastSolverAt = Date.now()
    try {
      const proxy = await opts.getProxy?.()
      const res = await opts.turnstile.solve({ sitekey: params.sitekey, pageurl: params.pageurl, proxy, timeoutMs: 45000, provider: 'katfile' })
      if (res?.token && res.token.length >= 20) {
        if (pendingJob) {
          pendingJob.solvingSolver = res.solverId
          pendingJob.solvingStage = `Token obtido via ${res.solverId} em ${res.elapsedMs}ms`
          pendingJob.error = `Captcha resolvido via ${res.solverId} — enviando...`
        }
        await injectTurnstileToken(res.token)
        if (pendingJob) {
          pendingJob.status = 'pending'
          pendingJob.solvingSolver = undefined
          pendingJob.solvingStage = undefined
        }
        return true
      }
    } catch {
      if (pendingJob) pendingJob.error = `Solver falhou, tentando próximo...`
    } finally {
      solverBusy = false
      if (pendingJob && pendingJob.status === 'solving_captcha') {
        pendingJob.status = 'pending'
        pendingJob.solvingStage = 'Aguardando resolução manual (fallback)'
        pendingJob.error = undefined
      }
    }
    return false
  }

  // O botao "SLOW SPEED DOWNLOAD" (#fbtn1) tem um gate anti-adblock de 2
  // cliques que so funciona se TODOS os scripts do jQuery ready() da pagina
  // carregarem sem erro — confirmado que isso falha as vezes via Tor (mesma
  // causa raiz do botao "Send" do captcha: um erro em QUALQUER script daquele
  // bloco impede os handlers seguintes de serem registrados, e o clique vira
  // no-op silencioso). O servidor nao valida esse clique, so o campo
  // method_free=1 no POST — entao submetemos o form#btn_download direto,
  // pulando o gate de anuncio inteiro. Comprovado em teste real: chega direto
  // na pagina de contagem/captcha em ~1-2s, sem depender de nenhum clique.
  async function clickFreeEntry(): Promise<boolean> {
    const win = getWindow()
    if (win.isDestroyed()) return false
    const result = (await win.webContents.executeJavaScript(
      `(() => {
        const form = document.querySelector('form#btn_download')
        if (!(form instanceof HTMLFormElement)) return false
        let input = form.querySelector('input[name="method_free"]')
        if (!(input instanceof HTMLInputElement)) {
          input = document.createElement('input')
          input.type = 'hidden'
          input.name = 'method_free'
          form.appendChild(input)
        }
        input.value = '1'
        form.submit()
        return true
      })()`,
      true
    )) as boolean
    logMain('katfile', 'clickFreeEntry (submit direto) resultado', { result, url: win.webContents.getURL() })
    return Boolean(result)
  }

  async function advanceFlow(): Promise<void> {
    const win = getWindow()
    if (win.isDestroyed()) return
    // Try auto solver first if challenge present (universal, como yt-dlp)
    try {
      // hasToken: um token JA injetado (resolvido num ciclo anterior) continua
      // no DOM ate a pagina navegar — sem checar isso, todo ciclo seguinte via
      // hasChallenge (o widget nao some so por injetar o token) chamava o
      // solver de novo, resolvendo o MESMO desafio repetidas vezes e abrindo
      // varias janelas de Chrome a toa em vez de so deixar o script principal
      // submeter o token que ja temos.
      // onInitialPage: a pagina inicial "Choose download type" as vezes mostra
      // uma verificacao de borda do proprio Cloudflare (o banner "Verificando..."
      // -> "Sucesso!") que se resolve SOZINHA em poucos segundos sem precisar de
      // nenhum solver nosso — nao e o captcha do download do Katfile. Chamar o
      // solver nessa hora so desperdiça tempo/banda e atrapalha o clique direto
      // no form#btn_download. So tenta resolver via solver depois que ja saimos
      // dessa pagina (form#btn_download nao existe mais).
      const { hasChallenge, hasToken, onInitialPage } = (await win.webContents.executeJavaScript(
        `({
          hasChallenge: ${CHALLENGE_DETECT_EXPR},
          hasToken: Array.from(document.querySelectorAll('textarea[name="cf-turnstile-response"], input[name="cf-turnstile-response"], textarea[name="g-recaptcha-response"], input[name="g-recaptcha-response"], textarea[name="h-captcha-response"], input[name="h-captcha-response"]')).some((n) => (n.value || '').trim().length >= 20),
          onInitialPage: Boolean(document.querySelector('form#btn_download'))
        })`,
        true,
      )) as { hasChallenge: boolean; hasToken: boolean; onInitialPage: boolean }
      if (hasChallenge && !hasToken && !onInitialPage && opts?.turnstile) {
        const solved = await tryAutoTurnstile().catch(() => false)
        if (solved) return
      }
    } catch {}
    try {
      const result = (await win.webContents.executeJavaScript(
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
        const pageText = ((document.body?.innerText || '') + '\\n' + (document.title || '')).toLowerCase()
        const hasChallengeWidget = ${CHALLENGE_DETECT_EXPR}
        const hasCloudflareInterstitial =
          /just a moment|checking your browser|verify you are human|verifique se voce e humano|verifique se você é humano/i.test(pageText)

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

          // O botao real de envio (".downloadbtn", texto "Send") tem um handler
          // de anti-adblock (2 cliques) que so e registrado se $.cookie (plugin
          // jQuery externo) carregar — as vezes falha via Tor, e nesse caso
          // clicar no botao nao faz absolutamente nada (handler nunca existiu).
          // O servidor nao valida esse clique de forma alguma — e so teatro
          // client-side — entao submetemos o formulario direto via API nativa,
          // que ignora handlers de clique/evento por completo e sempre funciona.
          if (activeForm instanceof HTMLFormElement) {
            activeForm.submit()
            return { branch: 'token_submit_form_direct' }
          }
          return { branch: 'token_no_form_found' }
        }

        // Ultima etapa depois do captcha: pagina "Your Free Download Speed Is
        // Limited" com um link direto pro arquivo (CDN, ja assinado) em
        // "#dlink" / a.kf-dl2-free, texto "Continue with Free Download", OU
        // qualquer <a> cujo href aponte pra CDN de download do Katfile
        // (ex: s5144.katfile.biz:183/d/<token>/<arquivo>) — cobre variantes
        // de pagina que nao usam exatamente o mesmo id/classe.
        const directDownloadLink =
          document.querySelector('#dlink')
          || document.querySelector('a.kf-dl2-free')
          || Array.from(document.querySelectorAll('a')).find((a) => /continue with free download/i.test(a.textContent || ''))
          || Array.from(document.querySelectorAll('a[href]')).find((a) => /katfile\\.(biz|com)(:\\d+)?\\/d\\//i.test(a.getAttribute('href') || ''))
        if (directDownloadLink instanceof HTMLAnchorElement) {
          directDownloadLink.click()
          return { branch: 'direct_download_link_click', href: directDownloadLink.href }
        }

        if (hasChallengeWidget || hasCloudflareInterstitial) {
          return { branch: 'challenge_widget_wait' }
        }

        if (typeof (window).estimated_time === 'number' && typeof (window).es === 'function') {
          if ((window).estimated_time > 1) {
            ;(window).estimated_time = 1
          }
          ;(window).es()
          return { branch: 'estimated_time_flow' }
        }

        if (typeof (window).adEnable !== 'undefined') {
          ;(window).adEnable = true
        }

        // Katfile novo usa WAIT_SECONDS=30 + cdSubmitForm() via RAF (anti-adblock)
        // Só executa se o countdown estiver visível para não interferir na initial page
        const countdownEl2 = document.querySelector('.emo-free-time, #countdown, [id*="countdown"]') || Array.from(document.querySelectorAll('*')).find(el => /Free slow download will start after/i.test(el.textContent || ''))
        const countdownVisible2 = Boolean(countdownEl2 && countdownEl2.getBoundingClientRect().width > 0)
        if (countdownVisible2 && typeof (window).WAIT_SECONDS === 'number' && typeof (window).cdSubmitForm === 'function') {
          if ((window).WAIT_SECONDS > 1) {
            ;(window).WAIT_SECONDS = 1
          }
          try { ;(window).cdSubmitForm() } catch {}
          return { branch: 'cdSubmitForm_called' }
        }
        if (countdownVisible2 && typeof (window).WAIT_SECONDS === 'number') {
          ;(window).WAIT_SECONDS = 1
          return { branch: 'wait_seconds_forced' }
        }

        // NUNCA clicar em "Click Here": no HTML real do Katfile todo elemento com esse texto
        // é o link de upsell Premium (href=".../?op=payments"), tanto dentro do banner
        // quanto dentro do próprio contador #txt/#m_txt. Clicar nele sequestra o fluxo para
        // a página de pagamentos (raiz do bug "trava em SLOW SPEED" / "janela abre em branco").
        // O contador real (var WAIT_SECONDS dentro do closure $(document).ready) não é
        // acessível via window.WAIT_SECONDS — ele se auto-envia via cdSubmitForm() sozinho
        // após ~30s reais; só precisamos aguardar sem interferir nem clicar em upsells.

        const freeButton =
          document.querySelector('#fbtn1')
          || document.querySelector('#m_fbtn1')
          || document.querySelector('input[name="method_free"]')
          || document.querySelector('button[name="method_free"]')

        const clickCount = Number((window)[clickCountKey] || 0)
        // Sem limite de tentativas: o botão exige 2 cliques reais (1o so mostra
        // anuncio, 2o inicia a contagem) e depois disso vira no-op no proprio JS
        // da pagina (guard interno slowDownloadStarted), entao reclicar e sempre
        // seguro. Um teto arbitrario aqui ja deixou o fluxo travado para sempre
        // quando o gate nao era satisfeito nas primeiras tentativas.
        if (freeButton instanceof HTMLElement) {
          ;(window)[clickCountKey] = clickCount + 1
          freeButton.click()
          freeButton.click()
          return { branch: 'reclick_free_button', clickCount: clickCount + 1 }
        } else if (activeForm instanceof HTMLFormElement) {
          const submitButton =
            document.querySelector('#freebtn')
            || document.querySelector('#Send')
            || document.querySelector('button[type="submit"]')
            || document.querySelector('input[type="submit"]')
          if (submitButton instanceof HTMLElement) {
            submitButton.click()
            return { branch: 'submit_button_click' }
          }
        }
        // Beco sem saida: nada bateu em nenhum seletor conhecido. Loga os
        // links/botoes visiveis pra dar visibilidade de qual variante de
        // pagina o Katfile serviu dessa vez, sem precisar reproduzir de novo.
        const visibleLinks = Array.from(document.querySelectorAll('a[href]'))
          .filter((a) => a.getBoundingClientRect().width > 0)
          .slice(0, 8)
          .map((a) => (a.getAttribute('href') || '').slice(0, 80) + ' :: ' + (a.textContent || '').trim().slice(0, 40))
        const visibleButtons = Array.from(document.querySelectorAll('button'))
          .filter((b) => b.getBoundingClientRect().width > 0)
          .slice(0, 8)
          .map((b) => (b.textContent || '').trim().slice(0, 40))
        return { branch: 'noop', hasChallengeWidget, hasCloudflareInterstitial, countdownVisible: countdownVisible2, waitSecondsType: typeof (window).WAIT_SECONDS, visibleLinks, visibleButtons }
      })()`,
      true
    )) as { branch: string; [key: string]: unknown } | undefined
    logMain('katfile', 'advanceFlow', { url: win.webContents.getURL(), ...result })
    } catch (e) {
      logMain('katfile', 'advanceFlow ERRO', { error: String(e), url: win.webContents.getURL() })
      if (String(e).includes('destroyed')) return
      throw e
    }
  }

  function clearJobTimers(job: KatfileDownloadJob): void {
    if (job.startTimeout) {
      clearTimeout(job.startTimeout)
      job.startTimeout = undefined
    }
    job.driveStopped = true
    if (job.driveInterval) {
      clearTimeout(job.driveInterval)
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

      logMain('katfile', 'will-download recebido', {
        jobId,
        url: item.getURL(),
        mimeType: item.getMimeType(),
        totalBytes: item.getTotalBytes(),
        contentDisposition: item.getContentDisposition(),
        canResume: item.canResume(),
      })

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

      if (helperWindow && !helperWindow.isDestroyed() && helperWindow.isVisible()) {
        helperWindow.hide()
      }

      let lastUpdatedLogAt = 0
      item.on('updated', (_ev, state) => {
        const nowLog = Date.now()
        if (nowLog - lastUpdatedLogAt > 5000) {
          lastUpdatedLogAt = nowLog
          logMain('katfile', 'download updated', {
            state,
            receivedBytes: item.getReceivedBytes(),
            totalBytes: item.getTotalBytes(),
            canResume: item.canResume(),
            isPaused: item.isPaused(),
            url: item.getURL(),
          })
        }
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

        // Watchdog de estagnação: o Katfile grátis permite só 1 download simultâneo
        // por conta/IP. Se outro item ficar preso em 0 bytes (conexão viva mas sem
        // dados), sem isso o job travaria para sempre — a trava de 8min só existe
        // antes do will-download disparar. Cancela e deixa o Rust reagendar.
        if (bytesDownloaded === 0 && now - job.lastProgressAt > 45_000) {
          logMain('katfile', 'download parado em 0 bytes, cancelando para permitir nova tentativa', {
            jobId: job.id,
            paradoHaMs: now - job.lastProgressAt,
          })
          item.cancel()
          return
        }

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
        } else {
          job.status = state === 'cancelled' ? 'cancelled' : 'error'
          job.error =
            state === 'cancelled'
              ? 'Download cancelado pelo navegador integrado do Katfile.'
              : 'O navegador integrado do Katfile interrompeu o download antes da conclusão.'
        }
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

      // setInterval dispararia de novo mesmo se o ciclo anterior de advanceFlow
      // ainda estivesse rodando (uma resolução de captcha pode levar 10-45s+),
      // empilhando dezenas de chamadas concorrentes — cada uma podia iniciar
      // seu proprio solver, abrindo uma pilha de janelas de Chrome que nunca
      // carregavam direito. Agenda-se de novo só depois que o ciclo anterior
      // realmente terminou, nunca em paralelo.
      const runAdvanceLoop = async (): Promise<void> => {
        if (job.driveStopped) return
        await advanceFlow().catch((e) => logMain('katfile', 'driveInterval advanceFlow rejeitou', { error: String(e) }))
        if (job.driveStopped) return
        job.driveInterval = setTimeout(() => {
          void runAdvanceLoop()
        }, 1800)
      }
      job.driveInterval = setTimeout(() => {
        void runAdvanceLoop()
      }, 1800)

      // A janela do Katfile nunca é exibida: o fluxo é 100% automático em
      // segundo plano. Se o solver falhar, o job simplesmente erra e o Rust
      // reagenda — sem popup pro usuário resolver na mão.
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
    // O lock exclusivo protege só a JANELA compartilhada (clique -> captcha ->
    // will-download): a partir daqui a transferência do arquivo roda via
    // download manager do Chromium, sem precisar da página/janela, então o
    // próximo item da fila já pode começar seu próprio fluxo de clique aqui.
    // O bloqueio "1 download simultâneo por IP" do Katfile grátis é tratado
    // pelo watchdog de estagnação em 'updated' (cancela e deixa o Rust tentar
    // de novo) e pelo Tor isolado por job (getProxy), não por serializar tudo
    // aqui — manter o lock até o fim do download travava a fila inteira
    // sempre que um job pausado/travado ficava preso em segundo plano.
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
        await ensureBrowsingProxy()
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
      refreshHelperThrottling()
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
        solvingSolver: job.solvingSolver,
        solvingStage: job.solvingStage,
      }
    }

    if (body.action === 'katfile_file_info') {
      const url = typeof body.url === 'string' ? body.url : ''
      if (!url) {
        throw new Error('Ação do Katfile sem URL.')
      }
      return getFileInfo(url)
    }

    if (body.action === 'katfile_cancel') {
      const destPath = typeof body.destPath === 'string' ? body.destPath : ''
      const job = [...jobs.values()].find((j) => j.destPath === destPath)
      if (job) {
        logMain('katfile', 'katfile_cancel recebido', { jobId: job.id, destPath })
        clearJobTimers(job)
        if (pendingDownloadJobId === job.id) {
          pendingDownloadJobId = null
        }
        // Se ainda estava na fase 1 (clique/captcha), libera o lock exclusivo
        // da janela agora para o próximo item da fila poder começar — sem
        // isso o job pausado ficava preso em segundo plano travando tudo.
        job.startReject?.(new Error('Cancelado pelo usuário.'))
        job.startResolve = undefined
        job.startReject = undefined
        jobs.delete(job.id)
        refreshHelperThrottling()
      }
      return { cancelled: Boolean(job) }
    }

    throw new Error('Ação do proxy Katfile não suportada.')
  }

  return {
    handleAction,
  }
}
