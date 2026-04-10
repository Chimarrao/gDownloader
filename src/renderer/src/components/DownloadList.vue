<template>
  <div>
    <div
      v-if="items.length === 0"
      style="
        min-height: 220px;
        display: grid;
        place-items: center;
        color: var(--text-secondary);
        font-size: 13px;
        border: 1px dashed var(--surface-border);
        border-radius: 12px;
      "
    >
      Nenhum download ativo. Cole uma URL acima.
    </div>

    <div v-else style="display: flex; flex-direction: column; gap: 10px">
      <div
        v-for="item in orderedItems"
        :key="item.id"
        style="
          background: var(--surface-card);
          border: 1px solid var(--surface-border);
          border-radius: 12px;
          padding: 12px;
        "
      >
        <div style="display: flex; align-items: center; gap: 10px; margin-bottom: 8px">
          <span
            :style="{
              fontSize: '11px',
              padding: '2px 8px',
              borderRadius: '999px',
              color: '#fff',
              background: moduleColor(item.moduleId)
            }"
          >
            {{ moduleLabel(item.moduleId) }}
          </span>
          <div
            style="
              flex: 1;
              min-width: 0;
              font-size: 13px;
              font-weight: 600;
              white-space: nowrap;
              overflow: hidden;
              text-overflow: ellipsis;
            "
            :title="item.title"
          >
            {{ item.title }}
          </div>
          <span class="status-chip" :class="`status-${item.status}`">{{ statusText(item.status) }}</span>
        </div>

        <div style="height: 8px; background: var(--surface-section); border-radius: 999px; overflow: hidden">
          <div
            :style="{
              width: item.percent + '%',
              height: '100%',
              background: 'var(--accent-gradient, var(--accent-color))',
              transition: 'width 0.2s ease'
            }"
          ></div>
        </div>

        <div style="display: flex; justify-content: space-between; margin-top: 8px; align-items: center">
          <div style="font-size: 11px; color: var(--text-secondary)">
            {{ Math.round(item.percent) }}% · {{ formatSpeed(item.speedBps) }} · ETA {{ formatEta(item.etaSec) }}
          </div>
          <Button
            v-if="item.status === 'pending' || item.status === 'downloading'"
            icon="pi pi-times"
            label="Cancelar"
            severity="danger"
            size="small"
            text
            @click="cancel(item.id)"
          />
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import Button from 'primevue/button'
import { DownloadStatus as DownloadStatusEnum } from '../../../shared/constants'
import type { DownloadItem } from '../../../shared/types'

type DownloadStatus = DownloadItem['status']

interface ModuleSummary {
  id: string
  name: string
  color: string
}

interface ProgressEvent {
  id: string
  percent: number
  speedBps: number
  etaSec: number
}

interface StatusEvent {
  id: string
  status: DownloadStatus
}

interface CompleteEvent {
  id: string
  outputPath: string
}

interface ErrorEvent {
  id: string
  error: string
}

interface CancelledEvent {
  id: string
}

const emit = defineEmits<{
  (e: 'count-change', count: number): void
  (e: 'download-complete', payload: CompleteEvent): void
}>()

const items = ref<DownloadItem[]>([])
const modulesById = ref<Record<string, ModuleSummary>>({})
const unsubs: Array<() => void> = []

const orderedItems = computed(() => [...items.value].sort((a, b) => b.addedAt - a.addedAt))

onMounted(async () => {
  const modules = await window.api.modules.list()
  modulesById.value = modules.reduce<Record<string, ModuleSummary>>((acc, mod) => {
    acc[mod.id] = { id: mod.id, name: mod.name, color: mod.color }
    return acc
  }, {})

  await hydrate()

  unsubs.push(window.api.downloads.on('download:progress', onProgress))
  unsubs.push(window.api.downloads.on('download:status', onStatus))
  unsubs.push(window.api.downloads.on('download:complete', onComplete))
  unsubs.push(window.api.downloads.on('download:error', onError))
  unsubs.push(window.api.downloads.on('download:cancelled', onCancelled))
})

onUnmounted(() => {
  for (const unsub of unsubs) {
    unsub()
  }
})

async function hydrate(): Promise<void> {
  items.value = await window.api.downloads.list()
  emit('count-change', items.value.length)
}

function moduleLabel(moduleId: string): string {
  return modulesById.value[moduleId]?.name ?? moduleId
}

function moduleColor(moduleId: string): string {
  return modulesById.value[moduleId]?.color ?? '#64748b'
}

function statusText(status: DownloadStatus): string {
  const map: Record<DownloadStatus, string> = {
    pending: 'Na fila',
    downloading: 'Baixando',
    complete: 'Concluido',
    error: 'Erro',
    cancelled: 'Cancelado',
    paused: 'Pausado'
  }
  return map[status]
}

function formatSpeed(speedBps: number): string {
  if (!speedBps || speedBps < 0) return '0 KB/s'
  const kb = speedBps / 1024
  if (kb < 1024) return `${kb.toFixed(1)} KB/s`
  return `${(kb / 1024).toFixed(2)} MB/s`
}

function formatEta(etaSec: number): string {
  if (!etaSec || etaSec < 0) return '--'
  if (etaSec < 60) return `${Math.round(etaSec)}s`
  const min = Math.floor(etaSec / 60)
  const sec = Math.round(etaSec % 60)
  return `${min}m ${sec}s`
}

function upsertById(id: string, patch: Partial<DownloadItem>): void {
  const idx = items.value.findIndex((item) => item.id === id)
  if (idx === -1) {
    void hydrate()
    return
  }
  items.value[idx] = { ...items.value[idx], ...patch }
}

function removeById(id: string): void {
  items.value = items.value.filter((item) => item.id !== id)
  emit('count-change', items.value.length)
}

function isProgressEvent(data: unknown): data is ProgressEvent {
  if (!data || typeof data !== 'object') return false
  return 'id' in data && 'percent' in data
}

function isStatusEvent(data: unknown): data is StatusEvent {
  if (!data || typeof data !== 'object') return false
  return 'id' in data && 'status' in data
}

function isCompleteEvent(data: unknown): data is CompleteEvent {
  if (!data || typeof data !== 'object') return false
  return 'id' in data && 'outputPath' in data
}

function isErrorEvent(data: unknown): data is ErrorEvent {
  if (!data || typeof data !== 'object') return false
  return 'id' in data && 'error' in data
}

function isCancelledEvent(data: unknown): data is CancelledEvent {
  if (!data || typeof data !== 'object') return false
  return 'id' in data
}

function onProgress(data: unknown): void {
  if (!isProgressEvent(data)) return
  upsertById(data.id, {
    percent: data.percent,
    speedBps: data.speedBps,
    etaSec: data.etaSec
  })
}

function onStatus(data: unknown): void {
  if (!isStatusEvent(data)) return
  upsertById(data.id, { status: data.status })
}

function onComplete(data: unknown): void {
  if (!isCompleteEvent(data)) return
  upsertById(data.id, {
    status: DownloadStatusEnum.Complete,
    percent: 100,
    outputPath: data.outputPath
  })
  emit('download-complete', data)
  removeById(data.id)
}

function onError(data: unknown): void {
  if (!isErrorEvent(data)) return
  upsertById(data.id, { status: DownloadStatusEnum.Error, error: data.error })
  removeById(data.id)
}

function onCancelled(data: unknown): void {
  if (!isCancelledEvent(data)) return
  removeById(data.id)
}

async function cancel(id: string): Promise<void> {
  await window.api.downloads.cancel(id)
}
</script>

<style scoped>
.status-chip {
  font-size: 10px;
  font-weight: 700;
  padding: 2px 8px;
  border-radius: 999px;
  border: 1px solid transparent;
}

.status-pending {
  background: rgba(148, 163, 184, 0.18);
  color: #94a3b8;
}

.status-downloading {
  background: rgba(34, 197, 94, 0.18);
  color: #22c55e;
}

.status-complete {
  background: rgba(34, 197, 94, 0.18);
  color: #22c55e;
}

.status-error {
  background: rgba(239, 68, 68, 0.18);
  color: #ef4444;
}

.status-cancelled {
  background: rgba(100, 116, 139, 0.2);
  color: #94a3b8;
}

.status-paused {
  background: rgba(250, 204, 21, 0.2);
  color: #facc15;
}
</style>

