import { contextBridge, ipcRenderer } from 'electron'
import { electronAPI } from '@electron-toolkit/preload'
import type {
  AppSettingsSnapshot,
  ArchivePassword,
  CreateDownloadPackagePayload,
  DownloadEvent,
  DuplicateDownload,
  DuplicateGroup,
} from '../shared/types'
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
  | 'download:verifying'
  | 'download:status'
  | 'download:complete'
  | 'download:error'
  | 'download:cancelled'
  | 'download:duplicate'

type ClipboardLinkPayload = {
  url: string
  provider: string
  providerName?: string
}

const downloadListeners: Record<DownloadChannel, Set<(data: unknown) => void>> = {
  'download:progress': new Set(),
  'download:verifying': new Set(),
  'download:status': new Set(),
  'download:complete': new Set(),
  'download:error': new Set(),
  'download:cancelled': new Set(),
  'download:duplicate': new Set(),
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

  if (event.type === 'verifying') {
    dispatchDownloadEvent('download:verifying', event)
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
    return
  }

  if (event.type === 'duplicate_detected') {
    dispatchDownloadEvent('download:duplicate', event)
    window.dispatchEvent(new CustomEvent('duplicate-detected', { detail: event }))
    return
  }

  if (event.type === 'stats_tick') {
    // Dispatch as a custom DOM event so StatsView can listen without extra IPC plumbing
    window.dispatchEvent(new CustomEvent('stats-tick', { detail: event }))
  }
}

async function ensureDownloadsSocket(): Promise<void> {
  if (
    downloadsSocket &&
    (downloadsSocket.readyState === WebSocket.OPEN ||
      downloadsSocket.readyState === WebSocket.CONNECTING)
  ) {
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
      selectedChildren?: string[],
      expectedHash?: { algorithm: string; value: string },
      duplicateActionOverride?: 'ask' | 'skip' | 'rename' | 'always_download',
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
          expected_hash: expectedHash,
          duplicate_action: duplicateActionOverride ?? settings?.duplicateAction ?? 'ask',
        }),
      })
      if (!resp.ok) {
        const body = await resp.json().catch(() => ({ error: 'Erro desconhecido' }))
        if (resp.status === 409 && body.duplicate) {
          const duplicate = body.duplicate as DuplicateDownload
          window.dispatchEvent(new CustomEvent('duplicate-detected', { detail: duplicate }))
          const choice = window.prompt(
            `Arquivo já baixado em:\\n${duplicate.path || duplicate.filename}\\n\\n1 = baixar mesmo assim\\n2 = abrir existente\\n3 = salvar com sufixo _2`,
            '3',
          )
          if (choice === '1') {
            return api.downloads.add(
              url,
              _moduleId,
              _title,
              _size,
              destDir,
              selectedChildren,
              expectedHash,
              'always_download',
            )
          }
          if (choice === '2') {
            if (duplicate.path) {
              await ipcRenderer.invoke('shell:openPath', duplicate.path).catch(() => null)
            }
            throw new Error('Download ignorado: arquivo existente aberto')
          }
          if (choice === '3' || choice === null) {
            return api.downloads.add(
              url,
              _moduleId,
              _title,
              _size,
              destDir,
              selectedChildren,
              expectedHash,
              'rename',
            )
          }
        }
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

    setPriority: async (id: string, priority: number) => {
      await fetchBackend(`/downloads/${id}/priority`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ priority }),
      })
    },

    setSpeedLimit: async (id: string, speedLimitKib: number) => {
      await fetchBackend(`/downloads/${id}/speed-limit`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ speed_limit_kib: speedLimitKib }),
      })
    },

    remove: async (id: string) => {
      await fetchBackend(`/downloads/${id}/remove`, { method: 'DELETE' })
    },

    removeWithFiles: async (id: string) => {
      await fetchBackend(`/downloads/${id}/remove-with-files`, {
        method: 'DELETE',
      })
    },

    clearFinished: async () => {
      await fetchBackend('/downloads/finished', { method: 'DELETE' })
    },

    togglePin: async (id: string) => {
      await fetchBackend(`/downloads/${id}/pin`, { method: 'POST' })
    },

    duplicates: async (): Promise<DuplicateGroup[]> => {
      const resp = await fetchBackend('/downloads/duplicates')
      if (!resp.ok) return []
      return resp.json()
    },

    events: async (id: string): Promise<DownloadEvent[]> => {
      const resp = await fetchBackend(`/downloads/${id}/events`)
      if (!resp.ok) return []
      const rows = (await resp.json()) as Array<Record<string, unknown>>
      return rows.map((row) => ({
        id: Number(row.id),
        downloadId: String(row.download_id ?? row.downloadId ?? ''),
        kind: String(row.kind ?? ''),
        message: String(row.message ?? ''),
        createdAt: Number(row.created_at ?? row.createdAt ?? 0) * 1000,
      }))
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
    save: (s: AppSettingsSnapshot): Promise<AppSettingsSnapshot> =>
      ipcRenderer.invoke('settings:save', s),
    chooseDirectory: (): Promise<string> => ipcRenderer.invoke('dialog:chooseDirectory'),
  },

  remoteAccess: {
    info: (): Promise<{
      enabled: boolean
      running: boolean
      lanIp: string
      port: number
      username: string
      password: string
      url: string
      credentialUrl: string
      qrCodeDataUrl?: string
      error?: string
    }> => ipcRenderer.invoke('remote:info'),
    generateCredentials: (): Promise<NonNullable<AppSettingsSnapshot['remoteAccess']>> =>
      ipcRenderer.invoke('remote:generateCredentials'),
  },

  config: {
    testProxy: (): Promise<{ ip: string }> => ipcRenderer.invoke('config:test-proxy'),
  },

  auth: {
    isLoggedIn: (moduleId: string): Promise<boolean> =>
      ipcRenderer.invoke('auth:isLoggedIn', moduleId),
    login: (moduleId: string, params: Record<string, string>): Promise<void> =>
      ipcRenderer.invoke('auth:login', moduleId, params),
    logout: (moduleId: string): Promise<void> => ipcRenderer.invoke('auth:logout', moduleId),
    accountInfo: (moduleId: string): Promise<unknown> =>
      ipcRenderer.invoke('auth:accountInfo', moduleId),
  },

  // --- Histórico ---
  loadHistory: (filters?: unknown) => ipcRenderer.invoke('history:load', filters),
  saveHistory: (items: unknown) => ipcRenderer.invoke('history:save', items),
  appendHistory: (item: unknown) => ipcRenderer.invoke('history:append', item),
  historyHosts: () => ipcRenderer.invoke('history:hosts'),
  removeHistoryItem: (id: string) => ipcRenderer.invoke('history:remove', id),
  clearHistory: () => ipcRenderer.invoke('history:clear'),

  // --- Shell ---
  openPath: (path: string): Promise<string> => ipcRenderer.invoke('shell:openPath', path),
  showInFolder: (path: string): Promise<void> => ipcRenderer.invoke('shell:showInFolder', path),
  clipboard: {
    writeText: (text: string): Promise<boolean> => ipcRenderer.invoke('clipboard:writeText', text),
    onLinkDetected: (cb: (payload: ClipboardLinkPayload) => void) => {
      const handler = (_event: Electron.IpcRendererEvent, payload: ClipboardLinkPayload) =>
        cb(payload)
      ipcRenderer.on('clipboard:link-detected', handler)
      return () => ipcRenderer.removeListener('clipboard:link-detected', handler)
    },
  },
  system: {
    notify: (title: string, body?: string): Promise<boolean> =>
      ipcRenderer.invoke('system:notify', title, body),
  },
  logs: {
    tail: (maxLines?: number): Promise<{ path: string; lines: string[] }> =>
      ipcRenderer.invoke('logs:tail', maxLines),
    watch: (cb: (payload: { path: string; lines: string[] }) => void) => {
      const handler = (
        _event: Electron.IpcRendererEvent,
        payload: { path: string; lines: string[] },
      ) => cb(payload)
      ipcRenderer.on('logs:update', handler)
      ipcRenderer.send('logs:watch-start')
      return () => {
        ipcRenderer.removeListener('logs:update', handler)
        ipcRenderer.send('logs:watch-stop')
      }
    },
  },
  archive: {
    extract: (archivePath: string): Promise<string> =>
      ipcRenderer.invoke('archive:extract', archivePath),
    autoExtract: (
      archivePath: string,
      passwords: string[],
    ): Promise<{
      success: boolean
      outputDir?: string
      error?: string
      passwordUsed?: string
    }> => ipcRenderer.invoke('archive:auto-extract', archivePath, passwords),
  },
  archivePasswords: {
    list: async (): Promise<ArchivePassword[]> => {
      const rows = (await ipcRenderer.invoke('archive-passwords:list')) as Array<
        Record<string, unknown>
      >
      return rows.map((row) => ({
        password: String(row.password ?? ''),
        successCount: Number(row.successCount ?? row.success_count ?? 0),
        lastUsedAt:
          row.lastUsedAt || row.last_used_at
            ? Number(row.lastUsedAt ?? row.last_used_at) * 1000
            : undefined,
        source: String(row.source ?? 'manual'),
      }))
    },
    import: (passwords: string[]): Promise<void> =>
      ipcRenderer.invoke('archive-passwords:import', passwords),
    forget: (password: string): Promise<void> =>
      ipcRenderer.invoke('archive-passwords:forget', password),
  },
  packages: {
    list: async () => {
      const resp = await fetchBackend('/packages')
      if (!resp.ok) return []
      return resp.json()
    },
    create: async (payload: CreateDownloadPackagePayload) => {
      const resp = await fetchBackend('/packages', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload),
      })
      if (!resp.ok) {
        const body = await resp.json().catch(() => ({ error: 'Falha ao criar pacote' }))
        throw new Error(String(body.error ?? 'Falha ao criar pacote'))
      }
      return resp.json()
    },
    remove: async (id: string) => {
      await fetchBackend(`/packages/${encodeURIComponent(id)}`, {
        method: 'DELETE',
      })
    },
    assign: async (packageId: string, downloadId: string) => {
      await fetchBackend(
        `/packages/${encodeURIComponent(packageId)}/assign/${encodeURIComponent(downloadId)}`,
        { method: 'POST' },
      )
    },
    unassign: async (downloadId: string) => {
      await fetchBackend(`/packages/unassign/${encodeURIComponent(downloadId)}`, {
        method: 'DELETE',
      })
    },
  },
  links: {
    importContainer: async (
      file: File,
    ): Promise<Array<{ url: string; filename: string; size: number }>> => {
      const form = new FormData()
      form.append('file', file, file.name)
      const resp = await fetchBackend('/links/import-container', {
        method: 'POST',
        body: form,
      })
      if (!resp.ok) {
        const body = await resp.json().catch(() => ({ error: 'Falha ao importar container' }))
        throw new Error(String(body.error ?? 'Falha ao importar container'))
      }
      const data = (await resp.json()) as {
        links?: Array<{ url: string; filename: string; size: number }>
      }
      return data.links ?? []
    },
  },
  terabox: {
    netRequest: (params: {
      url: string
      method?: string
      headers?: Record<string, string>
      body?: string
    }): Promise<unknown> => ipcRenderer.invoke('terabox:net-request', params),
  },
  captcha: {
    nopechaSolve: (params: {
      type: string
      sitekey: string
      pageurl: string
    }): Promise<string | null> => ipcRenderer.invoke('captcha:nopecha-solve', params),
    openWindow: (params: {
      provider?: string
      pageUrl: string
      sourceUrl?: string
    }): Promise<string | null> => ipcRenderer.invoke('captcha:open-window', params),
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
          emit({
            type: 'error',
            payload: 'Falha ao iniciar stream de mirrors',
          })
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

  // Stats
  getRealtimeStats: async () => {
    try {
      const resp = await fetchBackend('/stats/realtime')
      if (!resp.ok) return { ticks: [] }
      return resp.json()
    } catch {
      return { ticks: [] }
    }
  },

  // Compatibilidade com código antigo
  getBackendPort: (): Promise<number> => ipcRenderer.invoke('backend:getPort'),

  tray: {
    updateStats: (data: { activeCount: number; speed: string }) =>
      ipcRenderer.send('tray:update-stats', data),
  },
}

// Listen for tray commands and forward as window events
ipcRenderer.on('tray:pause-all', () => window.dispatchEvent(new Event('tray-pause-all')))
ipcRenderer.on('tray:resume-all', () => window.dispatchEvent(new Event('tray-resume-all')))
ipcRenderer.on('tray:set-speed-limit', (_e, limit: number) =>
  window.dispatchEvent(new CustomEvent('tray-set-speed-limit', { detail: limit })),
)

function normalizeModuleId(provider: unknown): string {
  const raw = String(provider ?? '')
    .trim()
    .toLowerCase()
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
    expectedHash: d.expected_hash ?? undefined,
    captchaType: d.captcha_type ?? undefined,
    captchaSitekey: d.captcha_sitekey ?? undefined,
    captchaPageUrl: d.captcha_page_url ?? undefined,
    outputPath: d.dest_path,
    priority: d.priority ?? 0,
    pinned: Boolean(d.pinned),
    packageId: typeof d.package_id === 'string' ? d.package_id : undefined,
    parallelParts: Number(d.parallel_parts ?? 1),
    sequential: Boolean(d.sequential),
    addedAt: ((d.created_at as number) ?? 0) * 1000,
    startedAt: d.started_at ? (d.started_at as number) * 1000 : undefined,
    completedAt: d.completed_at ? (d.completed_at as number) * 1000 : undefined,
    lastProgressAt: d.last_progress_at ? (d.last_progress_at as number) * 1000 : undefined,
  }
}

contextBridge.exposeInMainWorld('electron', electronAPI)
contextBridge.exposeInMainWorld('api', api)
