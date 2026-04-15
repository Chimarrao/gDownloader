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
      <p class="empty-sub">Cole uma URL acima para começar</p>
    </div>

    <!-- Download items -->
    <div v-else class="items-container">
      <transition-group name="item">
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
              <span class="item-title" :title="item.title">{{ item.title || item.url }}</span>
              <div class="item-actions">
                <span class="status-badge" :class="`badge-${item.status}`">
                  <span class="badge-dot" :class="`dot-${item.status}`"></span>
                  {{ statusText(item.status) }}
                </span>
                <button
                  v-if="item.status === 'pending' || item.status === 'downloading'"
                  class="cancel-btn"
                  title="Cancelar"
                  @click="cancel(item.id)"
                >
                  <i class="pi pi-times"></i>
                </button>
                <button
                  v-if="item.status === 'complete' && item.outputPath"
                  class="open-btn"
                  title="Mostrar na pasta"
                  @click="openFolder(item.outputPath!)"
                >
                  <i class="pi pi-folder-open"></i>
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

              <template v-if="item.status === 'error' && item.error">
                <span class="meta-sep">·</span>
                <span class="meta-error" :title="item.error">{{ item.error }}</span>
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
          </div>
        </div>
      </transition-group>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { DownloadStatus as DownloadStatusEnum } from '../../../shared/constants'
import type { DownloadItem } from '../../../shared/types'
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
const unsubs: Array<() => void> = []

// ── Computed ───────────────────────────────────────────────
const orderedItems = computed(() => [...items.value].sort((a, b) => b.addedAt - a.addedAt))

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
      const ev = event as { type?: string; id: string; bytes?: number; total?: number; speed?: number; eta?: number; status?: string }
      if (!ev?.id) return
      const idx = items.value.findIndex((i) => i.id === ev.id)
      if (idx >= 0) {
        const total = ev.total ?? items.value[idx].size
        const bytes = ev.bytes ?? 0
        items.value[idx] = {
          ...items.value[idx],
          percent: total > 0 ? Math.min(100, Math.floor((bytes / total) * 100)) : items.value[idx].percent,
          speedBps: ev.speed ?? 0,
          etaSec: ev.eta ?? 0,
          status: (ev.status as DownloadItem['status']) ?? items.value[idx].status,
          size: total > 0 ? total : items.value[idx].size
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
  items.value = await window.api.downloads.list().catch(() => [])
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
}

function openFolder(filePath: string): void {
  window.api.showInFolder(filePath)
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
</script>

<style scoped>
/* ── Container ──────────────────────────────────────────────── */
.download-list {
  display: flex;
  flex-direction: column;
  gap: 0;
}

/* ── Empty state ────────────────────────────────────────────── */
.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  min-height: 260px;
  gap: 10px;
  color: var(--text-muted);
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
  gap: 8px;
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

.item-title {
  flex: 1;
  font-size: 13px;
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--text-primary);
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
.open-btn {
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

/* ── List transitions ───────────────────────────────────────── */
.item-enter-active {
  animation: slide-in-up 0.2s ease;
}

.item-leave-active {
  animation: slide-in-up 0.15s ease reverse;
}

.item-move {
  transition: transform 0.2s ease;
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
