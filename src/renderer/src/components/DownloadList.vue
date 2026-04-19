<template>
  <div class="download-list">
    <!-- Empty state -->
    <div v-if="items.length === 0 && (skeletonCount ?? 0) === 0" class="empty-state">
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
    <div v-if="items.length > 0 || (skeletonCount ?? 0) > 0" class="items-container">
      <div
        v-for="i in (skeletonCount ?? 0)"
        :key="`skeleton-${i}`"
        class="download-card skeleton-card"
      >
        <div class="skeleton-line skeleton-title"></div>
        <div class="skeleton-line skeleton-progress"></div>
        <div class="skeleton-line skeleton-meta"></div>
      </div>

      <div v-if="items.length > 0" class="list-toolbar">
        <span class="list-count">{{ items.length }} item(ns) na sessão</span>
        <button
          class="toolbar-btn"
          :disabled="finishedCount === 0"
          title="Remover downloads encerrados da lista"
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
                <span
                  class="type-icon"
                  :class="getFileIcon(item.title || item.url, undefined, item.isFolder).className"
                  :aria-label="getFileIcon(item.title || item.url, undefined, item.isFolder).alt"
                  role="img"
                ></span>
                <span class="item-title" :title="item.title">{{ item.title || item.url }}</span>
              </div>
              <div class="item-actions">
                <span class="status-badge" :class="`badge-${item.status}`">
                  <span class="badge-dot" :class="`dot-${item.status}`"></span>
                  {{ statusText(item) }}
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

              <template v-else-if="isWaitingRetry(item)">
                <span class="meta-sep">·</span>
                <span class="meta-wait">
                  <i class="pi pi-clock"></i>
                  {{ formatEta(retryCountdown(item)) }} para tentar novamente
                </span>
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

              <template v-if="isWaitingRetry(item) && item.error">
                <span class="meta-sep">·</span>
                <span class="meta-wait-reason" :title="item.error">{{ item.error }}</span>
              </template>

              <template v-else-if="item.status === 'error' && item.error">
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
              v-show="item.isFolder && isExpanded(item.id) && (item.children?.length ?? 0) > 0"
              class="folder-children"
            >
              <div
                v-for="node in childNodes(item.children)"
                :key="`${item.id}:${node.key}`"
                class="child-row"
                :class="{ 'is-folder-row': node.isFolder }"
              >
                <div class="child-main">
                  <div class="child-name" :style="{ paddingInlineStart: `${node.depth * 18}px` }">
                    <span
                      class="child-icon"
                      :class="getFileIcon(node.name, node.mimeType, node.isFolder).className"
                      :aria-label="getFileIcon(node.name, node.mimeType, node.isFolder).alt"
                      role="img"
                    ></span>
                    <span class="child-name-label">{{ node.name }}</span>
                    <span v-if="node.isFolder" class="child-folder-badge">{{ node.fileCount }} item(ns)</span>
                  </div>
                  <div class="child-meta">
                    <span class="child-status">{{ childStatusText(node.status) }}</span>
                    <span class="meta-sep">·</span>
                    <span>{{ childPercent(node) }}%</span>
                    <template v-if="(node.speedBps ?? 0) > 0">
                      <span class="meta-sep">·</span>
                      <span class="meta-speed">{{ formatSpeed(node.speedBps ?? 0) }}</span>
                    </template>
                    <template v-if="(node.etaSec ?? 0) > 0">
                      <span class="meta-sep">·</span>
                      <span>{{ formatEta(node.etaSec ?? 0) }}</span>
                    </template>
                    <template v-if="node.isFolder">
                      <span class="meta-sep">·</span>
                      <span>{{ node.fileCount }} arquivo(s)</span>
                    </template>
                  </div>
                  <div class="child-track">
                    <div
                      class="child-fill"
                      :style="{ width: `${childPercent(node)}%` }"
                    ></div>
                  </div>
                </div>
                <span class="child-size">{{ formatBytes(node.size) }}</span>
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
import { buildChildTree, flattenChildTree, type DerivedChildNode } from '../utils/child-tree'

interface ModuleSummary {
  id: string
  name: string
  color: string
}

// ── Props ──────────────────────────────────────────────────
const props = withDefaults(defineProps<{ skeletonCount?: number }>(), {
  skeletonCount: 0
})
const skeletonCount = computed(() => props.skeletonCount)

// ── Emits ──────────────────────────────────────────────────
const emit = defineEmits<{
  (e: 'count-change', count: number): void
  (e: 'download-complete', payload: { id: string; outputPath: string }): void
  (e: 'global-speed', bps: number): void
  (e: 'skeleton-done'): void
}>()

// ── State ──────────────────────────────────────────────────
const items = ref<DownloadItem[]>([])
const modulesById = ref<Record<string, ModuleSummary>>({})
const expandedFolders = ref<Record<string, boolean>>({})
const unsubs: Array<() => void> = []
// Mutex: at most one hydrate() runs at a time; hydrateQueued ensures one follow-up
// run executes after the in-flight one finishes.
let hydrateQueued = false
let hydrateInFlight = false
let isMounted = false
let lastSpeedEmit = 0
const nowTick = ref(Date.now())
let retryTimer: number | null = null
let hydrateTimer: number | null = null

// ── Computed ───────────────────────────────────────────────
const orderedItems = computed(() => [...items.value].sort((a, b) => b.addedAt - a.addedAt))
const finishedCount = computed(() =>
  items.value.filter((item) => isTerminal(item.status)).length
)

// ── Lifecycle ──────────────────────────────────────────────
onMounted(async () => {
  isMounted = true
  retryTimer = window.setInterval(() => {
    nowTick.value = Date.now()
  }, 1000)
  hydrateTimer = window.setInterval(() => {
    if (!isMounted) return
    void hydrate()
  }, 1500)
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
        child_path?: string
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
            const matches = ev.child_path
              ? child.path === ev.child_path
              : child.filename === ev.child_filename

            if (!matches) {
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

        const isFolder = items.value[idx].isFolder && (nextChildren?.length ?? 0) > 0
        const aggregatedChildBytes = isFolder
          ? nextChildren!.reduce((sum, child) => sum + (child.bytesDownloaded ?? 0), 0)
          : bytes
        const aggregatedChildSpeed = isFolder
          ? nextChildren!.reduce((sum, child) => sum + (child.speedBps ?? 0), 0)
          : (ev.speed ?? 0)
        const aggregatedPercent = total > 0
          ? Math.min(100, Math.floor((aggregatedChildBytes / total) * 100))
          : items.value[idx].percent
        const aggregatedEta = aggregatedChildSpeed > 0 && total > aggregatedChildBytes
          ? Math.floor((total - aggregatedChildBytes) / aggregatedChildSpeed)
          : 0

        items.value[idx] = {
          ...items.value[idx],
          percent: aggregatedPercent,
          speedBps: aggregatedChildSpeed,
          etaSec: isFolder ? aggregatedEta : (ev.eta ?? 0),
          status: (ev.status as DownloadItem['status']) ?? items.value[idx].status,
          size: total > 0 ? total : items.value[idx].size,
          // Keep parent bytes implicit in percent/size, but base folder progress on the sum of children.
          children: nextChildren
        }
        // Somar speed de todos os itens ativos (throttled a 200ms para não sobrecarregar)
        const now = Date.now()
        if (now - lastSpeedEmit >= 120) {
          lastSpeedEmit = now
          const totalSpeed = items.value
            .filter((i) => i.status === 'downloading')
            .reduce((sum, i) => sum + (i.speedBps ?? 0), 0)
          emit('global-speed', totalSpeed)
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
          speedBps: 0,
          etaSec: 0,
          outputPath
        }
        emit('download-complete', { id: ev.id, outputPath })
      }
      void hydrate()
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
          speedBps: 0,
          etaSec: 0,
          error: ev.message ?? ev.error ?? 'Erro desconhecido'
        }
      }
      void hydrate()
    })
  )

  // Also subscribe to old-style events for compatibility
  unsubs.push(
    window.api.downloads.on('download:status', (event: unknown) => {
      const ev = event as { id: string; status: DownloadItem['status'] }
      if (!ev?.id) return
      void hydrate()
    })
  )

  unsubs.push(
    window.api.downloads.on('download:cancelled', (event: unknown) => {
      const ev = event as { id: string }
      if (!ev?.id) return
      upsertById(ev.id, { status: DownloadStatusEnum.Cancelled })
      void hydrate()
    })
  )
})

onUnmounted(() => {
  isMounted = false
  if (retryTimer !== null) {
    window.clearInterval(retryTimer)
    retryTimer = null
  }
  if (hydrateTimer !== null) {
    window.clearInterval(hydrateTimer)
    hydrateTimer = null
  }
  for (const unsub of unsubs) unsub()
})

// ── Data methods ───────────────────────────────────────────
async function hydrate(): Promise<void> {
  if (hydrateInFlight) {
    hydrateQueued = true
    return
  }
  hydrateInFlight = true
  try {
    if (!isMounted) return
    const fresh: DownloadItem[] = await window.api.downloads.list().catch(() => [])
    const freshById = new Map<string, DownloadItem>(fresh.map((item) => [item.id, item]))

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
    emit('skeleton-done')
  } finally {
    hydrateInFlight = false
    if (hydrateQueued) {
      hydrateQueued = false
      if (isMounted) void hydrate()
    }
  }
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

function childNodes(children?: DownloadChild[]): DerivedChildNode[] {
  if (!children?.length) {
    return []
  }
  return flattenChildTree(buildChildTree(children))
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
  if (isWaitingRetry(item)) return 'linear-gradient(90deg, #f59e0b, #fbbf24)'
  // Use provider color for active downloads
  const color = modulesById.value[item.moduleId]?.color ?? getProviderColor(item.moduleId)
  return `linear-gradient(90deg, ${color}, ${color}cc)`
}

function statusText(item: DownloadItem): string {
  const map: Record<string, string> = {
    pending: 'Na fila',
    downloading: 'Baixando',
    complete: 'Concluído',
    error: 'Erro',
    cancelled: 'Cancelado',
    paused: 'Pausado'
  }
  if (isWaitingRetry(item)) {
    return 'Aguardando retry'
  }
  return map[item.status] ?? item.status
}

function isWaitingRetry(item: DownloadItem): boolean {
  return item.status === DownloadStatusEnum.Pending
    && typeof item.retryAt === 'number'
    && item.retryAt > nowTick.value
}

function retryCountdown(item: DownloadItem): number {
  if (!item.retryAt) return 0
  return Math.max(0, Math.ceil((item.retryAt - nowTick.value) / 1000))
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
  if (secs < 3600) {
    const m = Math.floor(secs / 60)
    const s = Math.round(secs % 60)
    return `${m}m ${s}s`
  }
  const h = Math.floor(secs / 3600)
  const m = Math.floor((secs % 3600) / 60)
  return `${h}h ${m}m`
}

function childPercent(child: Pick<DownloadChild, 'size' | 'bytesDownloaded'>): number {
  if (!child.size || child.size <= 0) return 0
  const bytes = child.bytesDownloaded ?? 0
  return Math.max(0, Math.min(100, Math.floor((bytes / child.size) * 100)))
}

function childStatusText(status?: DownloadChild['status']): string {
  if (!status) return 'Na fila'
  if (status === DownloadStatusEnum.Pending) return 'Na fila'
  if (status === DownloadStatusEnum.Downloading) return 'Baixando'
  if (status === DownloadStatusEnum.Complete) return 'Concluído'
  if (status === DownloadStatusEnum.Error) return 'Erro'
  if (status === DownloadStatusEnum.Cancelled) return 'Cancelado'
  if (status === DownloadStatusEnum.Paused) return 'Pausado'
  return String(status)
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
  min-height: 0;
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
  width: 100%;
  min-width: 0;
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
  width: 100%;
  box-sizing: border-box;
  align-self: stretch;
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
  min-width: 0;
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
  display: inline-block;
  background-size: contain;
  background-position: center;
  background-repeat: no-repeat;
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

.meta-wait,
.meta-wait-reason {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  color: #f59e0b;
}

.meta-wait {
  font-weight: 600;
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
  overflow: hidden;
  transition: opacity 0.2s ease;
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
  flex-wrap: wrap;
}

.child-name-label {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
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
  display: inline-block;
  background-size: contain;
  background-position: center;
  background-repeat: no-repeat;
}

.child-size {
  flex-shrink: 0;
  color: var(--text-muted);
}

.child-folder-badge {
  display: inline-flex;
  align-items: center;
  padding: 2px 7px;
  border-radius: 999px;
  font-size: 10px;
  font-weight: 700;
  color: #b27a00;
  background: rgba(255, 193, 7, 0.12);
}

.child-row.is-folder-row {
  background: color-mix(in srgb, var(--bg-card) 82%, rgba(255, 193, 7, 0.07));
  border-radius: 8px;
  padding: 4px 6px;
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

/* ── Skeleton cards ─────────────────────────────────────────── */
@keyframes shimmer-skeleton {
  0%   { background-position: -200% 0; }
  100% { background-position:  200% 0; }
}

.skeleton-card {
  pointer-events: none;
  flex-direction: column;
  gap: 8px;
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
  animation: shimmer-skeleton 1.4s infinite;
}

.skeleton-title    { height: 14px; width: 55%; }
.skeleton-progress { height: 8px;  width: 100%; }
.skeleton-meta     { height: 10px; width: 35%; }
</style>
