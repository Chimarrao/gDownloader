# gDownloader — Melhorias e Correções (Abril 2026)

## Escopo

13 itens divididos em 4 grupos: bugs de UI, bugs de lógica, nova feature (gráfico de velocidade) e trabalho de ícones/assets.

---

## Grupo 1 — Bugs de UI

### 1. Scroll em LinkGrabber e DownloadList
- Adicionar `overflow-y: auto` + `max-height: 100%` nos containers de lista de ambos os componentes.
- O Electron não propaga scroll automaticamente em flex columns sem height explícito.

### 2. Piscadas ao adicionar 3+ itens simultâneos
- Causa: eventos `download:progress` via WebSocket chegam antes de `list()` terminar, não encontram o item e causam salto de estado quando ele aparece.
- Fix: enfileirar eventos recebidos durante hidratação inicial; aplicar a fila após `list()` resolver.

### 3. Itens não esticam/encolhem com a janela
- Cards de download têm largura fixa. Trocar para `width: 100%` / `flex: 1 1 0` no container.

### 8. Folder toggle trava/buга UI
- Causa: `v-for` nos filhos de pasta usa índice como key (ou sem key), causando re-render total ao expandir/recolher.
- Fix: usar `child.filename` como key; garantir que expand/collapse não muta o array `items` diretamente.

---

## Grupo 2 — Bugs de Lógica

### 5. Speed limiter global não funciona
- O `speed_limit_kib` das settings precisa ser enviado no payload `POST /downloads` ao criar cada download.
- Verificar se o frontend inclui `speedLimitKib` no body de criação.
- Verificar se o provider MediaFire lê e respeita o campo `speed_limit_kib` durante o download (throttle real).
- Se o provider não implementa throttle: adicionar lógica de throttle no loop de download (sleep proporcional por chunk).

### 6. Formato de tempo humano
- Atual: `2498:47 restante` (MM:SS sem tratar horas > 99).
- Novo: função utilitária `formatEta(secs: number): string`:
  - `< 60s` → `"Xs"`
  - `< 3600s` → `"Xm Ys"`
  - `>= 3600s` → `"Xh Ym"`
- Resultado esperado: `41h 38m` em vez de `2498:47`.
- Aplicar em todos os lugares que exibem ETA (item pai e filhos).

### 7. Progresso total não atualiza em pastas
- Causa provável: filhos criados sem `bytes_downloaded: 0` (undefined vs 0 quebra a soma).
- Fix: garantir inicialização de `bytes_downloaded: 0` em todos os filhos ao criar o download.
- Verificar que o backend envia o acumulado correto no campo `bytes` do evento Progress para o item pai.

### 9. Notificação ao concluir não funciona
- Cadeia: `DownloadList` emite `'download-complete'` → `App.vue` chama `window.api.system.notify()`.
- Adicionar logs em cada ponto da cadeia para identificar onde quebra.
- Verificar se `system.notify` está exposto no `preload/index.ts`.
- Verificar se o Electron `Notification` tem permissão no contexto macOS/Windows.

---

## Grupo 3 — Gráfico de Velocidade Global

### 4. Widget de velocidade + sparkline (top-right)

**Componente:** `SpeedWidget.vue`

**Dados:**
- Buffer circular de 60 pontos (1 ponto/segundo = ~1 min de histórico), mantido em estado reativo no `App.vue`.
- `setInterval` de 1s empurra a velocidade global atual (soma das speeds de downloads ativos).
- `DownloadList.vue` emite evento `'global-speed'` a cada evento `download:progress` recebido, com a soma instantânea.

**Props do componente:**
```typescript
props: {
  speedHistory: number[]   // 60 pontos em bytes/s
  currentSpeed: number     // bytes/s atual
}
```

**Renderização:**
- SVG `<polyline>` inline (sem lib de charting) — ~160×48px.
- Linha de download: cor do tema (roxo).
- Linha de upload reservada em cinza (sempre `—` por ora).
- Texto abaixo: `↓ 3.2 MB/s` e `↑ —`.

**Fluxo:**
```
WS progress events
  → DownloadList soma speeds de todos os itens ativos
  → emite 'global-speed' com valor
  → App.vue atualiza buffer + currentSpeed
  → passa como props para SpeedWidget
```

**Posição:** Header direito do `App.vue`, alinhado com as abas.

---

## Grupo 4 — Ícones e Assets

### 10. Converter ícones de arquivo para SVG

**Fonte:** `@phosphor-icons/core` (npm, MIT).

**Mapeamento Phosphor → categoria:**
| Categoria | Ícone Phosphor sugerido |
|-----------|------------------------|
| folder | `Folder` |
| file | `File` |
| video | `FilmStrip` |
| audio | `MusicNote` |
| archive | `FileZip` |
| image | `Image` |
| pdf | `FilePdf` |
| doc | `FileDoc` |
| sheet | `FileXls` |
| slides | `FilePpt` |
| text | `FileText` |
| code | `FileCode` |
| disk | `HardDrive` |
| app | `AppWindow` |
| database | `Database` |
| **font** (novo) | `TextAa` |
| **subtitle** (novo) | `Subtitles` |

**Mudanças em `file-icons.ts`:**
- Remover imports de PNG.
- Importar SVGs do `@phosphor-icons/core` como strings.
- Interface `FileIconDef`: substituir campo `src: string` por `svg: string`.

**Mudanças nos componentes:**
- `DownloadList.vue`, `LinkGrabber.vue`: trocar `<img :src="icon.src">` por `<span v-html="icon.svg" class="file-icon">` com tamanho `24×24px`.

**Provider icons:**
- `pixeldrain.png` → converter para SVG inline (buscar SVG oficial ou criar geométrico simples).
- Demais providers já são SVG — verificar se seguem a nomenclatura correta.

### 11. Novas categorias de ícone

**Adicionadas:**
- `font`: ttf, otf, woff, woff2
- `subtitle`: srt, vtt, ass, ssa

**Removidas/ajustadas:**
- Torrent permanece em `archive` (sem categoria separada).
- Certificados (cer, crt, pem, p12, pfx) permanecem em `file`.

### 12. Renomear ícones para padrão minúsculo sem acento

Todos os arquivos em `file-icons/` e `provider-icons/`:
- Minúsculo, sem acento, sem espaço, extensão `.svg`.
- Exemplos: `video.svg`, `audio.svg`, `archive.svg`, `subtitle.svg`, `font.svg`, `folder.svg`.
- Nomes em inglês para todos (consistência com nomes atuais).

**Ao final:** Deletar a pasta `file-icons/` com os PNGs antigos após confirmar build funcionando.

### 13. Remover créditos de ícones externos do README

Remover da seção "Destaques Visuais":
- Menção a `gui-bus/TechIcons`
- Menção a Flaticon Cloud Storage Logo Pack

---

## Ordem de implementação sugerida

1. Bugs de UI (1, 2, 3, 8) — CSS/Vue, sem dependências
2. Bugs de lógica (6, 7, 9, 5) — do mais simples ao mais complexo
3. Gráfico de velocidade (4) — novo componente isolado
4. Ícones (10, 11, 12, 13) — trabalho de asset independente, deletar PNGs no final
