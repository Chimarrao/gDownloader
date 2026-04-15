import { contextBridge, ipcRenderer } from 'electron'
import { electronAPI } from '@electron-toolkit/preload'

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

    // Stub de autenticação (não implementado)
    isLoggedIn: async (_moduleId: string) => false,
  },

  // --- Downloads ---
  downloads: {
    // Adiciona download à fila do backend
    add: async (url: string, _moduleId: string, _title: string, _size: number, destDir: string) => {
      const resp = await fetchBackend('/downloads', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ url, dest_dir: destDir }),
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

    // Subscreve a eventos de progresso via WebSocket
    on: (channel: string, cb: (data: unknown) => void) => {
      let ws: WebSocket | null = null

      getPort().then((port) => {
        ws = new WebSocket(`ws://127.0.0.1:${port}/ws`)
        ws.onmessage = (msg) => {
          try {
            const event = JSON.parse(msg.data as string)
            // Mapeamos os eventos do backend para os canais esperados pelo renderer
            if (channel === 'download:progress' && event.type === 'progress') {
              cb(event)
            } else if (channel === 'download:complete' && event.type === 'complete') {
              cb(event)
            } else if (channel === 'download:error' && event.type === 'error') {
              cb(event)
            }
          } catch {
            // Ignora mensagens malformadas
          }
        }
      })

      // Retorna função de cleanup
      return () => {
        ws?.close()
      }
    },
  },

  // --- Settings ---
  settings: {
    load: () => ipcRenderer.invoke('settings:load'),
    save: (s: unknown) => ipcRenderer.invoke('settings:save', s),
  },

  // --- Histórico ---
  loadHistory: () => ipcRenderer.invoke('history:load'),
  saveHistory: (items: unknown) => ipcRenderer.invoke('history:save', items),
  clearHistory: () => ipcRenderer.invoke('history:clear'),

  // --- Shell ---
  openPath: (path: string): Promise<string> => ipcRenderer.invoke('shell:openPath', path),
  showInFolder: (path: string): Promise<void> => ipcRenderer.invoke('shell:showInFolder', path),

  // Compatibilidade com código antigo
  getBackendPort: (): Promise<number> => ipcRenderer.invoke('backend:getPort'),
}

// Converte o formato de download do backend Rust para o formato esperado pelo renderer
function rustDownloadToItem(d: Record<string, unknown>) {
  const size = (d.size as number) ?? 0
  const bytes = (d.bytes_downloaded as number) ?? 0
  return {
    id: d.id,
    url: d.url,
    moduleId: d.provider,
    title: d.filename,
    size,
    status: d.status,
    percent: size > 0 ? Math.floor((bytes / size) * 100) : 0,
    speedBps: d.speed_bps ?? 0,
    etaSec: d.eta_secs ?? 0,
    error: d.error ?? '',
    outputPath: d.dest_path,
    addedAt: ((d.created_at as number) ?? 0) * 1000,
  }
}

contextBridge.exposeInMainWorld('electron', electronAPI)
contextBridge.exposeInMainWorld('api', api)
