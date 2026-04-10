import { contextBridge, ipcRenderer } from 'electron'
import { electronAPI } from '@electron-toolkit/preload'

// API exposta para o renderer Vue via contextBridge
// contextBridge garante que o renderer não tem acesso direto ao Node.js
// Só as funções explicitamente listadas aqui ficam disponíveis no browser
const api = {
  // Retorna a porta onde o backend Rust está rodando
  // Chamado pelo api/client.ts no Vue para saber onde conectar
  getBackendPort: (): Promise<number> => ipcRenderer.invoke('backend:getPort'),

  // Abre um arquivo ou pasta no explorador do sistema operacional
  openPath: (path: string): Promise<string> => ipcRenderer.invoke('shell:openPath', path),
  showInFolder: (path: string): Promise<void> => ipcRenderer.invoke('shell:showInFolder', path),
}

contextBridge.exposeInMainWorld('electron', electronAPI)
contextBridge.exposeInMainWorld('api', api)
