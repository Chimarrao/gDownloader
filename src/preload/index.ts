import { contextBridge, ipcRenderer } from 'electron'
import { electronAPI } from '@electron-toolkit/preload'
import type { AppSettingsSnapshot } from '../shared/types'
import { splitSseMessages } from './mirror-sse'

// Porta do backend Rust (obtida via IPC e cacheada)
let cachedPort: number | null = null

async function getPort(): Promise<number> {
  if (!cachedPort) {
    cachedPort = (await ipcRenderer.invoke('backend:getPort')) as number
  }
  return cachedPort!
}

async function fetchBackend(path: string, options?: RequestInit): Promise<Response> {
  const port = await getPort()
  return fetch(`http://127.0.0.1:${port}${path}`, options)
}

// Callbacks registrados para eventos de mirrors (SSE)
type MirrorStartPayload = {
  filename: string
  total: number
}

type MirrorProgressPayload = {
  current: number
  total: number
  searcher: string
  phase: string
  newResults: number
  totalResults: number
  rawResults: number
  rejectedResults: number
  durationMs: number
  error?: string | null
}

type MirrorResultPayload = {
  url: string
  source: string
  hoster?: string | null
  score: number
}

type MirrorDonePayload = {
  filename: string
  searchers: number
  total: number
  hosters: number
  durationMs: number
}

type MirrorRendererEvent =
  | { type: 'start'; payload: MirrorStartPayload }
  | { type: 'progress'; payload: MirrorProgressPayload }
  | { type: 'log'; payload: string }
  | { type: 'result'; payload: MirrorResultPayload }
  | { type: 'done'; payload: MirrorDonePayload }
  | { type: 'error'; payload: string }

const mirrorEventHandlers: Array<(ev: MirrorRendererEvent) => void> = []
let activeMirrorController: AbortController | null = null
let activeMirrorSearchSeq = 0

type DownloadChannel =
  | 'download:progress'
  | 'download:status'
  | 'download:complete'
  | 'download:error'
  | 'download:cancelled'

const downloadListeners: Record<DownloadChannel, Set<(data: unknown) => void>> = {
  'download:progress': new Set(),
  'download:status': new Set(),
  'download:complete': new Set(),
  'download:error': new Set(),
  'download:cancelled': new Set(),
}

let downloadsSocket: WebSocket | null = null
let downloadsSocketPromise: Promise<void> | null = null
let downloadsReconnectTimer: ReturnType<typeof setTimeout> | null = null

function hasDownloadListeners(): boolean {
  return Object.values(downloadListeners).some((listeners) => listeners.size > 0)
}

function dispatchDownloadEvent(channel: DownloadChannel, payload: unknown): void {
  for (const listener of downloadListeners[channel]) {
    listener(payload)
  }
}

function routeDownloadEvent(event: Record<string, unknown>): void {
  if (event.type === 'progress') {
    dispatchDownloadEvent('download:progress', event)
    return
  }

  if (event.type === 'status' || event.type === 'status_changed') {
    dispatchDownloadEvent('download:status', event)
    if (event.type === 'status' && event.status === 'cancelled') {
      dispatchDownloadEvent('download:cancelled', event)
    }
    return
  }

  if (event.type === 'complete') {
    dispatchDownloadEvent('download:complete', event)
    return
  }

  if (event.type === 'error') {
    dispatchDownloadEvent('download:error', event)
  }
}

async function ensureDownloadsSocket(): Promise<void> {
  if (downloadsSocket && (downloadsSocket.readyState === WebSocket.OPEN || downloadsSocket.readyState === WebSocket.CONNECTING)) {
    return
  }
  if (downloadsSocketPromise) {
    return downloadsSocketPromise
  }

  downloadsSocketPromise = (async () => {
    const port = await getPort()

    await new Promise<void>((resolve) => {
      const ws = new WebSocket(`ws://127.0.0.1:${port}/ws`)
      downloadsSocket = ws

      ws.onopen = () => {
        resolve()
      }

      ws.onmessage = (msg) => {
        try {
          routeDownloadEvent(JSON.parse(msg.data as string) as Record<string, unknown>)
        } catch {
          // ignora mensagens malformadas
        }
      }

      ws.onerror = () => {
        resolve()
      }

      ws.onclose = () => {
        downloadsSocket = null
        if (!hasDownloadListeners()) {
          return
        }
        if (downloadsReconnectTimer !== null) {
          clearTimeout(downloadsReconnectTimer)
        }
        downloadsReconnectTimer = setTimeout(() => {
          downloadsReconnectTimer = null
          void ensureDownloadsSocket()
        }, 800)
      }
    })
  })()

  try {
    await downloadsSocketPromise
  } finally {
    downloadsSocketPromise = null
  }
}

function closeDownloadsSocketIfIdle(): void {
  if (hasDownloadListeners()) {
    return
  }

  if (downloadsReconnectTimer !== null) {
    clearTimeout(downloadsReconnectTimer)
    downloadsReconnectTimer = null
  }

  downloadsSocket?.close()
  downloadsSocket = null
}

// API completa exposta para o renderer Vue
const api = {
  // --- Provider / Módulos ---
  modules: {
    // Lista todos os providers suportados
    list: async () => {
      try {
        const resp = await fetchBackend('/providers')
        if (!resp.ok) return []
        return resp.json()
      } catch {
        return []
      }
    },

    // Detecta qual provider suporta a URL
    detect: async (url: string) => {
      try {
        const resp = await fetchBackend(`/detect?url=${encodeURIComponent(url)}`)
        if (!resp.ok) return null
        return resp.json()
      } catch {
        return null
      }
    },

    // Retorna metadados do arquivo (nome, tamanho)
    fileInfo: async (_moduleId: string, url: string) => {
      const resp = await fetchBackend(`/file-info?url=${encodeURIComponent(url)}`)
      if (!resp.ok) {
        const body = await resp.json().catch(() => ({ error: 'Erro desconhecido' }))
        throw new Error(body.error ?? 'Falha ao obter informações do arquivo')
      }
      return resp.json()
    },

    cachedFileInfo: async (_moduleId: string, url: string) => {
      const resp = await fetchBackend(`/file-info/cache?url=${encodeURIComponent(url)}`)
      if (!resp.ok) {
        return null
      }
      return resp.json()
    },

    isLoggedIn: async (moduleId: string) => ipcRenderer.invoke('auth:isLoggedIn', moduleId),
  },

  // --- Downloads ---
  downloads: {
    // Adiciona download à fila do backend
    add: async (
      url: string,
      _moduleId: string,
      _title: string,
      _size: number,
      destDir: string,
      selectedChildren?: string[]
    ) => {
      const settings = await ipcRenderer.invoke('settings:load').catch(() => null)
      const resp = await fetchBackend('/downloads', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          url,
          dest_dir: destDir,
          max_retries: Math.max(0, Number(settings?.maxRetriesPerDownload ?? 0) - 1),
          speed_limit_kib: settings?.speedLimitKib ?? 0,
          parallel_parts: settings?.parallelPartsPerDownload ?? 1,
          selected_children: selectedChildren,
        }),
      })
      if (!resp.ok) {
        const body = await resp.json().catch(() => ({ error: 'Erro desconhecido' }))
        throw new Error(body.error ?? 'Erro ao adicionar download')
      }
      const d = await resp.json()
      return rustDownloadToItem(d)
    },

    // Lista todos os downloads
    list: async () => {
      try {
        const resp = await fetchBackend('/downloads')
        if (!resp.ok) return []
        const downloads = await resp.json()
        return downloads.map(rustDownloadToItem)
      } catch {
        return []
      }
    },

    // Cancela um download pelo ID
    cancel: async (id: string) => {
      await fetchBackend(`/downloads/${id}`, { method: 'DELETE' })
    },

    pause: async (id: string) => {
      await fetchBackend(`/downloads/${id}/pause`, { method: 'POST' })
    },

    resume: async (id: string) => {
      await fetchBackend(`/downloads/${id}/resume`, { method: 'POST' })
    },

    retry: async (id: string) => {
      await fetchBackend(`/downloads/${id}/retry`, { method: 'POST' })
    },

    restart: async (id: string) => {
      await fetchBackend(`/downloads/${id}/restart`, { method: 'POST' })
    },

    force: async (id: string) => {
      await fetchBackend(`/downloads/${id}/force`, { method: 'POST' })
    },

    remove: async (id: string) => {
      await fetchBackend(`/downloads/${id}/remove`, { method: 'DELETE' })
    },

    removeWithFiles: async (id: string) => {
      await fetchBackend(`/downloads/${id}/remove-with-files`, { method: 'DELETE' })
    },

    clearFinished: async () => {
      await fetchBackend('/downloads/finished', { method: 'DELETE' })
    },

    // Subscreve a eventos de progresso via WebSocket
    on: (channel: DownloadChannel, cb: (data: unknown) => void) => {
      downloadListeners[channel].add(cb)
      void ensureDownloadsSocket()

      return () => {
        downloadListeners[channel].delete(cb)
        closeDownloadsSocketIfIdle()
      }
    },
  },

  // --- Settings ---
  settings: {
    load: (): Promise<AppSettingsSnapshot> => ipcRenderer.invoke('settings:load'),
    save: (s: AppSettingsSnapshot): Promise<AppSettingsSnapshot> => ipcRenderer.invoke('settings:save', s),
    chooseDirectory: (): Promise<string> => ipcRenderer.invoke('dialog:chooseDirectory'),
  },

  auth: {
    isLoggedIn: (moduleId: string): Promise<boolean> => ipcRenderer.invoke('auth:isLoggedIn', moduleId),
    login: (moduleId: string, params: Record<string, string>): Promise<void> =>
      ipcRenderer.invoke('auth:login', moduleId, params),
    logout: (moduleId: string): Promise<void> => ipcRenderer.invoke('auth:logout', moduleId),
    accountInfo: (moduleId: string): Promise<unknown> => ipcRenderer.invoke('auth:accountInfo', moduleId),
  },

  // --- Histórico ---
  loadHistory: () => ipcRenderer.invoke('history:load'),
  saveHistory: (items: unknown) => ipcRenderer.invoke('history:save', items),
  clearHistory: () => ipcRenderer.invoke('history:clear'),

  // --- Shell ---
  openPath: (path: string): Promise<string> => ipcRenderer.invoke('shell:openPath', path),
  showInFolder: (path: string): Promise<void> => ipcRenderer.invoke('shell:showInFolder', path),
  clipboard: {
    writeText: (text: string): Promise<boolean> => ipcRenderer.invoke('clipboard:writeText', text),
  },
  system: {
    notify: (title: string, body?: string): Promise<boolean> => ipcRenderer.invoke('system:notify', title, body),
  },
  archive: {
    extract: (archivePath: string): Promise<string> => ipcRenderer.invoke('archive:extract', archivePath),
  },
  terabox: {
    netRequest: (params: { url: string; method?: string; headers?: Record<string, string>; body?: string }): Promise<unknown> =>
      ipcRenderer.invoke('terabox:net-request', params),
  },
  captcha: {
    nopechaSolve: (params: { type: string; sitekey: string; pageurl: string }): Promise<string | null> =>
      ipcRenderer.invoke('captcha:nopecha-solve', params),
    openWindow: (params: { provider?: string; pageUrl: string; sourceUrl?: string }): Promise<string | null> =>
      ipcRenderer.invoke('captcha:open-window', params),
    submit: (id: string, token: string): Promise<void> =>
      fetchBackend(`/captcha/submit`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ id, token }),
      }).then(() => undefined),
  },

  // ── Mirrors ──────────────────────────────────────────────────────────────
  mirrors: {
    /**
     * Inicia a busca de mirrors via SSE no backend Rust.
     * Os eventos chegam via onEvent; resolve quando termina.
     */
    search: async (filename: string): Promise<void> => {
      activeMirrorSearchSeq += 1
      activeMirrorController?.abort()
      const controller = new AbortController()
      const searchSeq = activeMirrorSearchSeq
      activeMirrorController = controller
      const port = await getPort()
      const url = `http://127.0.0.1:${port}/mirrors/search?filename=${encodeURIComponent(filename)}`
      const emit = (ev: MirrorRendererEvent) => {
        if (searchSeq !== activeMirrorSearchSeq) {
          return
        }
        for (const h of mirrorEventHandlers) h(ev)
      }

      const parseMessage = (payload: string): boolean => {
        if (!payload) {
          return false
        }

        try {
          const data = JSON.parse(payload) as Record<string, unknown>
          if (data.type === 'start') {
            emit({
              type: 'start',
              payload: {
                filename: String(data.filename ?? ''),
                total: Number(data.total ?? 0),
              },
            })
          } else if (data.type === 'progress') {
            emit({
              type: 'progress',
              payload: {
                current: Number(data.current ?? 0),
                total: Number(data.total ?? 0),
                searcher: String(data.searcher ?? ''),
                phase: String(data.phase ?? ''),
                newResults: Number(data.newResults ?? 0),
                totalResults: Number(data.totalResults ?? 0),
                rawResults: Number(data.rawResults ?? 0),
                rejectedResults: Number(data.rejectedResults ?? 0),
                durationMs: Number(data.durationMs ?? 0),
                error: typeof data.error === 'string' ? data.error : null,
              },
            })
          } else if (data.type === 'log') {
            emit({ type: 'log', payload: String(data.payload ?? '') })
          } else if (data.type === 'result') {
            emit({
              type: 'result',
              payload: {
                url: String(data.url ?? ''),
                source: String(data.source ?? ''),
                hoster: typeof data.hoster === 'string' ? data.hoster : null,
                score: Number(data.score ?? 0),
              },
            })
          } else if (data.type === 'done') {
            emit({
              type: 'done',
              payload: {
                filename: String(data.filename ?? ''),
                searchers: Number(data.searchers ?? 0),
                total: Number(data.total ?? 0),
                hosters: Number(data.hosters ?? 0),
                durationMs: Number(data.durationMs ?? 0),
              },
            })
            return true
          }
        } catch {
          // Ignora mensagens SSE malformadas.
        }

        return false
      }

      try {
        const response = await fetch(url, {
          headers: { Accept: 'text/event-stream' },
          cache: 'no-store',
          signal: controller.signal,
        })

        if (!response.ok || !response.body) {
          emit({ type: 'error', payload: 'Falha ao iniciar stream de mirrors' })
          return
        }

        const reader = response.body.getReader()
        const decoder = new TextDecoder()
        let buffer = ''
        let doneReceived = false

        try {
          while (true) {
            const { done, value } = await reader.read()
            if (done) {
              break
            }

            buffer += decoder.decode(value, { stream: true })

            const parsed = splitSseMessages(buffer)
            buffer = parsed.rest

            for (const message of parsed.messages) {
              if (parseMessage(message.data)) {
                doneReceived = true
                await reader.cancel().catch(() => undefined)
                return
              }
            }
          }

          const trailing = buffer.trim()
          if (trailing && parseMessage(trailing)) {
            doneReceived = true
          }
        } finally {
          reader.releaseLock()
        }
        if (!doneReceived && !controller.signal.aborted) {
          emit({ type: 'error', payload: 'Conexão SSE perdida' })
        }
      } catch {
        if (!controller.signal.aborted) {
          emit({ type: 'error', payload: 'Conexão SSE perdida' })
        }
      } finally {
        if (activeMirrorController === controller) {
          activeMirrorController = null
        }
      }
    },

    abort: (): void => {
      activeMirrorSearchSeq += 1
      activeMirrorController?.abort()
      activeMirrorController = null
    },

    /**
     * Subscreve a eventos de progresso da busca:
     *   { type: 'start',    payload: { filename, total } }
     *   { type: 'progress', payload: { current, total, searcher, phase, ... } }
     *   { type: 'log',      payload: string }
     *   { type: 'result',   payload: { url, source, hoster, score } }
     *   { type: 'done',     payload: { filename, searchers, total, hosters, durationMs } }
     *   { type: 'error',    payload: string }
     * Retorna função de cleanup.
     */
    onEvent: (cb: (event: MirrorRendererEvent) => void) => {
      mirrorEventHandlers.push(cb)
      return () => {
        const idx = mirrorEventHandlers.indexOf(cb)
        if (idx >= 0) mirrorEventHandlers.splice(idx, 1)
      }
    },
  },

  // Compatibilidade com código antigo
  getBackendPort: (): Promise<number> => ipcRenderer.invoke('backend:getPort'),
}

function normalizeModuleId(provider: unknown): string {
  const raw = String(provider ?? '').trim().toLowerCase()
  switch (raw) {
    case 'google drive':
    case 'googledrive':
    case 'gdrive':
      return 'gdrive'
    case 'mediafire':
      return 'mediafire'
    case 'mega':
      return 'mega'
    case 'pixeldrain':
      return 'pixeldrain'
    case '1fichier':
    case 'fichier':
      return 'fichier'
    case 'drime':
      return 'drime'
    case 'rapidgator':
      return 'rapidgator'
    case 'brupload':
      return 'brupload'
    case 'brfiles':
      return 'brfiles'
    case 'moondl':
      return 'moondl'
    case 'akirabox':
      return 'akirabox'
    case 'katfile':
      return 'katfile'
    case 'terabox':
      return 'terabox'
    case 'onedrive':
    case 'one drive':
      return 'onedrive'
    default:
      return raw || 'unknown'
  }
}

// Converte o formato de download do backend Rust para o formato esperado pelo renderer
function rustDownloadToItem(d: Record<string, unknown>) {
  const size = (d.size as number) ?? 0
  const bytes = (d.bytes_downloaded as number) ?? 0
  return {
    id: d.id,
    url: d.url,
    moduleId: normalizeModuleId(d.provider),
    title: d.filename,
    size,
    isFolder: d.is_folder ?? false,
    children: d.children ?? [],
    status: d.status,
    percent: size > 0 ? Math.floor((bytes / size) * 100) : 0,
    speedBps: d.speed_bps ?? 0,
    etaSec: d.eta_secs ?? 0,
    retryCount: d.retry_count ?? 0,
    maxRetries: d.max_retries ?? 0,
    retryAt: d.retry_at ? (d.retry_at as number) * 1000 : undefined,
    error: d.error ?? '',
    captchaType: d.captcha_type ?? undefined,
    captchaSitekey: d.captcha_sitekey ?? undefined,
    captchaPageUrl: d.captcha_page_url ?? undefined,
    outputPath: d.dest_path,
    priority: d.priority ?? 0,
    addedAt: ((d.created_at as number) ?? 0) * 1000,
    startedAt: d.started_at ? (d.started_at as number) * 1000 : undefined,
    completedAt: d.completed_at ? (d.completed_at as number) * 1000 : undefined,
    lastProgressAt: d.last_progress_at ? (d.last_progress_at as number) * 1000 : undefined,
  }
}

contextBridge.exposeInMainWorld('electron', electronAPI)
contextBridge.exposeInMainWorld('api', api)
