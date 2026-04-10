// Cliente HTTP centralizado para comunicação com o backend Rust
// Todos os componentes Vue usam este módulo em vez de chamar fetch diretamente
// Equivalente a criar uma instância do axios com baseURL configurada

import type { Download, WsEvent } from './types'

export type { Download, WsEvent }
export type { DownloadStatus, FileInfo } from './types'

// URL base da API — preenchida após Electron informar a porta
let baseUrl = ''

// Referência à conexão WebSocket atual
let wsConnection: WebSocket | null = null

// Tipo da API exposta pelo preload (contextBridge)
// Declarado aqui para evitar erros de TypeScript com window.api
interface ElectronApi {
  getBackendPort: () => Promise<number>
  openPath: (path: string) => Promise<string>
  showInFolder: (path: string) => Promise<void>
}

// Acessa a API do Electron com tipagem segura
function getElectronApi(): ElectronApi {
  return (window as unknown as { api: ElectronApi }).api
}

// Inicializa o cliente com a porta do backend Rust
// Deve ser chamado uma vez ao montar o App.vue
export async function initClient(): Promise<void> {
  const port = await getElectronApi().getBackendPort()
  baseUrl = `http://127.0.0.1:${port}`
  console.log(`[API] Backend disponível em ${baseUrl}`)
}

// Verifica se o backend está respondendo (útil para mostrar indicador na UI)
export async function checkHealth(): Promise<boolean> {
  try {
    const resp = await fetch(`${baseUrl}/health`)
    return resp.ok
  } catch {
    return false
  }
}

// --- Downloads ---

// Adiciona um novo download à fila do backend
// Equivalente a POST /downloads
export async function addDownload(url: string, destDir: string): Promise<Download> {
  const resp = await fetch(`${baseUrl}/downloads`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ url, dest_dir: destDir }),
  })

  if (!resp.ok) {
    // O backend retorna { "error": "mensagem" } em caso de erro
    const body = await resp.json().catch(() => ({ error: 'Erro desconhecido' }))
    throw new Error(body.error ?? 'Erro ao adicionar download')
  }

  return resp.json()
}

// Lista todos os downloads (ativos, completos, com erro)
export async function listDownloads(): Promise<Download[]> {
  const resp = await fetch(`${baseUrl}/downloads`)
  if (!resp.ok) throw new Error('Erro ao listar downloads')
  return resp.json()
}

// Cancela um download pelo ID
export async function cancelDownload(id: string): Promise<void> {
  const resp = await fetch(`${baseUrl}/downloads/${id}`, { method: 'DELETE' })
  // 204 = sucesso sem body, 404 = já foi removido (ignoramos ambos)
  if (!resp.ok && resp.status !== 404) {
    throw new Error('Erro ao cancelar download')
  }
}

// --- WebSocket para progresso em tempo real ---

type WsHandler = (event: WsEvent) => void

// Conecta ao WebSocket do backend e chama onEvent para cada mensagem recebida
// Retorna uma função de cleanup para fechar a conexão
export function connectWebSocket(onEvent: WsHandler): () => void {
  // Troca http:// por ws:// para a URL do WebSocket
  const wsUrl = baseUrl.replace('http://', 'ws://') + '/ws'

  wsConnection = new WebSocket(wsUrl)

  wsConnection.onmessage = (msg) => {
    try {
      const event: WsEvent = JSON.parse(msg.data as string)
      onEvent(event)
    } catch {
      console.warn('[WS] Mensagem inválida recebida:', msg.data)
    }
  }

  wsConnection.onerror = () => {
    console.error('[WS] Erro na conexão WebSocket')
  }

  wsConnection.onclose = () => {
    console.log('[WS] Conexão com backend encerrada')
  }

  // Retorna função de cleanup (para usar em onUnmounted() no Vue)
  return () => {
    wsConnection?.close()
    wsConnection = null
  }
}

// --- Utilitários ---

// Abre um arquivo no aplicativo padrão do sistema
export function openFile(filePath: string): Promise<string> {
  return getElectronApi().openPath(filePath)
}

// Abre a pasta contendo o arquivo no explorador do sistema
export function showInFolder(filePath: string): Promise<void> {
  return getElectronApi().showInFolder(filePath)
}
