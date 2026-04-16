<template>
  <div class="download-list">
    <!-- Empty state -->
    <div v-if="items.length === 0" class="empty-state">
      <div class="empty-icon">
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48" fill="none" width="48" height="48">
          <circle cx="24" cy="24" r="22" stroke="currentColor" stroke-width="1.5" opacity="0.3"/>
          <path d="M24 14 L24 30 M18 25 L24 32 L30 25" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
          <path d="M16 36 H32" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
        </svg>
      </div>
      <p class="empty-title">Nenhum download ativo</p>
      <p class="empty-sub">Use o Capturador de Links para começar</p>
    </div>

    <!-- Download items -->
    <div v-else class="items-container">
      <div class="list-toolbar">
        <span class="list-count">{{ items.length }} item(ns) na sessão</span>
        <button
          class="toolbar-btn"
          :disabled="finishedCount === 0"
          @click="clearFinished"
        >
          Limpar concluídos
        </button>
      </div>
      <transition-group name="item" tag="div" class="items-stack">
        <div
          v-for="item in orderedItems"
          :key="item.id"
          class="download-card"
          :class="`status-bg-${item.status}`"
        >
          <!-- Left: provider icon -->
          <div
            class="provider-icon"
            v-html="getIcon(item.moduleId).svg"
            :title="moduleLabel(item.moduleId)"
          ></div>

          <!-- Center: info -->
          <div class="item-body">
            <!-- Row 1: filename + status + actions -->
            <div class="item-header">
              <div class="item-title-wrap">
                <img
                  class="type-icon"
                  :src="getFileIcon(item.title || item.url, undefined, item.isFolder).src"
                  :alt="getFileIcon(item.title || item.url, undefined, item.isFolder).alt"
                  draggable="false"
                />
                <span class="item-title" :title="item.title">{{ item.title || item.url }}</span>
              </div>
              <div class="item-actions">
                <span class="status-badge" :class="`badge-${item.status}`">
                  <span class="badge-dot" :class="`dot-${item.status}`"></span>
                  {{ statusText(item.status) }}
                </span>
                <button
                  class="action-btn"
                  :title="item.isFolder ? 'Copiar URLs' : 'Copiar URL'"
                  @click="copyUrl(item)"
                >
                  <i class="pi pi-copy"></i>
                </button>
                <button
                  v-if="item.isFolder && (item.children?.length ?? 0) > 0"
                  class="action-btn"
                  :title="isExpanded(item.id) ? 'Ocultar itens' : 'Mostrar itens'"
                  @click="toggleFolder(item.id)"
                >
                  <i class="pi" :class="isExpanded(item.id) ? 'pi-chevron-up' : 'pi-chevron-down'"></i>
                </button>
                <button
                  v-if="item.status === 'pending' || item.status === 'downloading'"
                  class="action-btn"
                  title="Pausar"
                  @click="pause(item.id)"
                >
                  <i class="pi pi-pause"></i>
                </button>
                <button
                  v-if="item.status === 'paused'"
                  class="action-btn"
                  title="Retomar"
                  @click="resume(item.id)"
                >
                  <i class="pi pi-play"></i>
                </button>
                <button
                  v-if="item.status === 'pending' || item.status === 'downloading' || item.status === 'paused'"
                  class="cancel-btn"
                  title="Cancelar"
                  @click="cancel(item.id)"
                >
                  <i class="pi pi-times"></i>
                </button>
                <button
                  v-if="item.status === 'paused' || item.status === 'error' || item.status === 'cancelled'"
                  class="action-btn"
                  title="Tentar novamente"
                  @click="retry(item.id)"
                >
                  <i class="pi pi-refresh"></i>
                </button>
                <button
                  v-if="item.status === 'paused' || item.status === 'error' || item.status === 'cancelled' || item.status === 'complete'"
                  class="action-btn"
                  title="Reiniciar"
                  @click="restart(item.id)"
                >
                  <i class="pi pi-replay"></i>
                </button>
                <button
                  v-if="item.status === 'complete' && item.outputPath && isExtractableArchive(item.outputPath)"
                  class="action-btn"
                  title="Extrair"
                  @click="extract(item.outputPath!)"
                >
                  <i class="pi pi-folder-plus"></i>
                </button>
                <button
                  v-if="item.status === 'complete' && item.outputPath"
                  class="open-btn"
                  title="Mostrar na pasta"
                  @click="openFolder(item.outputPath!)"
                >
                  <i class="pi pi-folder-open"></i>
                </button>
                <button
                  v-if="isTerminal(item.status)"
                  class="action-btn"
                  title="Remover"
                  @click="remove(item.id)"
                >
                  <i class="pi pi-trash"></i>
                </button>
              </div>
            </div>

            <!-- Row 2: progress bar -->
            <div class="progress-track">
              <div
                class="progress-fill"
                :class="{ 'progress-shimmer': item.status === 'downloading' }"
                :style="{
                  width: item.percent + '%',
                  background: getProgressColor(item)
                }"
              ></div>
            </div>

            <!-- Row 3: meta info -->
            <div class="item-meta">
              <span class="meta-percent">{{ item.percent }}%</span>

              <template v-if="item.status === 'downloading'">
                <span class="meta-sep">·</span>
                <span class="meta-speed">{{ formatSpeed(item.speedBps) }}</span>
                <span class="meta-sep">·</span>
                <span class="meta-eta">{{ formatEta(item.etaSec) }} restante</span>
              </template>

              <template v-if="item.size > 0">
                <span class="meta-sep">·</span>
                <span class="meta-size">
                  {{ formatBytes(Math.floor((item.percent / 100) * item.size)) }}
                  / {{ formatBytes(item.size) }}
                </span>
              </template>

              <template v-if="item.isFolder && (item.children?.length ?? 0) > 0">
                <span class="meta-sep">·</span>
                <span class="meta-size">{{ item.children?.length }} item(ns)</span>
              </template>

              <template v-if="item.status === 'error' && item.error">
                <span class="meta-sep">·</span>
                <span class="meta-error" :title="item.error">{{ item.error }}</span>
              </template>

              <template v-if="(item.maxRetries ?? 0) > 0">
                <span class="meta-sep">·</span>
                <span class="meta-retries">
                  tentativa {{ (item.retryCount ?? 0) + 1 }}/{{ (item.maxRetries ?? 0) + 1 }}
                </span>
              </template>
            </div>

            <!-- Row 4: output path (clickable) -->
            <div
              v-if="item.outputPath"
              class="item-path"
              :title="item.outputPath"
              @click="openFolder(item.outputPath!)"
            >
              <i class="pi pi-folder" style="font-size: 10px;"></i>
              {{ item.outputPath }}
            </div>

            <div
              v-if="item.isFolder && isExpanded(item.id) && (item.children?.length ?? 0) > 0"
              class="folder-children"
            >
              <div
                v-for="child in item.children"
                :key="`${item.id}:${child.filename}`"
                class="child-row"
              >
                <div class="child-main">
                  <div class="child-name">
                    <img
                      class="child-icon"
                      :src="getFileIcon(child.filename, child.mimeType, child.isFolder).src"
                      :alt="getFileIcon(child.filename, child.mimeType, child.isFolder).alt"
                      draggable="false"
                    />
                    <span>{{ child.filename }}</span>
                  </div>
                  <div class="child-meta">
                    <span class="child-status">{{ childStatusText(child.status) }}</span>
                    <span class="meta-sep">·</span>
                    <span>{{ childPercent(child) }}%</span>
                    <template v-if="(child.speedBps ?? 0) > 0">
                      <span class="meta-sep">·</span>
                      <span class="meta-speed">{{ formatSpeed(child.speedBps ?? 0) }}</span>
                    </template>
                    <template v-if="(child.etaSec ?? 0) > 0">
                      <span class="meta-sep">·</span>
                      <span>{{ formatEta(child.etaSec ?? 0) }}</span>
                    </template>
                  </div>
                  <div class="child-track">
                    <div
                      class="child-fill"
                      :style="{ width: `${childPercent(child)}%` }"
                    ></div>
                  </div>
                </div>
                <span class="child-size">{{ formatBytes(child.size) }}</span>
              </div>
            </div>
          </div>
        </div>
      </transition-group>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { DownloadStatus as DownloadStatusEnum } from '../../../shared/constants'
import type { DownloadChild, DownloadItem } from '../../../shared/types'
import { getFileIcon } from '../assets/file-icons'
import { getProviderIcon, getProviderColor } from '../assets/provider-icons'

interface ModuleSummary {
  id: string
  name: string
  color: string
}

// ── Emits ──────────────────────────────────────────────────
const emit = defineEmits<{
  (e: 'count-change', count: number): void
  (e: 'download-complete', payload: { id: string; outputPath: string }): void
}>()

// ── State ──────────────────────────────────────────────────
const items = ref<DownloadItem[]>([])
const modulesById = ref<Record<string, ModuleSummary>>({})
const expandedFolders = ref<Record<string, boolean>>({})
const unsubs: Array<() => void> = []

// ── Computed ───────────────────────────────────────────────
const orderedItems = computed(() => [...items.value].sort((a, b) => b.addedAt - a.addedAt))
const finishedCount = computed(() =>
  items.value.filter((item) => isTerminal(item.status)).length
)

// ── Lifecycle ──────────────────────────────────────────────
onMounted(async () => {
  // Load module metadata for labels
  const modules = await window.api.modules.list().catch(() => [])
  modulesById.value = modules.reduce<Record<string, ModuleSummary>>((acc, mod) => {
    acc[mod.id] = { id: mod.id, name: mod.name, color: mod.color }
    return acc
  }, {})

  // Load existing downloads from backend
  await hydrate()

  // Real-time progress events
  unsubs.push(
    window.api.downloads.on('download:progress', (event: unknown) => {
      const ev = event as {
        type?: string
        id: string
        bytes?: number
        total?: number
        speed?: number
        eta?: number
        status?: string
        child_filename?: string
        child_bytes?: number
        child_total?: number
        child_speed?: number
        child_eta?: number
      }
      if (!ev?.id) return
      const idx = items.value.findIndex((i) => i.id === ev.id)
      if (idx >= 0) {
        const total = ev.total ?? items.value[idx].size
        const bytes = ev.bytes ?? 0
        let nextChildren = items.value[idx].children
        if (ev.child_filename && nextChildren?.length) {
          nextChildren = nextChildren.map((child) => {
            if (child.filename !== ev.child_filename) {
              return child.status === DownloadStatusEnum.Downloading
                ? { ...child, speedBps: 0, etaSec: 0 }
                : child
            }

            const childTotal = ev.child_total ?? child.size ?? 0
            const childBytes = ev.child_bytes ?? child.bytesDownloaded ?? 0
            const childStatus =
              childTotal > 0 && childBytes >= childTotal
                ? DownloadStatusEnum.Complete
                : DownloadStatusEnum.Downloading

            return {
              ...child,
              bytesDownloaded: childBytes,
              speedBps: ev.child_speed ?? child.speedBps ?? 0,
              etaSec: ev.child_eta ?? child.etaSec ?? 0,
              status: childStatus
            }
          })
        }
        items.value[idx] = {
          ...items.value[idx],
          percent: total > 0 ? Math.min(100, Math.floor((bytes / total) * 100)) : items.value[idx].percent,
          speedBps: ev.speed ?? 0,
          etaSec: ev.eta ?? 0,
          status: (ev.status as DownloadItem['status']) ?? items.value[idx].status,
          size: total > 0 ? total : items.value[idx].size,
          children: nextChildren
        }
      } else {
        // Unknown item — refresh list
        void hydrate()
      }
    })
  )

  unsubs.push(
    window.api.downloads.on('download:complete', (event: unknown) => {
      const ev = event as { id: string; path?: string; outputPath?: string }
      if (!ev?.id) return
      const idx = items.value.findIndex((i) => i.id === ev.id)
      const outputPath = ev.path ?? ev.outputPath ?? ''
      if (idx >= 0) {
        items.value[idx] = {
          ...items.value[idx],
          status: DownloadStatusEnum.Complete,
          percent: 100,
          outputPath
        }
        emit('download-complete', { id: ev.id, outputPath })
      }
    })
  )

  unsubs.push(
    window.api.downloads.on('download:error', (event: unknown) => {
      const ev = event as { id: string; message?: string; error?: string }
      if (!ev?.id) return
      const idx = items.value.findIndex((i) => i.id === ev.id)
      if (idx >= 0) {
        items.value[idx] = {
          ...items.value[idx],
          status: DownloadStatusEnum.Error,
          error: ev.message ?? ev.error ?? 'Erro desconhecido'
        }
      }
    })
  )

  // Also subscribe to old-style events for compatibility
  unsubs.push(
    window.api.downloads.on('download:status', (event: unknown) => {
      const ev = event as { id: string; status: DownloadItem['status'] }
      if (!ev?.id) return
      upsertById(ev.id, { status: ev.status })
    })
  )

  unsubs.push(
    window.api.downloads.on('download:cancelled', (event: unknown) => {
      const ev = event as { id: string }
      if (!ev?.id) return
      upsertById(ev.id, { status: DownloadStatusEnum.Cancelled })
    })
  )
})

onUnmounted(() => {
  for (const unsub of unsubs) unsub()
})

// ── Data methods ───────────────────────────────────────────
async function hydrate(): Promise<void> {
  const fresh: DownloadItem[] = await window.api.downloads.list().catch(() => [])
  const freshById = new Map<string, DownloadItem>(fresh.map((item) => [item.id, item] as const))

  // Remove items no longer present (reverse to avoid index shift)
  for (let i = items.value.length - 1; i >= 0; i--) {
    if (!freshById.has(items.value[i].id)) {
      items.value.splice(i, 1)
    }
  }

  // Update existing in-place or push new — avoids full-array replace that triggers flicker
  for (const freshItem of fresh) {
    const idx = items.value.findIndex((i) => i.id === freshItem.id)
    if (idx >= 0) {
      Object.assign(items.value[idx], freshItem)
    } else {
      items.value.push(freshItem)
    }
  }

  emit('count-change', items.value.length)
}

function upsertById(id: string, patch: Partial<DownloadItem>): void {
  const idx = items.value.findIndex((item) => item.id === id)
  if (idx === -1) {
    void hydrate()
    return
  }
  items.value[idx] = { ...items.value[idx], ...patch }
}

// ── Actions ────────────────────────────────────────────────
async function cancel(id: string): Promise<void> {
  await window.api.downloads.cancel(id).catch(() => null)
  await hydrate()
}

async function pause(id: string): Promise<void> {
  await window.api.downloads.pause(id).catch(() => null)
  await hydrate()
}

async function resume(id: string): Promise<void> {
  await window.api.downloads.resume(id).catch(() => null)
  await hydrate()
}

async function retry(id: string): Promise<void> {
  await window.api.downloads.retry(id).catch(() => null)
  await hydrate()
}

async function restart(id: string): Promise<void> {
  await window.api.downloads.restart(id).catch(() => null)
  await hydrate()
}

async function remove(id: string): Promise<void> {
  await window.api.downloads.remove(id).catch(() => null)
  await hydrate()
}

async function clearFinished(): Promise<void> {
  await window.api.downloads.clearFinished().catch(() => null)
  await hydrate()
}

async function copyUrl(item: DownloadItem): Promise<void> {
  const folderUrls = (item.children ?? [])
    .map((child: DownloadChild) => child.sourceUrl?.trim() ?? '')
    .filter((url) => url.length > 0)

  const payload = item.isFolder && folderUrls.length > 0
    ? folderUrls.join('\n')
    : item.url

  await window.api.clipboard.writeText(payload).catch(() => null)
}

async function extract(filePath: string): Promise<void> {
  try {
    const extractedPath = await window.api.archive.extract(filePath)
    await window.api.system.notify('Extração concluída', extractedPath).catch(() => null)
    await window.api.openPath(extractedPath).catch(() => null)
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error)
    await window.api.system.notify('Falha na extração', message).catch(() => null)
  }
}

function toggleFolder(id: string): void {
  expandedFolders.value = {
    ...expandedFolders.value,
    [id]: !expandedFolders.value[id]
  }
}

function isExpanded(id: string): boolean {
  return !!expandedFolders.value[id]
}

function openFolder(filePath: string): void {
  window.api.showInFolder(filePath)
}

function isTerminal(status: DownloadItem['status']): boolean {
  return status === DownloadStatusEnum.Complete
    || status === DownloadStatusEnum.Error
    || status === DownloadStatusEnum.Cancelled
}

function isExtractableArchive(filePath: string): boolean {
  const lower = filePath.toLowerCase()
  return (
    lower.endsWith('.zip') ||
    lower.endsWith('.rar') ||
    lower.endsWith('.7z') ||
    lower.endsWith('.tar') ||
    lower.endsWith('.tar.gz') ||
    lower.endsWith('.tgz') ||
    lower.endsWith('.tar.bz2') ||
    lower.endsWith('.tbz2') ||
    lower.endsWith('.tar.xz') ||
    lower.endsWith('.txz') ||
    lower.endsWith('.tar.zst')
  )
}

// ── Display helpers ────────────────────────────────────────
function moduleLabel(moduleId: string): string {
  return modulesById.value[moduleId]?.name ?? moduleId
}

function getIcon(moduleId: string) {
  return getProviderIcon(moduleId)
}

function getProgressColor(item: DownloadItem): string {
  if (item.status === 'error') return '#ef4444'
  if (item.status === 'complete') return 'linear-gradient(90deg, #22c55e, #4ade80)'
  if (item.status === 'cancelled') return '#666'
  // Use provider color for active downloads
  const color = modulesById.value[item.moduleId]?.color ?? getProviderColor(item.moduleId)
  return `linear-gradient(90deg, ${color}, ${color}cc)`
}

function statusText(status: DownloadItem['status']): string {
  const map: Record<string, string> = {
    pending: 'Na fila',
    downloading: 'Baixando',
    complete: 'Concluído',
    error: 'Erro',
    cancelled: 'Cancelado',
    paused: 'Pausado'
  }
  return map[status] ?? status
}

function formatBytes(n: number): string {
  if (!n || n < 0) return '0 B'
  if (n < 1024) return `${n} B`
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`
  if (n < 1024 * 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} MB`
  return `${(n / (1024 * 1024 * 1024)).toFixed(2)} GB`
}

function formatSpeed(bps: number): string {
  if (!bps || bps <= 0) return '0 KB/s'
  if (bps < 1024 * 1024) return `${(bps / 1024).toFixed(0)} KB/s`
  return `${(bps / (1024 * 1024)).toFixed(1)} MB/s`
}

function formatEta(secs: number): string {
  if (!secs || secs <= 0) return '--'
  if (secs < 60) return `${Math.round(secs)}s`
  const m = Math.floor(secs / 60)
  const s = Math.round(secs % 60)
  return `${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`
}

function childPercent(child: DownloadChild): number {
  if (!child.size || child.size <= 0) return 0
  const bytes = child.bytesDownloaded ?? 0
  return Math.max(0, Math.min(100, Math.floor((bytes / child.size) * 100)))
}

function childStatusText(status?: DownloadChild['status']): string {
  if (!status) return 'Na fila'
  return statusText(status)
}
</script>

<style scoped>
/* ── Container ──────────────────────────────────────────────── */
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

/* ── Empty state ────────────────────────────────────────────── */
.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  flex: 1;
  min-height: 100%;
  gap: 10px;
  color: var(--text-muted);
  text-align: center;
}

.empty-icon {
  opacity: 0.4;
  margin-bottom: 8px;
}

.empty-title {
  font-size: 15px;
  font-weight: 600;
  margin: 0;
  color: var(--text-muted);
}

.empty-sub {
  font-size: 13px;
  margin: 0;
  color: var(--text-muted);
  opacity: 0.7;
}

/* ── Items list ─────────────────────────────────────────────── */
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

.list-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.list-count {
  font-size: 12px;
  color: var(--text-muted);
}

.toolbar-btn {
  border: 1px solid var(--border-color);
  background: var(--bg-card);
  color: var(--text-primary);
  border-radius: 10px;
  padding: 8px 12px;
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
}

.toolbar-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.items-stack {
  display: flex;
  flex-direction: column;
  gap: 10px;
  width: 100%;
  min-width: 0;
  align-self: stretch;
}

/* ── Download card ──────────────────────────────────────────── */
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
}

.download-card::before {
  content: '';
  position: absolute;
  left: 0;
  top: 0;
  bottom: 0;
  width: 3px;
  border-radius: 12px 0 0 12px;
  background: var(--status-indicator, var(--border-color));
}

.download-card.status-bg-downloading::before {
  background: var(--accent-gradient);
  animation: pulse-glow 2s ease-in-out infinite;
}

.download-card.status-bg-complete::before {
  background: linear-gradient(180deg, #22c55e, #4ade80);
}

.download-card.status-bg-error::before {
  background: #ef4444;
}

.download-card.status-bg-cancelled::before {
  background: #555;
}

.download-card:hover {
  border-color: color-mix(in srgb, var(--accent-color) 40%, var(--border-color));
  box-shadow: var(--shadow-card);
}

/* ── Provider icon ──────────────────────────────────────────── */
.provider-icon {
  width: 36px;
  height: 36px;
  flex-shrink: 0;
  border-radius: 8px;
  overflow: hidden;
  display: flex;
  align-items: center;
  justify-content: center;
  margin-top: 2px;
}

.provider-icon :deep(svg) {
  width: 36px;
  height: 36px;
}

/* ── Item body ──────────────────────────────────────────────── */
.item-body {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 7px;
}

/* ── Header row ─────────────────────────────────────────────── */
.item-header {
  display: flex;
  align-items: center;
  gap: 8px;
}

.item-title-wrap {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 8px;
}

.item-title {
  font-size: 13px;
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--text-primary);
}

.type-icon {
  width: 18px;
  height: 18px;
  flex-shrink: 0;
  object-fit: contain;
}

.item-actions {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
}

/* ── Status badge ───────────────────────────────────────────── */
.status-badge {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 3px 8px;
  border-radius: 999px;
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.3px;
  text-transform: uppercase;
}

.badge-pending {
  background: rgba(136, 136, 170, 0.15);
  color: var(--status-pending);
}

.badge-downloading {
  background: rgba(124, 111, 255, 0.15);
  color: var(--status-downloading);
}

.badge-complete {
  background: rgba(34, 197, 94, 0.15);
  color: var(--status-complete);
}

.badge-error {
  background: rgba(239, 68, 68, 0.15);
  color: var(--status-error);
}

.badge-cancelled {
  background: rgba(100, 100, 120, 0.15);
  color: var(--status-cancelled);
}

.badge-paused {
  background: rgba(250, 204, 21, 0.15);
  color: var(--status-paused);
}

.badge-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  flex-shrink: 0;
}

.dot-pending { background: var(--status-pending); }
.dot-downloading {
  background: var(--status-downloading);
  animation: pulse-glow 1.2s ease-in-out infinite;
}
.dot-complete { background: var(--status-complete); }
.dot-error { background: var(--status-error); }
.dot-cancelled { background: var(--status-cancelled); }
.dot-paused { background: var(--status-paused); }

/* ── Action buttons ─────────────────────────────────────────── */
.cancel-btn,
.open-btn,
.action-btn {
  width: 26px;
  height: 26px;
  border-radius: 6px;
  border: 1px solid var(--border-color);
  background: transparent;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  font-size: 11px;
  transition: all 0.15s ease;
}

.action-btn {
  color: var(--text-muted);
}

.action-btn:hover {
  background: rgba(99, 102, 241, 0.15);
  border-color: #6366f1;
  color: #6366f1;
}

.cancel-btn {
  color: var(--text-muted);
}

.cancel-btn:hover {
  background: rgba(239, 68, 68, 0.15);
  border-color: #ef4444;
  color: #ef4444;
}

.open-btn {
  color: var(--text-muted);
}

.open-btn:hover {
  background: rgba(34, 197, 94, 0.15);
  border-color: #22c55e;
  color: #22c55e;
}

/* ── Progress track ─────────────────────────────────────────── */
.progress-track {
  height: 6px;
  background: var(--surface-section);
  border-radius: 999px;
  overflow: hidden;
}

.progress-fill {
  height: 100%;
  border-radius: 999px;
  transition: width 0.3s ease;
  min-width: 2px;
}

.progress-shimmer {
  background-size: 200% 100% !important;
  animation: shimmer 1.8s ease-in-out infinite;
}

@keyframes shimmer {
  0% { background-position: 200% 0; }
  100% { background-position: -200% 0; }
}

/* ── Meta info ──────────────────────────────────────────────── */
.item-meta {
  display: flex;
  align-items: center;
  gap: 4px;
  flex-wrap: wrap;
  font-size: 11px;
  color: var(--text-muted);
}

.meta-percent {
  font-weight: 700;
  color: var(--text-primary);
  min-width: 32px;
}

.meta-sep {
  opacity: 0.4;
  padding: 0 2px;
}

.meta-speed {
  color: var(--accent-color);
  font-weight: 600;
}

.meta-eta {
  color: var(--text-muted);
}

.meta-size {
  color: var(--text-muted);
  font-family: 'Courier New', monospace;
  font-size: 10.5px;
}

.meta-error {
  color: #fca5a5;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 200px;
}

.meta-retries {
  color: var(--text-muted);
  font-size: 10.5px;
}

/* ── Output path ────────────────────────────────────────────── */
.item-path {
  display: flex;
  align-items: center;
  gap: 5px;
  font-size: 10.5px;
  color: var(--text-muted);
  font-family: 'Courier New', monospace;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  cursor: pointer;
  transition: color 0.15s;
  padding: 2px 0;
}

.item-path:hover {
  color: var(--accent-color);
}

.folder-children {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-top: 4px;
  padding: 8px 10px;
  border: 1px solid color-mix(in srgb, var(--border-color) 70%, transparent);
  border-radius: 10px;
  background: color-mix(in srgb, var(--bg-card) 70%, transparent);
}

.child-row {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
  font-size: 12px;
  color: var(--text-muted);
}

.child-main {
  min-width: 0;
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.child-name {
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 8px;
}

.child-name span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.child-meta {
  display: flex;
  align-items: center;
  gap: 4px;
  flex-wrap: wrap;
  font-size: 10.5px;
  color: var(--text-muted);
}

.child-status {
  font-weight: 600;
  color: var(--text-primary);
}

.child-track {
  height: 4px;
  background: var(--surface-section);
  border-radius: 999px;
  overflow: hidden;
}

.child-fill {
  height: 100%;
  min-width: 2px;
  border-radius: 999px;
  background: linear-gradient(90deg, var(--accent-color), color-mix(in srgb, var(--accent-color) 60%, #ffffff));
}

.child-icon {
  width: 16px;
  height: 16px;
  flex-shrink: 0;
  object-fit: contain;
}

.child-size {
  flex-shrink: 0;
  color: var(--text-muted);
}

/* ── List transitions ───────────────────────────────────────── */
.item-enter-active {
  animation: slide-in-up 0.2s ease;
}

.item-leave-active {
  animation: slide-in-up 0.15s ease reverse;
}

.item-move {
  transition: transform 0.2s ease;
  will-change: transform;
}

@keyframes slide-in-up {
  from {
    opacity: 0;
    transform: translateY(6px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

@keyframes pulse-glow {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.5; }
}
</style>
