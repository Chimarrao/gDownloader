# gDownloader

Cliente de downloads com backend Rust/Axum e frontend Electron/Vue.

## Destaques Visuais

- Ícones de arquivo e pasta em PNG no renderer.
- Base preparada para uso de PNG/SVG dedicados por provedor.
- Créditos e referências visuais:
  - `gui-bus/TechIcons`: https://github.com/gui-bus/TechIcons
  - Flaticon Cloud Storage Logo Pack: https://www.flaticon.com/packs/cloud-storage-logo

## Provedores

| Provedor | Formas de download | Integração | Testes | Observações |
| --- | --- | --- | --- | --- |
| Mega | Arquivo único (`/file/`, formato antigo `#!`) e pasta pública (`/folder/`) | Funcional | Unitários + testes reais | Para pasta, baixa os arquivos em sequência para um diretório local. |
| MediaFire | Arquivo único em página pública `/file/.../file` e pasta pública `/folder/.../...` | Funcional | Unitários + testes reais | Para pasta, usa a API pública `folder/get_content` e baixa os arquivos em sequência. |
| Google Drive | Arquivo único (`/file/d/...`, `/open?id=...`, `/uc?id=...`) | Parcial | Unitários | Fluxo atual depende de arquivo compartilhado publicamente. |
| PixelDrain | Arquivo único (`/u/...`) e lista/pasta (`/l/...#item=n`) | Funcional | Unitários + smoke real | Para links de lista, o item é escolhido via `#item=`; sem fragmento usa o primeiro arquivo. |

## Status dos módulos

| Módulo | Status | Cobertura atual | Observações |
| --- | --- | --- | --- |
| `backend/src/providers/mega.rs` | Integrado | Parsing + testes reais de arquivo e pasta | Pasta pública agregada por tamanho total e download sequencial. |
| `backend/src/providers/mediafire.rs` | Integrado | Parsing + `get_file_info` real + smoke de download real | Corrigido parsing do nome do arquivo e resolução de CDN atual. |
| `backend/src/providers/gdrive.rs` | Em progresso | Detecção e parsing testados | Falta smoke real de download e casos de confirmação para arquivos grandes. |
| `backend/src/providers/pixeldrain.rs` | Integrado | Detecção + `get_file_info` real + smoke de download real | Agora suporta links `/u/` e `/l/` com seleção por fragmento. |
| `backend/src/routes/downloads.rs` | Integrado | Cobertura indireta | Expansão de `~`, retries, pause/resume básico, cancelamento e reinício. |
| `src/renderer/src/components/LinkGrabber.vue` | Integrado | Sem teste automatizado | Lista links lidos, metadados, checkbox de seleção e preview de pasta. |
| `src/renderer/src/components/DownloadList.vue` | Integrado | Sem teste automatizado | Reidrata a fila, mostra ações por item, retry/restart/extrair e notificações. |
| `src/renderer/src/assets/file-icons.ts` | Integrado | Manual | Ícones PNG por categoria com fallback por extensão e MIME. |

## Testes

- Unitários e integração backend: `cd backend && cargo test`
- Renderer TypeScript: `npm run typecheck:web`
- Os links reais de teste ficam em `backend/.env.test.local`, que é ignorado no Git.
- Use `backend/.env.test.example` como modelo para preencher os links locais.
- Para manter a suíte viável, os smoke tests reais de arquivo baixam apenas um pequeno `Range`, mas passam pelo fluxo real de resolução do provedor e pelo host real.

## Funcionalidades atuais

- Download de arquivo e pasta para provedores suportados.
- Retry automático configurável e retry manual por item.
- Reinício manual do download.
- Pause/resume básico por item e em lote.
- Fila respeitando limite configurável de downloads simultâneos.
- Remoção individual e limpeza de concluídos/erros/cancelados persistidas no backend da sessão.
- Notificação nativa ao concluir.
- Extração manual para `zip` e famílias `tar.*` quando a ferramenta do sistema estiver disponível.
- Link Grabber com leitura prévia, seleção por checkbox e preview de pastas.

## Observações de plataforma

- macOS: fluxo principal validado durante o desenvolvimento.
- Linux e Windows: base técnica já preparada para empacotar o binário Rust junto do Electron e tratar melhor abrir pasta/arquivo, mas ainda precisam de rodada dedicada de smoke test e build final.
