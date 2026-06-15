# gDownloader — Melhorias e Correções — Plano de Implementação

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Corrigir 12 bugs/melhorias de UI, lógica e assets no gDownloader.

**Architecture:** Mudanças isoladas em frontend Vue (DownloadList, LinkGrabber, App, AppSettings), novo componente SpeedWidget, refactor de assets de ícones (PNG → SVG via Phosphor), e correções pontuais no backend Rust (speed limiter, formato de ETA).

**Tech Stack:** Vue 3 + TypeScript + Electron + Rust/Axum + Phosphor Icons

---

## Mapa de arquivos

| Arquivo | Alterações |
|---------|-----------|
| `src/renderer/src/App.vue` | CSS scroll, topbar SpeedWidget, buffer de velocidade global |
| `src/renderer/src/components/DownloadList.vue` | scroll, blinking, stretch, folder key, formatEta, folder progress, emit global-speed, span v-html ícones |
| `src/renderer/src/components/LinkGrabber.vue` | scroll, span v-html ícones |
| `src/renderer/src/components/SpeedWidget.vue` | criar — widget de velocidade global com sparkline |
| `src/renderer/src/assets/file-icons.ts` | reescrever — remover PNG imports, adicionar SVG strings, novas categorias font/subtitle |
| `src/renderer/src/assets/file-icons/` | substituir 15 PNGs por 17 SVGs (Phosphor), deletar PNGs |
| `src/renderer/src/assets/provider-icons.ts` | pixeldrain: PNG → SVG inline |
| `src/renderer/src/assets/provider-icons/pixeldrain.png` | deletar após conversão |
| `backend/src/providers/mod.rs` | apply_speed_limit: granularidade de chunk para throttle suave |
| `backend/src/providers/mediafire.rs` | loop de chunk: quebrar em sub-chunks de 64KB |
| `README.md` | remover seção de créditos de ícones externos |

---

## Task 1: Fix scroll em DownloadList e LinkGrabber

**Files:**
- Modify: `src/renderer/src/App.vue` (CSS `.app-root`, `.app-main`, `.downloads-panel`)
- Modify: `src/renderer/src/components/DownloadList.vue` (CSS `.download-list`, `.items-container`)
- Modify: `src/renderer/src/components/LinkGrabber.vue` (CSS `.captured-list`)

- [ ] **Step 1: Corrigir chain de altura no App.vue**

Em `src/renderer/src/App.vue`, no `<style scoped>`, fazer as seguintes mudanças:

```css
/* ANTES */
.app-root {
  min-height: 100vh;
  display: flex;
  flex-direction: column;
  background: var(--bg-primary);
  color: var(--text-primary);
}

.app-main {
  flex: 1;
  min-height: 0;
  display: flex;
  overflow: auto;
  padding: 18px;
}

.panel {
  flex: 1;
  min-width: 0;
  min-height: 0;
  display: flex;
  align-items: stretch;
}

.downloads-panel {
  width: 100%;
  max-width: 1040px;
  margin: 0 auto;
  align-self: flex-start;
  min-width: 0;
}

/* DEPOIS */
.app-root {
  height: 100vh;
  display: flex;
  flex-direction: column;
  background: var(--bg-primary);
  color: var(--text-primary);
  overflow: hidden;
}

.app-main {
  flex: 1;
  min-height: 0;
  display: flex;
  overflow: hidden;
  padding: 18px;
}

.panel {
  flex: 1;
  min-width: 0;
  min-height: 0;
  display: flex;
  align-items: stretch;
  overflow: hidden;
}

.downloads-panel {
  width: 100%;
  max-width: 1040px;
  margin: 0 auto;
  align-self: stretch;
  min-width: 0;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}
```

- [ ] **Step 2: Tornar DownloadList scrollável internamente**

Em `src/renderer/src/components/DownloadList.vue`, alterar:

```css
/* ANTES */
.download-list {
  display: flex;
  flex-direction: column;
  flex: 1;
  width: 100%;
  min-width: 0;
  min-height: 100%;
  gap: 0;
  align-self: stretch;
}

.items-container {
  display: flex;
  flex-direction: column;
  width: 100%;
  min-width: 0;
  align-self: stretch;
  gap: 10px;
}

/* DEPOIS */
.download-list {
  display: flex;
  flex-direction: column;
  flex: 1;
  width: 100%;
  min-width: 0;
  min-height: 0;
  gap: 0;
  align-self: stretch;
  overflow: hidden;
}

.items-container {
  display: flex;
  flex-direction: column;
  width: 100%;
  min-width: 0;
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  gap: 10px;
  padding-right: 2px;
}
```

- [ ] **Step 3: Tornar LinkGrabber scrollável**

Em `src/renderer/src/components/LinkGrabber.vue`, localizar a classe `.captured-list` no `<style scoped>` e adicionar/alterar:

```css
.captured-list {
  /* manter propriedades existentes, adicionar: */
  overflow-y: auto;
  flex: 1;
  min-height: 0;
}
```

Também garantir que `.link-grabber` (root do componente) tem `overflow: hidden; display: flex; flex-direction: column; flex: 1; min-height: 0`.

- [ ] **Step 4: Testar scroll manualmente**

Adicionar 10+ downloads via Link Grabber (pode usar URLs fictícias para ver o efeito). Verificar que a lista rola com o scroll do mouse sem rolar a janela toda.

- [ ] **Step 5: Commit**

```bash
git add src/renderer/src/App.vue src/renderer/src/components/DownloadList.vue src/renderer/src/components/LinkGrabber.vue
git commit -m "fix: enable inner scroll in download list and link grabber"
```

---

## Task 2: Fix "piscadas" ao adicionar 3+ downloads simultâneos

**Files:**
- Modify: `src/renderer/src/components/DownloadList.vue` (script section)

**Causa:** Eventos `download:progress` chegam via WebSocket antes da lista ser hidratada com os novos itens. Quando o evento não encontra o item, chama `void hydrate()`. Múltiplas chamadas concorrentes a `hydrate()` mutam `items.value` simultaneamente, causando saltos de render.

- [ ] **Step 1: Adicionar flag de hidratação e debounce**

No `<script setup>` de `DownloadList.vue`, logo após as declarações de state existentes:

```typescript
// Adicionar estas duas linhas após: const unsubs: Array<() => void> = []
let hydrateQueued = false
let hydrateInFlight = false
```

- [ ] **Step 2: Substituir a função `hydrate` por versão segura**

Localizar a função `async function hydrate(): Promise<void>` (linha ~413) e substituir por:

```typescript
async function hydrate(): Promise<void> {
  if (hydrateInFlight) {
    hydrateQueued = true
    return
  }
  hydrateInFlight = true
  try {
    const fresh: DownloadItem[] = await window.api.downloads.list().catch(() => [])
    const freshById = new Map<string, DownloadItem>(fresh.map((item) => [item.id, item] as const))

    for (let i = items.value.length - 1; i >= 0; i--) {
      if (!freshById.has(items.value[i].id)) {
        items.value.splice(i, 1)
      }
    }

    for (const freshItem of fresh) {
      const idx = items.value.findIndex((i) => i.id === freshItem.id)
      if (idx >= 0) {
        Object.assign(items.value[idx], freshItem)
      } else {
        items.value.push(freshItem)
      }
    }

    emit('count-change', items.value.length)
  } finally {
    hydrateInFlight = false
    if (hydrateQueued) {
      hydrateQueued = false
      void hydrate()
    }
  }
}
```

- [ ] **Step 3: Verificar que os eventos de progresso não causam hydrate durante a hidratação inicial**

No handler `onMounted`, garantir que a chamada `await hydrate()` acontece ANTES de registrar os listeners. Isso já é o caso no código atual (os `unsubs.push(...)` vêm após `await hydrate()`). Confirmar visualmente que a ordem está correta.

- [ ] **Step 4: Testar**

Adicionar 5 links via Link Grabber de uma vez. Observar que a lista aparece suavemente sem piscadas.

- [ ] **Step 5: Commit**

```bash
git add src/renderer/src/components/DownloadList.vue
git commit -m "fix: prevent concurrent hydrate calls causing list flicker"
```

---

## Task 3: Items esticam/encolhem com a janela + Fix folder toggle freeze

**Files:**
- Modify: `src/renderer/src/components/DownloadList.vue` (CSS)

**Problema stretch:** `.download-card` precisa de `width: 100%` explícito para garantir que preenche a coluna em todos os casos.

**Problema folder toggle:** Quando a pasta expande/recolhe, a `transition-group` aplica `item-move` (CSS transform) em TODOS os cards filhos, causando animações erráticas durante mudanças de altura. Fix: usar `v-show` nos filhos para evitar add/remove de DOM.

- [ ] **Step 1: Adicionar `width: 100%` ao download-card**

Em `DownloadList.vue` style, na classe `.download-card`:

```css
.download-card {
  display: flex;
  align-items: flex-start;
  gap: 12px;
  padding: 14px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: 12px;
  transition: border-color 0.2s ease, box-shadow 0.2s ease;
  position: relative;
  overflow: hidden;
  width: 100%;         /* ADICIONAR */
  box-sizing: border-box;  /* ADICIONAR */
}
```

- [ ] **Step 2: Trocar `v-if` por `v-show` nos folder-children**

No template de `DownloadList.vue`, localizar:

```html
<div
  v-if="item.isFolder && isExpanded(item.id) && (item.children?.length ?? 0) > 0"
  class="folder-children"
>
```

Substituir por:

```html
<div
  v-show="item.isFolder && isExpanded(item.id) && (item.children?.length ?? 0) > 0"
  class="folder-children"
>
```

- [ ] **Step 3: Adicionar transição suave aos folder-children**

Em `DownloadList.vue` style, adicionar/alterar `.folder-children`:

```css
.folder-children {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-top: 4px;
  padding: 8px 10px;
  border: 1px solid color-mix(in srgb, var(--border-color) 70%, transparent);
  border-radius: 10px;
  background: color-mix(in srgb, var(--bg-card) 70%, transparent);
  overflow: hidden;
  transition: max-height 0.25s ease, opacity 0.2s ease;
}
```

- [ ] **Step 4: Testar**

Expandir e fechar uma pasta com vários filhos. Verificar que outros cards na lista não travam/saltam. Verificar que cards esticam ao redimensionar a janela.

- [ ] **Step 5: Commit**

```bash
git add src/renderer/src/components/DownloadList.vue
git commit -m "fix: download cards stretch to window width and folder toggle no longer freezes UI"
```

---

## Task 4: Fix formatEta para tempos > 1 hora

**Files:**
- Modify: `src/renderer/src/components/DownloadList.vue` (script, função `formatEta`)

- [ ] **Step 1: Reescrever formatEta**

Localizar em `DownloadList.vue` (linha ~587):

```typescript
function formatEta(secs: number): string {
  if (!secs || secs <= 0) return '--'
  if (secs < 60) return `${Math.round(secs)}s`
  const m = Math.floor(secs / 60)
  const s = Math.round(secs % 60)
  return `${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`
}
```

Substituir por:

```typescript
function formatEta(secs: number): string {
  if (!secs || secs <= 0) return '--'
  if (secs < 60) return `${Math.round(secs)}s`
  if (secs < 3600) {
    const m = Math.floor(secs / 60)
    const s = Math.round(secs % 60)
    return `${m}m ${s}s`
  }
  const h = Math.floor(secs / 3600)
  const m = Math.floor((secs % 3600) / 60)
  return `${h}h ${m}m`
}
```

- [ ] **Step 2: Verificar resultado**

Com um arquivo de 10GB a 68 KB/s: `eta = (10 * 1024 * 1024 * 1024 - bytes) / 68000`. Para o início: `eta ≈ 158000s → 43h 53m`. Verificar manualmente: `Math.floor(158000 / 3600) = 43`, `Math.floor((158000 % 3600) / 60) = 53`. Resultado: `"43h 53m"` ✓

- [ ] **Step 3: Commit**

```bash
git add src/renderer/src/components/DownloadList.vue
git commit -m "fix: format ETA correctly for downloads longer than 1 hour"
```

---

## Task 5: Fix progresso total de pasta

**Files:**
- Modify: `src/renderer/src/components/DownloadList.vue` (script, handler de `download:progress`)
- Modify: `backend/src/routes/downloads.rs` (run_download, atualização do total do pai)

**Causa identificada:** O campo `d.size` no backend é preenchido no momento da criação com `file_info.size`. Para pastas, isso é a soma dos filhos. O progress event envia `bytes: update.bytes_downloaded` e `total: update.total_bytes`. Se `update.total_bytes` for 0 (o provider não enviou total), o frontend usa `items.value[idx].size` como fallback — mas se `size = 0` no item do frontend, o percent nunca atualiza.

- [ ] **Step 1: Garantir que `rustDownloadToItem` preserva `size` corretamente**

Em `src/preload/index.ts` (linha ~188), a função `rustDownloadToItem` já faz `size = (d.size as number) ?? 0`. Confirmar que para pastas, `d.size` vem do backend como a soma dos filhos. **Ler** a resposta de `GET /downloads` em runtime (via console.log temporário no renderer) para verificar.

- [ ] **Step 2: Garantir `size` no backend para pastas**

Em `backend/src/routes/downloads.rs`, o `Download` é criado com `size: file_info.size`. Para MediaFire folders, `file_info.size` é `total_size = children.iter().map(|c| c.size).sum()`. Verificar que isso chega no frontend. Adicionar temporariamente:

No handler de `download:progress` em `DownloadList.vue` (linha ~312), antes do cálculo de `total`:

```typescript
if (items.value[idx]?.isFolder) {
  console.log('[folder progress]', {
    evBytes: ev.bytes,
    evTotal: ev.total,
    itemSize: items.value[idx].size,
    itemPercent: items.value[idx].percent,
  })
}
```

Testar com um folder do MediaFire. Se `evTotal` é 0, o problema é no backend. Se `itemSize` é 0, o problema é na hidratação.

- [ ] **Step 3: Corrigir `size` no backend**

No `run_download` em `downloads.rs`, quando o download completa (`Ok(Ok(_bytes))`), o `d.bytes_downloaded = d.size` é atribuído. Mas durante o download, `d.size` pode ser 0 se o total não foi atualizado. Adicionar atualização do `size` no loop de progress:

```rust
// Dentro do bloco: let mut map = state.downloads.lock().await;
if let Some(d) = map.get_mut(&id) {
    d.bytes_downloaded = update.bytes_downloaded;
    d.speed_bps = speed;
    d.eta_secs = eta;
    // Atualizar size se ainda não foi definido (pode vir do ProgressUpdate)
    if update.total_bytes > 0 && d.size == 0 {
        d.size = update.total_bytes;
    }
    // ... resto do bloco de children
```

- [ ] **Step 4: Remover console.log temporário**

Após confirmar que funciona, remover o `console.log` adicionado no Step 2.

- [ ] **Step 5: Commit**

```bash
git add src/renderer/src/components/DownloadList.vue backend/src/routes/downloads.rs
git commit -m "fix: folder total progress now updates correctly during download"
```

---

## Task 6: Fix notificação ao concluir

**Files:**
- Modify: `src/renderer/src/App.vue` (onDownloadComplete, adicionar logging)
- Modify: `src/renderer/src/components/AppSettings.vue` (corrigir default de speedLimitKib)

**Causa provável:** Em modo dev no macOS, a `Notification` do Electron pode retornar `isSupported() = false` ou ser silenciada pelo sistema. O código de notificação está correto — o problema é de permissão/ambiente.

**Bug secundário identificado:** `AppSettings.vue` tem `speedLimitKib: 5000` como default no reactive, mas `defaultSettings` em `main/index.ts` tem `0`. Se as settings forem salvas antes do `onMounted` terminar de carregar, o limite incorreto de 5000 KiB/s é salvo.

- [ ] **Step 1: Corrigir default errado em AppSettings.vue**

Em `src/renderer/src/components/AppSettings.vue`, linha ~155, na declaração do `reactive<AppSettings>`:

```typescript
// ANTES
const settings = reactive<AppSettings>({
  outputDir: '~/Downloads',
  maxConcurrentDownloads: 3,
  maxRetriesPerDownload: 3,
  parallelPartsPerDownload: 4,
  speedLimitKib: 5000,  // ← BUG: deveria ser 0
  // ...
})

// DEPOIS
const settings = reactive<AppSettings>({
  outputDir: '~/Downloads',
  maxConcurrentDownloads: 3,
  maxRetriesPerDownload: 3,
  parallelPartsPerDownload: 4,
  speedLimitKib: 0,  // ← CORRIGIDO
  // ...
})
```

- [ ] **Step 2: Adicionar logging na cadeia de notificação**

Em `src/renderer/src/App.vue`, na função `onDownloadComplete`:

```typescript
async function onDownloadComplete(payload: DownloadCompletePayload): Promise<void> {
  const settings = await window.api.settings.load().catch(() => null)
  if (settings?.nativeNotification) {
    const title = payload.outputPath.split('/').pop() || payload.outputPath
    const shown = await window.api.system.notify('Download concluído', title).catch((e) => {
      console.warn('[notify] erro ao mostrar notificação:', e)
      return false
    })
    if (!shown) {
      console.warn('[notify] Notification.isSupported() retornou false ou notificação foi bloqueada')
    }
  } else {
    console.log('[notify] nativeNotification desabilitado nas settings')
  }
  // ... resto da função (history) permanece igual
```

- [ ] **Step 3: Testar em dev e produção**

3a. Em modo dev, completar um download pequeno e verificar nos console logs do Electron (`View → Toggle Developer Tools → Console`) o que acontece.

3b. Se `Notification.isSupported()` retorna false em dev: é esperado em alguns ambientes. Fazer build de produção (`npm run build`) e testar no app empacotado.

3c. Se retorna true mas não aparece: verificar `Preferências do Sistema → Notificações` no macOS e garantir que o Electron (ou o app empacotado) tem permissão.

- [ ] **Step 4: Commit**

```bash
git add src/renderer/src/App.vue src/renderer/src/components/AppSettings.vue
git commit -m "fix: notification logging for debugging + fix speedLimitKib default value"
```

---

## Task 7: Fix speed limiter global (MediaFire)

**Files:**
- Modify: `backend/src/providers/mod.rs` (apply_speed_limit + try_parallel_download)
- Modify: `backend/src/providers/mediafire.rs` (loop de chunk)

**Causa:** O `apply_speed_limit` atual controla a velocidade corretamente em teoria, mas para downloads paralelos (`try_parallel_download`), cada task usa o total global com uma única `started_at`. Se uma task baixa um chunk grande rapidamente, todas as tasks ficam dormindo por longos períodos, causando comportamento de "burst e pausa" em vez de throttle suave. Para downloads sequenciais, chunks grandes da rede também causam pausas longas.

**Fix:** Quebrar a escrita em sub-chunks de 64KB para granularidade fina de throttle.

- [ ] **Step 1: Refatorar apply_speed_limit para aceitar sub-chunks**

Em `backend/src/providers/mod.rs`, a função `apply_speed_limit` já está correta. A mudança é nos loops de escrita dos providers.

- [ ] **Step 2: Refatorar loop de download sequencial no MediaFire**

Em `backend/src/providers/mediafire.rs`, no bloco de download de arquivo único (linha ~456), substituir:

```rust
// ANTES
while let Some(chunk) = stream.next().await {
    let chunk = chunk?;
    file.write_all(&chunk).await?;
    let chunk_len = chunk.len() as u64;
    downloaded += chunk_len;
    session_downloaded += chunk_len;

    let _ = progress_tx
        .send(ProgressUpdate {
            bytes_downloaded: downloaded,
            total_bytes: total,
            child_filename: None,
            child_bytes_downloaded: None,
            child_total_bytes: None,
            child_speed_bps: None,
            child_eta_secs: None,
        })
        .await;
    apply_speed_limit(started_at, session_downloaded, speed_limit_bps).await;
}

// DEPOIS
while let Some(chunk) = stream.next().await {
    let chunk = chunk?;
    for piece in chunk.chunks(65_536) {
        file.write_all(piece).await?;
        let piece_len = piece.len() as u64;
        downloaded += piece_len;
        session_downloaded += piece_len;

        let _ = progress_tx
            .send(ProgressUpdate {
                bytes_downloaded: downloaded,
                total_bytes: total,
                child_filename: None,
                child_bytes_downloaded: None,
                child_total_bytes: None,
                child_speed_bps: None,
                child_eta_secs: None,
            })
            .await;
        apply_speed_limit(started_at, session_downloaded, speed_limit_bps).await;
    }
}
```

- [ ] **Step 3: Refatorar loop de download de pasta (MediaFire)**

No mesmo arquivo, no loop de arquivos da pasta (linha ~374), dentro do `while let Some(chunk) = stream.next().await`, fazer a mesma mudança:

```rust
// ANTES
while let Some(chunk) = stream.next().await {
    let chunk = chunk?;
    file_handle.write_all(&chunk).await?;
    let chunk_len = chunk.len() as u64;
    downloaded_total += chunk_len;
    session_downloaded += chunk_len;
    child_session_downloaded += chunk_len;
    // ... cálculo de speed/eta
    let _ = progress_tx.send(...).await;
    apply_speed_limit(started_at, session_downloaded, speed_limit_bps).await;
}

// DEPOIS
while let Some(chunk) = stream.next().await {
    let chunk = chunk?;
    for piece in chunk.chunks(65_536) {
        file_handle.write_all(piece).await?;
        let piece_len = piece.len() as u64;
        downloaded_total += piece_len;
        session_downloaded += piece_len;
        child_session_downloaded += piece_len;

        let child_elapsed = child_started_at.elapsed().as_secs_f64();
        let child_speed = if child_elapsed > 0.0 {
            (child_session_downloaded as f64 / child_elapsed) as u64
        } else {
            0
        };
        let child_downloaded = if resumed { existing_bytes } else { 0 } + child_session_downloaded;
        let child_eta = if child_speed > 0 && file_total > child_downloaded {
            (file_total - child_downloaded) / child_speed
        } else {
            0
        };

        let _ = progress_tx
            .send(ProgressUpdate {
                bytes_downloaded: downloaded_total,
                total_bytes: total_size,
                child_filename: Some(filename.clone()),
                child_bytes_downloaded: Some(child_downloaded),
                child_total_bytes: Some(file_total),
                child_speed_bps: Some(child_speed),
                child_eta_secs: Some(child_eta),
            })
            .await;
        apply_speed_limit(started_at, session_downloaded, speed_limit_bps).await;
    }
}
```

- [ ] **Step 4: Corrigir throttle em parallel download**

Em `backend/src/providers/mod.rs`, na função `try_parallel_download`, dentro da task spawned, fazer o mesmo wrap de 64KB:

```rust
// ANTES
while let Some(chunk) = stream.next().await {
    let chunk = chunk?;
    file.write_all(&chunk).await?;

    let downloaded = total_downloaded.fetch_add(chunk.len() as u64, Ordering::SeqCst)
        + chunk.len() as u64;
    let _ = progress_tx.send(ProgressUpdate { ... }).await;
    apply_speed_limit(started_at, total_downloaded.load(Ordering::SeqCst), speed_limit_bps).await;
}

// DEPOIS — também passar limit/part_count por task
// (antes do loop de tasks, `part_count` já está disponível)
// dentro de cada task spawned:
let task_limit = speed_limit_bps.map(|l| l / part_count as u64);
while let Some(chunk) = stream.next().await {
    let chunk = chunk?;
    for piece in chunk.chunks(65_536) {
        file.write_all(piece).await?;
        let piece_len = piece.len() as u64;
        let downloaded = total_downloaded.fetch_add(piece_len, Ordering::SeqCst) + piece_len;
        let _ = progress_tx
            .send(ProgressUpdate {
                bytes_downloaded: downloaded,
                total_bytes,
                child_filename: None,
                child_bytes_downloaded: None,
                child_total_bytes: None,
                child_speed_bps: None,
                child_eta_secs: None,
            })
            .await;
        // Usar task_limit: limit / num_tasks — cada task controla sua fração
        apply_speed_limit(started_at, total_downloaded.load(Ordering::SeqCst) / part_count as u64, task_limit).await;
    }
}
```

Nota: `part_count` precisa ser capturado pela closure. Ele já existe no escopo antes do `for part_index in 0..part_count`.

- [ ] **Step 5: Compilar backend**

```bash
cd backend && cargo build 2>&1
```

Esperado: compilação sem erros. Se houver erro de borrow checker ou variável não capturada, ajustar a closure para capturar `part_count` via `let part_count = part_count;` antes do `tokio::spawn`.

- [ ] **Step 6: Testar com link MediaFire**

Definir `speedLimitKib = 500` (500 KB/s) nas settings. Adicionar um arquivo do MediaFire. Observar no monitor de rede (Activity Monitor → Network ou equivalente) que a velocidade se mantém próxima de 500 KB/s.

- [ ] **Step 7: Commit**

```bash
git add backend/src/providers/mod.rs backend/src/providers/mediafire.rs
git commit -m "fix: speed limiter now applies smoothly with 64KB chunk granularity"
```

---

## Task 8: Widget de velocidade global com sparkline

**Files:**
- Create: `src/renderer/src/components/SpeedWidget.vue`
- Modify: `src/renderer/src/components/DownloadList.vue` (emit 'global-speed')
- Modify: `src/renderer/src/App.vue` (buffer, topbar, SpeedWidget import)

- [ ] **Step 1: Criar SpeedWidget.vue**

Criar `src/renderer/src/components/SpeedWidget.vue`:

```vue
<template>
  <div class="speed-widget">
    <svg class="sparkline" :viewBox="`0 0 ${WIDTH} ${HEIGHT}`" preserveAspectRatio="none">
      <polyline
        v-if="points.length > 1"
        :points="points"
        fill="none"
        :stroke="lineColor"
        stroke-width="1.5"
        stroke-linecap="round"
        stroke-linejoin="round"
        vector-effect="non-scaling-stroke"
      />
      <polyline
        v-if="points.length > 1"
        :points="fillPoints"
        fill="url(#sparkGrad)"
        stroke="none"
        vector-effect="non-scaling-stroke"
      />
      <defs>
        <linearGradient id="sparkGrad" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" :stop-color="lineColor" stop-opacity="0.3" />
          <stop offset="100%" :stop-color="lineColor" stop-opacity="0" />
        </linearGradient>
      </defs>
    </svg>
    <div class="speed-labels">
      <span class="speed-down">↓ {{ formattedSpeed }}</span>
      <span class="speed-up">↑ —</span>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'

const WIDTH = 120
const HEIGHT = 36

const props = defineProps<{
  speedHistory: number[]
  currentSpeed: number
  lineColor?: string
}>()

const lineColor = computed(() => props.lineColor ?? 'var(--accent-color)')

const points = computed(() => {
  const data = props.speedHistory
  if (data.length < 2) return ''
  const max = Math.max(...data, 1)
  return data
    .map((v, i) => {
      const x = (i / (data.length - 1)) * WIDTH
      const y = HEIGHT - (v / max) * HEIGHT * 0.85
      return `${x.toFixed(1)},${y.toFixed(1)}`
    })
    .join(' ')
})

const fillPoints = computed(() => {
  const data = props.speedHistory
  if (data.length < 2) return ''
  const max = Math.max(...data, 1)
  const pts = data.map((v, i) => {
    const x = (i / (data.length - 1)) * WIDTH
    const y = HEIGHT - (v / max) * HEIGHT * 0.85
    return `${x.toFixed(1)},${y.toFixed(1)}`
  })
  pts.push(`${WIDTH},${HEIGHT}`)
  pts.unshift(`0,${HEIGHT}`)
  return pts.join(' ')
})

const formattedSpeed = computed(() => {
  const bps = props.currentSpeed
  if (!bps || bps <= 0) return '0 KB/s'
  if (bps < 1024 * 1024) return `${(bps / 1024).toFixed(0)} KB/s`
  return `${(bps / (1024 * 1024)).toFixed(1)} MB/s`
})
</script>

<style scoped>
.speed-widget {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-shrink: 0;
}

.sparkline {
  width: 120px;
  height: 36px;
  border-radius: 4px;
  overflow: visible;
}

.speed-labels {
  display: flex;
  flex-direction: column;
  gap: 2px;
  font-size: 11px;
  line-height: 1.2;
}

.speed-down {
  color: var(--accent-color);
  font-weight: 600;
  font-variant-numeric: tabular-nums;
}

.speed-up {
  color: var(--text-muted);
  font-variant-numeric: tabular-nums;
}
</style>
```

- [ ] **Step 2: Emitir velocidade global no DownloadList**

Em `src/renderer/src/components/DownloadList.vue`, adicionar o emit:

```typescript
// No defineEmits, adicionar:
const emit = defineEmits<{
  (e: 'count-change', count: number): void
  (e: 'download-complete', payload: { id: string; outputPath: string }): void
  (e: 'global-speed', bps: number): void  // ← ADICIONAR
}>()
```

No handler de `download:progress` (dentro do `unsubs.push(...)`), ao final do bloco de atualização do item, após `items.value[idx] = { ... }`, adicionar:

```typescript
// Somar speed de todos os itens ativos
const totalSpeed = items.value
  .filter((i) => i.status === 'downloading')
  .reduce((sum, i) => sum + (i.speedBps ?? 0), 0)
emit('global-speed', totalSpeed)
```

- [ ] **Step 3: Buffer de histórico e SpeedWidget no App.vue**

Em `src/renderer/src/App.vue`, no `<script setup>`:

**3a. Atualizar o import do vue** — adicionar `onUnmounted` à linha existente:
```typescript
// ANTES
import { onMounted, ref } from 'vue'
// DEPOIS
import { onMounted, onUnmounted, ref } from 'vue'
```

**3b. Adicionar import do SpeedWidget** após os imports de componentes existentes:
```typescript
import SpeedWidget from './components/SpeedWidget.vue'
```

**3c. Adicionar state** (após as declarações de refs existentes):
```typescript
const speedHistory = ref<number[]>(new Array(60).fill(0))
const currentSpeed = ref(0)
let speedTicker: ReturnType<typeof setInterval> | null = null

function onGlobalSpeed(bps: number): void {
  currentSpeed.value = bps
}

onUnmounted(() => {
  if (speedTicker) clearInterval(speedTicker)
})
```

**3d. No `onMounted` EXISTENTE**, adicionar ao final do bloco (dentro do mesmo `onMounted`):
```typescript
onMounted(async () => {
  // ... código existente de initTheme e settings ...

  // ADICIONAR ao final:
  speedTicker = setInterval(() => {
    speedHistory.value = [...speedHistory.value.slice(1), currentSpeed.value]
  }, 1000)
})
```

- [ ] **Step 4: Adicionar SpeedWidget no template do App.vue**

No `<template>` de `App.vue`, localizar a `.topbar` e adicionar o widget à direita:

```html
<header class="topbar">
  <div class="brand">
    <!-- ... conteúdo existente ... -->
  </div>
  <SpeedWidget
    :speed-history="speedHistory"
    :current-speed="currentSpeed"
  />
</header>
```

E no `<DownloadList>`, adicionar o listener:

```html
<DownloadList
  @count-change="downloadCount = $event"
  @download-complete="onDownloadComplete"
  @global-speed="onGlobalSpeed"
/>
```

E ajustar o CSS da `.topbar` para acomodar o widget:

```css
.topbar {
  display: flex;
  align-items: center;
  justify-content: space-between;  /* já existe */
  padding: 14px 18px;
  border-bottom: 1px solid var(--border-color);
  background: var(--bg-secondary);
}
```

- [ ] **Step 5: Testar**

Iniciar um download. Verificar que o widget aparece no topbar e que a velocidade muda em tempo real. Deixar o download rodar 30s e verificar que o gráfico desenha o histórico.

- [ ] **Step 6: Commit**

```bash
git add src/renderer/src/components/SpeedWidget.vue src/renderer/src/components/DownloadList.vue src/renderer/src/App.vue
git commit -m "feat: add global speed widget with sparkline graph to topbar"
```

---

## Task 9: Converter ícones para SVG (Phosphor) + novas categorias

**Files:**
- Modify: `src/renderer/src/assets/file-icons.ts`
- Create/Replace: `src/renderer/src/assets/file-icons/*.svg` (17 arquivos)
- Modify: `src/renderer/src/assets/provider-icons.ts`

### Sub-task 9a: Baixar SVGs do Phosphor

- [ ] **Step 1: Baixar os 17 SVGs do Phosphor Icons**

```bash
cd /Users/lucasreolon/Desktop/Código/gDownloader/src/renderer/src/assets/file-icons

BASE="https://raw.githubusercontent.com/phosphor-icons/core/main/assets/regular"

curl -sL "$BASE/folder.svg" -o folder.svg
curl -sL "$BASE/file.svg" -o file.svg
curl -sL "$BASE/film-strip.svg" -o video.svg
curl -sL "$BASE/music-note.svg" -o audio.svg
curl -sL "$BASE/file-zip.svg" -o archive.svg
curl -sL "$BASE/image.svg" -o image.svg
curl -sL "$BASE/file-pdf.svg" -o pdf.svg
curl -sL "$BASE/file-doc.svg" -o doc.svg
curl -sL "$BASE/table.svg" -o sheet.svg
curl -sL "$BASE/presentation.svg" -o slides.svg
curl -sL "$BASE/file-text.svg" -o text.svg
curl -sL "$BASE/code.svg" -o code.svg
curl -sL "$BASE/hard-drives.svg" -o disk.svg
curl -sL "$BASE/app-window.svg" -o app.svg
curl -sL "$BASE/database.svg" -o database.svg
curl -sL "$BASE/text-aa.svg" -o font.svg
curl -sL "$BASE/subtitles.svg" -o subtitle.svg
```

- [ ] **Step 2: Verificar downloads**

```bash
ls -la /Users/lucasreolon/Desktop/Código/gDownloader/src/renderer/src/assets/file-icons/*.svg
```

Esperado: 17 arquivos `.svg`, cada um com tamanho > 0. Se algum falhar (tamanho 0 ou conteúdo de erro), verificar o nome correto no repositório Phosphor:
- `subtitles.svg` pode estar em `closed-captioning.svg`
- `file-pdf.svg` pode estar em `file.svg` com variante específica

Se algum ícone não existir, substituir por um próximo adequado:
- `subtitles` → `closed-captioning`
- `file-doc` → `file-doc` (verificar existência)
- `hard-drives` → `hard-drive`
- `app-window` → `window`

- [ ] **Step 3: Deletar os PNGs antigos**

```bash
rm /Users/lucasreolon/Desktop/Código/gDownloader/src/renderer/src/assets/file-icons/*.png
```

### Sub-task 9b: Reescrever file-icons.ts

- [ ] **Step 4: Reescrever file-icons.ts**

Substituir o conteúdo completo de `src/renderer/src/assets/file-icons.ts`:

```typescript
import folderSvg from './file-icons/folder.svg?raw'
import fileSvg from './file-icons/file.svg?raw'
import videoSvg from './file-icons/video.svg?raw'
import archiveSvg from './file-icons/archive.svg?raw'
import audioSvg from './file-icons/audio.svg?raw'
import imageSvg from './file-icons/image.svg?raw'
import pdfSvg from './file-icons/pdf.svg?raw'
import docSvg from './file-icons/doc.svg?raw'
import sheetSvg from './file-icons/sheet.svg?raw'
import slidesSvg from './file-icons/slides.svg?raw'
import textSvg from './file-icons/text.svg?raw'
import codeSvg from './file-icons/code.svg?raw'
import diskSvg from './file-icons/disk.svg?raw'
import appSvg from './file-icons/app.svg?raw'
import databaseSvg from './file-icons/database.svg?raw'
import fontSvg from './file-icons/font.svg?raw'
import subtitleSvg from './file-icons/subtitle.svg?raw'

export interface FileIconDef {
  svg: string
  alt: string
  kind: string
}

const ICONS = {
  folder:   { svg: folderSvg,   alt: 'Folder',       kind: 'folder' },
  file:     { svg: fileSvg,     alt: 'File',          kind: 'file' },
  video:    { svg: videoSvg,    alt: 'Video',         kind: 'video' },
  archive:  { svg: archiveSvg,  alt: 'Archive',       kind: 'archive' },
  audio:    { svg: audioSvg,    alt: 'Audio',         kind: 'audio' },
  image:    { svg: imageSvg,    alt: 'Image',         kind: 'image' },
  pdf:      { svg: pdfSvg,      alt: 'PDF',           kind: 'pdf' },
  doc:      { svg: docSvg,      alt: 'Document',      kind: 'doc' },
  sheet:    { svg: sheetSvg,    alt: 'Spreadsheet',   kind: 'sheet' },
  slides:   { svg: slidesSvg,   alt: 'Presentation',  kind: 'slides' },
  text:     { svg: textSvg,     alt: 'Text',          kind: 'text' },
  code:     { svg: codeSvg,     alt: 'Code',          kind: 'code' },
  disk:     { svg: diskSvg,     alt: 'Disk image',    kind: 'disk' },
  app:      { svg: appSvg,      alt: 'Application',   kind: 'app' },
  database: { svg: databaseSvg, alt: 'Database',      kind: 'database' },
  font:     { svg: fontSvg,     alt: 'Font',          kind: 'font' },
  subtitle: { svg: subtitleSvg, alt: 'Subtitle',      kind: 'subtitle' },
} satisfies Record<string, FileIconDef>

const EXT_MAP: Record<string, keyof typeof ICONS> = {
  // Video
  mkv: 'video', mp4: 'video', avi: 'video', mov: 'video', wmv: 'video',
  m4v: 'video', flv: 'video', webm: 'video', mts: 'video', m2ts: 'video',
  m2t: 'video', mpg: 'video', mpeg: 'video', vob: 'video', ogv: 'video',
  '3gp': 'video', asf: 'video', rm: 'video', rmvb: 'video',
  // Audio
  mp3: 'audio', flac: 'audio', aac: 'audio', ogg: 'audio', wav: 'audio',
  opus: 'audio', m4a: 'audio', wma: 'audio', aiff: 'audio', alac: 'audio',
  mid: 'audio', midi: 'audio', amr: 'audio',
  // Archives
  zip: 'archive', rar: 'archive', '7z': 'archive', tar: 'archive',
  gz: 'archive', tgz: 'archive', bz2: 'archive', xz: 'archive',
  lz: 'archive', zst: 'archive', cab: 'archive',
  // Disk images
  iso: 'disk', img: 'disk', dmg: 'disk', vhd: 'disk', vhdx: 'disk',
  vmdk: 'disk', qcow2: 'disk', toast: 'disk',
  // Images
  jpg: 'image', jpeg: 'image', png: 'image', gif: 'image', webp: 'image',
  svg: 'image', bmp: 'image', tif: 'image', tiff: 'image', heic: 'image',
  heif: 'image', avif: 'image', raw: 'image', psd: 'image', ai: 'image',
  eps: 'image', ico: 'image', icns: 'image',
  // Documents
  pdf: 'pdf',
  doc: 'doc', docx: 'doc', odt: 'doc', rtf: 'doc', pages: 'doc',
  epub: 'doc', mobi: 'doc', azw3: 'doc',
  xls: 'sheet', xlsx: 'sheet', csv: 'sheet', ods: 'sheet', numbers: 'sheet', tsv: 'sheet',
  ppt: 'slides', pptx: 'slides', odp: 'slides', key: 'slides',
  txt: 'text', md: 'text', markdown: 'text', log: 'text', nfo: 'text',
  ini: 'text', cfg: 'text', conf: 'text', yaml: 'text', yml: 'text', toml: 'text',
  // Subtitles
  srt: 'subtitle', vtt: 'subtitle', ass: 'subtitle', ssa: 'subtitle',
  // Fonts
  otf: 'font', ttf: 'font', woff: 'font', woff2: 'font',
  // Code / data
  json: 'code', xml: 'code',
  js: 'code', mjs: 'code', cjs: 'code', ts: 'code', jsx: 'code', tsx: 'code',
  rs: 'code', py: 'code', rb: 'code', php: 'code', go: 'code', java: 'code',
  kt: 'code', swift: 'code', c: 'code', cc: 'code', cpp: 'code', h: 'code',
  hpp: 'code', cs: 'code', sh: 'code', bash: 'code', zsh: 'code', fish: 'code',
  ps1: 'code', bat: 'code', cmd: 'code', html: 'code', css: 'code', scss: 'code',
  less: 'code', vue: 'code', svelte: 'code', lock: 'code', env: 'code',
  // Database
  sql: 'database', sqlite: 'database', db: 'database', db3: 'database', sqlite3: 'database',
  // Executables
  exe: 'app', msi: 'app', apk: 'app', ipa: 'app', deb: 'app', rpm: 'app',
  pkg: 'app', appimage: 'app', jar: 'app', bin: 'app', dmgpart: 'app',
  // Misc (fallback to file)
  cer: 'file', crt: 'file', pem: 'file', p12: 'file', pfx: 'file',
}

const MIME_EXACT_MAP: Record<string, keyof typeof ICONS> = {
  'application/pdf': 'pdf',
  'application/zip': 'archive',
  'application/x-rar-compressed': 'archive',
  'application/vnd.rar': 'archive',
  'application/x-7z-compressed': 'archive',
  'application/x-tar': 'archive',
  'application/gzip': 'archive',
  'application/x-bzip2': 'archive',
  'application/x-xz': 'archive',
  'application/json': 'code',
  'application/xml': 'code',
  'application/sql': 'database',
  'application/x-sqlite3': 'database',
  'application/vnd.sqlite3': 'database',
  'application/vnd.ms-excel': 'sheet',
  'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet': 'sheet',
  'application/msword': 'doc',
  'application/vnd.openxmlformats-officedocument.wordprocessingml.document': 'doc',
  'application/vnd.ms-powerpoint': 'slides',
  'application/vnd.openxmlformats-officedocument.presentationml.presentation': 'slides',
  'application/x-iso9660-image': 'disk',
  'application/x-apple-diskimage': 'disk',
  'application/x-msdownload': 'app',
  'application/vnd.android.package-archive': 'app',
  'font/otf': 'font',
  'font/ttf': 'font',
  'font/woff': 'font',
  'font/woff2': 'font',
}

const MIME_PREFIX_MAP: Array<[string, keyof typeof ICONS]> = [
  ['video/', 'video'],
  ['audio/', 'audio'],
  ['image/', 'image'],
  ['text/', 'text'],
  ['font/', 'font'],
  ['application/vnd.ms-', 'doc'],
  ['application/vnd.openxmlformats-officedocument.wordprocessingml', 'doc'],
  ['application/vnd.openxmlformats-officedocument.spreadsheetml', 'sheet'],
  ['application/vnd.openxmlformats-officedocument.presentationml', 'slides'],
  ['application/x-sharedlib', 'app'],
  ['application/x-executable', 'app'],
]

export function getFileIcon(filename: string, mimeType?: string, isFolder = false): FileIconDef {
  if (isFolder) return ICONS.folder

  const normalizedMime = mimeType?.split(';')[0].trim().toLowerCase()
  if (normalizedMime && MIME_EXACT_MAP[normalizedMime]) {
    return ICONS[MIME_EXACT_MAP[normalizedMime]]
  }
  if (normalizedMime) {
    const match = MIME_PREFIX_MAP.find(([prefix]) => normalizedMime.startsWith(prefix))
    if (match) return ICONS[match[1]]
  }

  const cleanName = filename.split('?')[0].split('#')[0]
  const ext = cleanName.includes('.') ? cleanName.split('.').pop()?.toLowerCase() ?? '' : ''
  return ICONS[EXT_MAP[ext] ?? 'file']
}
```

- [ ] **Step 5: Commit parcial (somente assets + file-icons.ts)**

```bash
git add src/renderer/src/assets/file-icons/ src/renderer/src/assets/file-icons.ts
git commit -m "feat: replace PNG file icons with Phosphor SVGs, add font and subtitle categories"
```

---

## Task 10: Atualizar componentes para usar SVGs (v-html)

**Files:**
- Modify: `src/renderer/src/components/DownloadList.vue` (img → span v-html)
- Modify: `src/renderer/src/components/LinkGrabber.vue` (img → span v-html)

**Nota:** A interface `FileIconDef` mudou: `src: string` foi substituído por `svg: string`. Qualquer uso de `.src` precisa ser substituído por `.svg`.

- [ ] **Step 1: Atualizar DownloadList.vue — ícone de arquivo do item principal**

Localizar (linha ~47):

```html
<img
  class="type-icon"
  :src="getFileIcon(item.title || item.url, undefined, item.isFolder).src"
  :alt="getFileIcon(item.title || item.url, undefined, item.isFolder).alt"
  draggable="false"
/>
```

Substituir por:

```html
<span
  class="type-icon"
  v-html="getFileIcon(item.title || item.url, undefined, item.isFolder).svg"
  :aria-label="getFileIcon(item.title || item.url, undefined, item.isFolder).alt"
></span>
```

- [ ] **Step 2: Atualizar DownloadList.vue — ícone dos filhos de pasta**

Localizar (linha ~213):

```html
<img
  class="child-icon"
  :src="getFileIcon(child.filename, child.mimeType, child.isFolder).src"
  :alt="getFileIcon(child.filename, child.mimeType, child.isFolder).alt"
  draggable="false"
/>
```

Substituir por:

```html
<span
  class="child-icon"
  v-html="getFileIcon(child.filename, child.mimeType, child.isFolder).svg"
  :aria-label="getFileIcon(child.filename, child.mimeType, child.isFolder).alt"
></span>
```

- [ ] **Step 3: Atualizar CSS dos ícones no DownloadList.vue**

No `<style scoped>`, substituir:

```css
/* ANTES */
.type-icon {
  width: 18px;
  height: 18px;
  flex-shrink: 0;
  object-fit: contain;
}

.child-icon {
  width: 16px;
  height: 16px;
  flex-shrink: 0;
  object-fit: contain;
}

/* DEPOIS */
.type-icon {
  width: 18px;
  height: 18px;
  flex-shrink: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  color: var(--text-muted);
}

.type-icon :deep(svg) {
  width: 18px;
  height: 18px;
}

.child-icon {
  width: 16px;
  height: 16px;
  flex-shrink: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  color: var(--text-muted);
}

.child-icon :deep(svg) {
  width: 16px;
  height: 16px;
}
```

- [ ] **Step 4: Atualizar LinkGrabber.vue**

Localizar em `LinkGrabber.vue` (linha ~81):

```html
<img
  class="row-icon"
  :src="getFileIcon(row.info?.name ?? row.displayName, row.info?.mimeType, row.info?.isFolder).src"
  :alt="getFileIcon(row.info?.name ?? row.displayName, row.info?.mimeType, row.info?.isFolder).alt"
  draggable="false"
/>
```

Substituir por:

```html
<span
  class="row-icon"
  v-html="getFileIcon(row.info?.name ?? row.displayName, row.info?.mimeType, row.info?.isFolder).svg"
  :aria-label="getFileIcon(row.info?.name ?? row.displayName, row.info?.mimeType, row.info?.isFolder).alt"
></span>
```

E atualizar o CSS de `.row-icon` no LinkGrabber.vue para usar inline-flex + `:deep(svg)` igual ao padrão acima (tamanho de acordo com o design atual da linha).

- [ ] **Step 5: Verificar TypeScript — não deve ter erros de `.src`**

```bash
cd /Users/lucasreolon/Desktop/Código/gDownloader
npx tsc --noEmit 2>&1 | grep -i "src\|svg\|FileIconDef"
```

Esperado: sem erros relacionados a `src` na interface `FileIconDef`.

- [ ] **Step 6: Verificar build**

```bash
npm run build 2>&1 | tail -20
```

Esperado: build sem erros.

- [ ] **Step 7: Commit**

```bash
git add src/renderer/src/components/DownloadList.vue src/renderer/src/components/LinkGrabber.vue
git commit -m "feat: use SVG icons via v-html in download list and link grabber"
```

---

## Task 11: Converter pixeldrain para SVG + limpeza final

**Files:**
- Modify: `src/renderer/src/assets/provider-icons.ts`
- Delete: `src/renderer/src/assets/provider-icons/pixeldrain.png`
- Modify: `README.md`

- [ ] **Step 1: Criar SVG inline para PixelDrain**

O logo do PixelDrain é uma forma simples (gota + letra P). Criar SVG inline baseado na identidade visual da marca (laranja #ff7b00):

Em `src/renderer/src/assets/provider-icons.ts`, localizar a entrada `pixeldrain` e substituir por SVG inline. Primeiro verificar o conteúdo atual do arquivo:

```typescript
// Localizar a linha com pixeldrain e substituir por:
pixeldrain: {
  color: '#ff7b00',
  svg: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 36 36" fill="none">
    <rect width="36" height="36" rx="8" fill="#ff7b00"/>
    <path d="M18 8 C18 8, 10 17, 10 22 C10 26.4 13.6 30 18 30 C22.4 30 26 26.4 26 22 C26 17 18 8 18 8Z" fill="white"/>
    <text x="18" y="24" text-anchor="middle" font-size="10" font-weight="bold" fill="#ff7b00" font-family="sans-serif">P</text>
  </svg>`,
},
```

- [ ] **Step 2: Remover import do PNG do pixeldrain**

Em `provider-icons.ts`, remover a linha de import:

```typescript
// REMOVER:
import pixeldrainPng from './provider-icons/pixeldrain.png'
```

E remover qualquer referência a `pixeldrainPng` no arquivo.

- [ ] **Step 3: Deletar o PNG do pixeldrain**

```bash
rm /Users/lucasreolon/Desktop/Código/gDownloader/src/renderer/src/assets/provider-icons/pixeldrain.png
```

- [ ] **Step 4: Limpar README**

Abrir `README.md` e remover a seção "Destaques Visuais" que contém:
- Referência a `gui-bus/TechIcons: https://github.com/gui-bus/TechIcons`
- Referência a `Flaticon Cloud Storage Logo Pack: https://www.flaticon.com/packs/cloud-storage-logo`
- O texto "Ícones de arquivo e pasta em PNG no renderer."
- O texto "Base preparada para uso de PNG/SVG dedicados por provedor."

Manter o restante do README intacto.

- [ ] **Step 5: Testar build final**

```bash
npm run build 2>&1 | tail -30
```

Esperado: sem erros.

- [ ] **Step 6: Verificar visualmente**

Abrir o app em dev (`npm run dev`), navegar pelas abas Downloads e Link Grabber. Verificar:
- Ícones de arquivo aparecem em SVG (devem ser monocromáticos/outline, estilo Phosphor)
- Ícone do PixelDrain aparece no lugar do PNG
- Ícones de provider (Mega, MediaFire, GDrive) continuam iguais

- [ ] **Step 7: Commit final**

```bash
git add src/renderer/src/assets/provider-icons.ts README.md
git commit -m "feat: convert pixeldrain icon to SVG, remove external icon credits from README"
```

---

---

## Task 12: Converter pixeldrain para SVG inline

**Files:**
- Modify: `src/renderer/src/assets/provider-icons.ts`
- Delete: `src/renderer/src/assets/provider-icons/pixeldrain.png`

- [ ] **Step 1: Substituir pixeldrain no provider-icons.ts**

Em `src/renderer/src/assets/provider-icons.ts`:
- Remover `import pixeldrainPng from './provider-icons/pixeldrain.png'`
- Substituir a entrada `pixeldrain` por SVG inline:

```typescript
pixeldrain: {
  color: '#ff7b00',
  svg: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 36 36" fill="none">
    <rect width="36" height="36" rx="8" fill="#ff7b00"/>
    <path d="M18 8 C18 8, 10 17, 10 22 C10 26.4 13.6 30 18 30 C22.4 30 26 26.4 26 22 C26 17 18 8 18 8Z" fill="white"/>
    <text x="18" y="24" text-anchor="middle" font-size="10" font-weight="bold" fill="#ff7b00" font-family="sans-serif">P</text>
  </svg>`,
},
```

- [ ] **Step 2: Deletar o PNG**

```bash
rm /Users/lucasreolon/Desktop/Código/gDownloader/src/renderer/src/assets/provider-icons/pixeldrain.png
```

- [ ] **Step 3: Commit**

```bash
git add src/renderer/src/assets/provider-icons.ts
git rm src/renderer/src/assets/provider-icons/pixeldrain.png
git commit -m "feat: convert pixeldrain icon to inline SVG, remove PNG"
```

---

## Task 13: Fix ícone do anonfiles (borda branca excessiva)

**Files:**
- Modify or Create: `src/renderer/src/assets/provider-icons/anonfiles.svg`
- Modify: `src/renderer/src/assets/provider-icons.ts`

**Contexto:** O ícone do provider anonfiles tem borda branca excessiva e precisa ser cortado para ficar aproximadamente quadrado, sem padding desnecessário.

- [ ] **Step 1: Localizar o arquivo**

Verificar se `src/renderer/src/assets/provider-icons/anonfiles.svg` existe:

```bash
ls -la /Users/lucasreolon/Desktop/Código/gDownloader/src/renderer/src/assets/provider-icons/
```

Se não existir, criar um SVG geométrico simples para anonfiles (marca em laranja-amarelo `#ffcc00` com fundo escuro `#1a1a2e`):

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 36 36">
  <rect width="36" height="36" rx="8" fill="#1a1a2e"/>
  <text x="18" y="24" text-anchor="middle" font-size="18" font-weight="bold" fill="#ffcc00" font-family="sans-serif">A</text>
</svg>
```

- [ ] **Step 2: Se o arquivo existe — ajustar o viewBox**

Abrir o SVG e verificar o atributo `viewBox`. Se há padding branco, calcular as dimensões reais do conteúdo e ajustar o viewBox para eliminar o excesso. O viewBox deve ser `"0 0 W H"` onde W e H são iguais (quadrado) ou próximos.

Usar essa técnica para SVGs com padding: identificar o `<rect>` ou `<g>` mais externo e ajustar viewBox para `"xMin yMin width height"` onde os valores excluem o whitespace.

- [ ] **Step 3: Registrar em provider-icons.ts**

Adicionar/atualizar em `src/renderer/src/assets/provider-icons.ts`:

```typescript
import anonfilesSvg from './provider-icons/anonfiles.svg?raw'

// Na constante ICONS:
anonfiles: {
  color: '#ffcc00',
  svg: anonfilesSvg,
},
```

- [ ] **Step 4: Commit**

```bash
git add src/renderer/src/assets/provider-icons/anonfiles.svg src/renderer/src/assets/provider-icons.ts
git commit -m "feat: add/fix anonfiles provider icon, remove white border"
```

---

## Task 14: Skeleton loading para downloads lentos

**Files:**
- Modify: `src/renderer/src/components/DownloadList.vue`
- Modify: `src/renderer/src/App.vue`
- Modify: `src/renderer/src/components/LinkGrabber.vue`

**Contexto:** Quando o usuário adiciona uma URL, o backend precisa chamar `get_file_info()` antes de criar o item de download. Isso pode levar alguns segundos. Atualmente a lista parece vazia até o item aparecer. O fix é mostrar N cards skeleton enquanto a requisição está em andamento.

**Fluxo:**
```
User clica "Add" no LinkGrabber (N URLs)
  → LinkGrabber emite 'adding-urls' com count=N
  → App.vue passa skeletonCount=N para DownloadList
  → DownloadList renderiza N skeleton cards no topo
  → Após hydrate() completar, skeleton cards desaparecem
```

- [ ] **Step 1: Adicionar emit 'adding-urls' no LinkGrabber**

Em `src/renderer/src/components/LinkGrabber.vue`, no `defineEmits`:

```typescript
const emit = defineEmits<{
  // ...existentes...
  (e: 'adding-urls', count: number): void
}>()
```

Antes de chamar `api.downloads.add(...)` (no loop de envio de URLs), emitir o count:

```typescript
emit('adding-urls', urlsToAdd.length)
```

- [ ] **Step 2: Receber e passar skeletonCount no App.vue**

Em `src/renderer/src/App.vue`:

```typescript
const skeletonCount = ref(0)

function onAddingUrls(count: number): void {
  skeletonCount.value = count
}

// No template, passar para DownloadList:
// <DownloadList :skeleton-count="skeletonCount" @skeleton-done="skeletonCount = 0" ... />
// <LinkGrabber @adding-urls="onAddingUrls" ... />
```

- [ ] **Step 3: Renderizar skeleton cards no DownloadList**

Em `src/renderer/src/components/DownloadList.vue`:

Adicionar prop:
```typescript
const props = defineProps<{
  skeletonCount?: number
}>()
```

No template, antes da lista principal, adicionar:

```html
<div
  v-for="i in (props.skeletonCount ?? 0)"
  :key="`skeleton-${i}`"
  class="download-card skeleton-card"
>
  <div class="skeleton-line skeleton-title"></div>
  <div class="skeleton-line skeleton-progress"></div>
  <div class="skeleton-line skeleton-meta"></div>
</div>
```

CSS dos skeletons (no `<style scoped>`):

```css
@keyframes shimmer {
  0%   { background-position: -200% 0; }
  100% { background-position:  200% 0; }
}

.skeleton-card {
  pointer-events: none;
  padding: 14px;
  gap: 8px;
  flex-direction: column;
}

.skeleton-line {
  border-radius: 6px;
  background: linear-gradient(
    90deg,
    var(--bg-card) 25%,
    color-mix(in srgb, var(--bg-card) 70%, var(--text-muted)) 50%,
    var(--bg-card) 75%
  );
  background-size: 200% 100%;
  animation: shimmer 1.4s infinite;
}

.skeleton-title   { height: 14px; width: 55%; }
.skeleton-progress{ height: 8px;  width: 100%; margin-top: 6px; }
.skeleton-meta    { height: 10px; width: 35%; margin-top: 4px; }
```

Quando `hydrate()` completa com sucesso, emitir `'skeleton-done'`:

```typescript
// No finally do hydrate(), após a lista ser preenchida:
emit('skeleton-done')
```

- [ ] **Step 4: Commit**

```bash
git add src/renderer/src/components/DownloadList.vue src/renderer/src/components/LinkGrabber.vue src/renderer/src/App.vue
git commit -m "feat: show skeleton cards while download is being resolved by backend"
```

---

## Task 15: README pt-BR com badges e keywords

**Files:**
- Modify: `README.md`

**Objetivo:** Reescrever o README em pt-BR com badges de tecnologia, keywords para SEO/discoverabilidade, estrutura clara e conteúdo que apareça bem em buscas (GitHub Search, Google).

**Estrutura do novo README:**

```markdown
# gDownloader

[Badges: plataformas, linguagens, licença]

> Gerenciador de downloads open-source com interface Electron/Vue 3 e backend em Rust/Axum. 
> Suporta Mega, MediaFire, Google Drive e PixelDrain.

**Keywords:** download manager, electron, rust, axum, vue3, mega downloader, mediafire downloader, 
google drive downloader, pixeldrain, desktop app, file downloader, gerenciador de downloads

## Funcionalidades
[lista com bullet points]

## Providers suportados
[tabela com ícones, status, observações]

## Tech Stack
[tabela ou badges: Electron, Vue 3, Rust, Axum, TypeScript, Tokio]

## Requisitos
[Node.js, Rust toolchain, plataformas]

## Instalação (desenvolvimento)
[passos com code blocks]

## Como usar
[screenshots ou GIF + passos básicos]

## Configurações
[lista das opções disponíveis]

## Roadmap
[providers planejados]

## Contribuindo
[link para issues, como rodar testes]

## Licença
[MIT ou outra]
```

**Badges a incluir (usando shields.io):**
- Electron version
- Rust (stable)
- Vue 3
- Plataformas: macOS, Linux, Windows
- Licença

**Nota:** Não inventar informações — basear tudo no que existe no código. Para licença, verificar se há `LICENSE` file ou `package.json → license`.

- [ ] **Step 1: Verificar informações do projeto**

```bash
cat /Users/lucasreolon/Desktop/Código/gDownloader/package.json | grep -E '"name|"version|"license|"description"'
ls /Users/lucasreolon/Desktop/Código/gDownloader/LICENSE* 2>/dev/null || echo "sem licença"
```

- [ ] **Step 2: Escrever o novo README.md**

Reescrever completamente, em pt-BR, com as seções listadas acima.

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs: rewrite README in pt-BR with badges, tech stack, and SEO keywords"
```

---

## Ordem de execução recomendada

Execute as tasks nesta ordem — cada uma é independente:

1. Task 1 (scroll) — CSS puro, sem risco
2. Task 4 (formatEta) — uma função, sem dependências
3. Task 2 (blinking) — lógica de estado
4. Task 3 (stretch + folder toggle) — CSS + template
5. Task 5 (folder progress) — requer teste com download real
6. Task 6 (notificação) — requer teste manual
7. Task 7 (speed limiter) — requer compilação Rust
8. Task 8 (SpeedWidget) — novo componente
9. Tasks 9–11 (ícones — SUPERSEDIDAS: usuário adotou file-icon-vectors)
10. Task 12 (pixeldrain SVG) — simples
11. Task 13 (anonfiles icon) — requer verificar se o arquivo existe
12. Task 14 (skeleton loading) — Vue + CSS
13. Task 15 (README) — documentação

---

## Notas para o executor

- **Phosphor icon names:** Se um nome de arquivo baixado resultar em HTML de erro (404), verificar o nome correto em `https://github.com/phosphor-icons/core/tree/main/assets/regular`. Os nomes são em kebab-case.
- **`?raw` imports:** Vite suporta isso nativamente. Se o bundler reclamar, verificar se `vite.config.ts` tem `assetsInclude: ['**/*.svg']` — mas normalmente não é necessário para `?raw`.
- **TypeScript após mudança de FileIconDef:** Qualquer arquivo que usava `.src` em `FileIconDef` vai falhar. Grep por `.src` para encontrar usos restantes: `grep -rn "\.src" src/renderer/src --include="*.vue" --include="*.ts"`.
- **Backend compilation:** Para Task 7, sempre checar warnings do Rust além de erros — variáveis não usadas podem indicar código morto.
