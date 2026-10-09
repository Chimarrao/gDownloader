// Implementação alternativa de `window.api` pra quando o app roda FORA do
// Electron — acessado remotamente pela LAN/celular, direto no navegador. Lá
// não existe contextBridge/ipcRenderer; tudo aqui vira fetch/WebSocket
// relativo à própria origem, que o servidor remoto (src/main/remote-access-server.ts)
// repassa pro backend Rust/Go de verdade, já autenticado pela sessão/cookie.
//
// Cobre o essencial pro uso remoto (fila de downloads + Capturador de Links);
// qualquer método não implementado aqui cai num stub genérico que rejeita com
// uma mensagem clara, em vez de travar a tela — recursos só-desktop (diálogos
// nativos, bandeja, Tor, torrents, etc.) ficam assim até serem portados.

function apiError(path: string, body: unknown, status: number): Error {
  const message =
    body && typeof body === 'object' && 'error' in body && typeof (body as { error: unknown }).error === 'string'
      ? (body as { error: string }).error
      : `HTTP ${status} em ${path}`
  return new Error(message)
}

async function call<T>(path: string, init?: RequestInit): Promise<T> {
  const resp = await fetch(path, init)
  if (resp.status === 204) return undefined as T
  const body = await resp.json().catch(() => null)
  if (!resp.ok) throw apiError(path, body, resp.status)
  return body as T
}

async function action(id: string, name: string): Promise<void> {
  await call<void>(`/api/downloads/${encodeURIComponent(id)}/${name}`, { method: 'POST' })
}

// Mesmo mapeamento de backend/src/preload/index.ts (normalizeModuleId) — o
// shim web precisa devolver o mesmo moduleId pros componentes reconhecerem o
// provider/ícone certo.
function normalizeModuleId(provider: unknown): string {
  const raw = String(provider ?? '').trim().toLowerCase()
  switch (raw) {
    case 'google drive':
    case 'googledrive':
    case 'gdrive':
      return 'gdrive'
    case '1fichier':
    case 'fichier':
      return 'fichier'
    case 'onedrive':
    case 'one drive':
      return 'onedrive'
    default:
      return raw || 'unknown'
  }
}

// Mesmo mapeamento de campos snake_case (Rust) -> camelCase (DownloadItem) de
// backend/src/preload/index.ts (rustDownloadToItem) — precisa ficar idêntico,
// senão os componentes recebem campos undefined.
function rustDownloadToItem(d: Record<string, unknown>): Record<string, unknown> {
  const size = (d.size as number) ?? 0
  const bytes = (d.bytes_downloaded as number) ?? 0
  return {
    id: d.id,
    url: d.url,
    moduleId: normalizeModuleId(d.provider),
    title: d.filename,
    size,
    durationSecs: typeof d.duration_secs === 'number' ? d.duration_secs : undefined,
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
    errorKind: typeof d.error_kind === 'string' ? d.error_kind : undefined,
    expectedHash: d.expected_hash ?? undefined,
    captchaType: d.captcha_type ?? undefined,
    captchaSitekey: d.captcha_sitekey ?? undefined,
    captchaPageUrl: d.captcha_page_url ?? undefined,
    outputPath: d.dest_path,
    priority: d.priority ?? 0,
    pinned: Boolean(d.pinned),
    packageId: typeof d.package_id === 'string' ? d.package_id : undefined,
    parallelParts: Number(d.parallel_parts ?? 1),
    speedLimitKib: Number(d.speed_limit_kib ?? 0),
    sequential: Boolean(d.sequential),
    networkRoute: d.network_route ?? undefined,
    addedAt: ((d.created_at as number) ?? 0) * 1000,
    startedAt: d.started_at ? (d.started_at as number) * 1000 : undefined,
    completedAt: d.completed_at ? (d.completed_at as number) * 1000 : undefined,
    lastProgressAt: d.last_progress_at ? (d.last_progress_at as number) * 1000 : undefined,
    thumbnailUrl: typeof d.thumbnail_url === 'string' ? d.thumbnail_url : undefined,
    thumbnailData: typeof d.thumbnail_data === 'string' ? d.thumbnail_data : undefined,
    channelName: typeof d.channel_name === 'string' ? d.channel_name : undefined,
    channelThumbnailUrl: typeof d.channel_thumbnail_url === 'string' ? d.channel_thumbnail_url : undefined,
    autoTorOnLimit: Boolean(d.auto_tor_on_limit),
    torRequired: Boolean(d.tor_required),
  }
}

type DownloadChannel =
  | 'download:progress'
  | 'download:verifying'
  | 'download:status'
  | 'download:complete'
  | 'download:error'
  | 'download:cancelled'
  | 'download:duplicate'

type Listener = (data: unknown) => void

// --- Poller compartilhado: substitui o WebSocket de progresso em tempo real
// por polling de /api/state. Mobile tolera ~1.5s de atraso numa barra de
// progresso; isso evita ter que montar um proxy de WebSocket bruto pro
// primeiro lançamento remoto. ---
const listeners: Record<DownloadChannel, Set<Listener>> = {
  'download:progress': new Set(),
  'download:verifying': new Set(),
  'download:status': new Set(),
  'download:complete': new Set(),
  'download:error': new Set(),
  'download:cancelled': new Set(),
  'download:duplicate': new Set(),
}
const lastStatusById = new Map<string, string>()
let pollTimer: ReturnType<typeof setInterval> | null = null

function dispatch(channel: DownloadChannel, payload: unknown): void {
  for (const listener of listeners[channel]) listener(payload)
}

function hasAnyListener(): boolean {
  return Object.values(listeners).some((set) => set.size > 0)
}

async function pollOnce(): Promise<void> {
  const state = await call<{ downloads?: Record<string, unknown>[] }>('/api/state').catch(() => null)
  const rows = state?.downloads ?? []
  for (const row of rows) {
    const id = String(row.id ?? '')
    if (!id) continue
    const status = String(row.status ?? '')
    const bytes = Number(row.bytes_downloaded ?? 0)
    const total = Number(row.size ?? 0)

    dispatch('download:progress', {
      id,
      bytes,
      total,
      speed: Number(row.speed_bps ?? 0),
      eta: Number(row.eta_secs ?? 0),
      status,
    })

    const previous = lastStatusById.get(id)
    if (previous !== status) {
      lastStatusById.set(id, status)
      dispatch('download:status', {
        id,
        status,
        error: row.error ?? null,
        retry_at: row.retry_at ?? null,
        captcha_type: row.captcha_type ?? null,
        captcha_sitekey: row.captcha_sitekey ?? null,
        captcha_page_url: row.captcha_page_url ?? null,
      })
      if (previous !== undefined) {
        if (status === 'complete') dispatch('download:complete', { id, path: row.dest_path ?? '' })
        else if (status === 'error') dispatch('download:error', { id, message: String(row.error ?? '') })
        else if (status === 'cancelled') dispatch('download:cancelled', { id })
      }
    }
  }
}

function ensurePolling(): void {
  if (pollTimer) return
  void pollOnce()
  pollTimer = setInterval(() => {
    if (!hasAnyListener()) {
      if (pollTimer) clearInterval(pollTimer)
      pollTimer = null
      return
    }
    void pollOnce()
  }, 1500)
}

function on(channel: DownloadChannel, cb: Listener): () => void {
  listeners[channel].add(cb)
  ensurePolling()
  return () => listeners[channel].delete(cb)
}

// --- Stub genérico: qualquer método não implementado aqui vira isto —
// funciona chamado direto (window.api.foo()) OU como namespace
// (window.api.foo.bar()), sempre rejeitando com uma mensagem clara em vez de
// travar a tela. ---
function createUnavailableHandle(path: string): unknown {
  const fn = (): Promise<never> =>
    Promise.reject(new Error(`"${path}" ainda não está disponível pelo acesso remoto.`))
  return new Proxy(fn, {
    get(_target, prop) {
      if (typeof prop !== 'string') return undefined
      return createUnavailableHandle(`${path}.${prop}`)
    },
    apply() {
      return fn()
    },
  })
}

export function createWebApiShim(): Window['api'] {
  const real = {
    modules: {
      list: () => call('/api/providers').catch(() => []),
      detect: (url: string) => call(`/api/modules/detect?url=${encodeURIComponent(url)}`).catch(() => null),
      fileInfo: (_moduleId: string, url: string) => call(`/api/modules/file-info?url=${encodeURIComponent(url)}`),
      cachedFileInfo: (_moduleId: string, url: string) =>
        call(`/api/modules/cached-file-info?url=${encodeURIComponent(url)}`).catch(() => null),
      isLoggedIn: () => Promise.resolve(false),
    },
    captureState: {
      load: () =>
        call<{ text?: string }>('/api/capture-state')
          .then((r) => r.text ?? '')
          .catch(() => ''),
      save: (text: string) =>
        call('/api/capture-state', {
          method: 'PUT',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ text }),
        }).then(() => undefined).catch(() => undefined),
    },
    downloads: {
      add: async (
        url: string,
        moduleId: string,
        _title: string,
        _size: number,
        destDir: string,
        selectedChildren?: string[],
        expectedHash?: { algorithm: string; value: string },
        duplicateActionOverride?: string,
        filename?: string,
        torRequired?: boolean,
      ) => {
        const created = await call<Record<string, unknown>>('/api/downloads', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            url,
            destDir,
            selectedChildren,
            expectedHash,
            duplicateAction: duplicateActionOverride,
            filename,
            torRequired,
            parallelParts: moduleId === 'youtube' ? 1 : undefined,
          }),
        })
        return rustDownloadToItem(created)
      },
      cancel: (id: string) => action(id, 'remove'),
      pause: (id: string) => action(id, 'pause'),
      resume: (id: string) => action(id, 'resume'),
      retry: (id: string) => action(id, 'retry'),
      restart: (id: string) => action(id, 'restart'),
      force: (id: string) => action(id, 'force'),
      togglePin: (id: string) => action(id, 'pin'),
      remove: (id: string) => action(id, 'remove'),
      removeWithFiles: (id: string) => action(id, 'remove-with-files'),
      setPriority: () => Promise.resolve(),
      setAutoTor: () => Promise.resolve(),
      setTorRequired: () => Promise.resolve(),
      setSpeedLimit: () => Promise.resolve(),
      setParallelParts: () => Promise.resolve(),
      pauseAll: () => Promise.resolve({ paused: 0 }),
      resumeAll: () => Promise.resolve({ resumed: 0 }),
      move: () => Promise.resolve(),
      clearFinished: () => Promise.resolve(),
      duplicates: () => Promise.resolve([]),
      events: () => Promise.resolve([]),
      list: () =>
        call<{ downloads?: Record<string, unknown>[] }>('/api/state')
          .then((s) => (s.downloads ?? []).map(rustDownloadToItem))
          .catch(() => []),
      checkKnownUrls: (urls: string[]) =>
        call('/api/downloads/known-urls', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ urls }),
        }).catch(() => ({})),
      on,
    },
    settings: {
      load: () => call('/api/settings').catch(() => null),
      save: (s: unknown) =>
        call('/api/settings', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(s),
        }),
      chooseDirectory: () => Promise.resolve(''),
    },
    system: {
      notify: () => Promise.resolve(false),
      diskSpace: (path: string) =>
        call<{ available?: number; total?: number }>(`/api/system/disk-space?path=${encodeURIComponent(path)}`)
          .then((d) => ({ path, freeBytes: d.available ?? 0, totalBytes: d.total ?? 0 }))
          .catch(() => ({ path, freeBytes: 0, totalBytes: 0 })),
      metrics: () =>
        Promise.resolve({ memoryUsed: 0, memoryTotal: 0, cpuPercent: 0, appMemoryUsed: 0, appCpuPercent: 0 }),
    },
    clipboard: {
      writeText: (text: string) =>
        navigator.clipboard
          ?.writeText(text)
          .then(() => true)
          .catch(() => false) ?? Promise.resolve(false),
      onLinkDetected: () => () => undefined,
    },
    tray: {
      updateStats: () => undefined,
    },
  }

  return new Proxy(real, {
    get(target, prop) {
      if (typeof prop !== 'string') return undefined
      if (prop in target) return (target as Record<string, unknown>)[prop]
      return createUnavailableHandle(prop)
    },
  }) as unknown as Window['api']
}
