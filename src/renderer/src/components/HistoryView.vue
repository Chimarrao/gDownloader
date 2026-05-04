<template>
  <div class="history-view">
    <!-- Toolbar -->
    <div class="history-toolbar">
      <span class="history-count">
        <i class="pi pi-history" style="font-size: 12px"></i>
        {{ visibleHistory.length }} item{{ visibleHistory.length !== 1 ? 's' : '' }}
        nesta página
      </span>
      <div class="toolbar-actions">
        <div class="search-wrapper">
          <i class="pi pi-search search-icon"></i>
          <input
            v-model="search"
            class="search-input"
            placeholder="Buscar no histórico..."
            type="text"
          />
        </div>
        <select v-model="hostFilter" class="filter-select">
          <option value="">Todos os hosts</option>
          <option v-for="host in hostOptions" :key="host" :value="host">
            {{ host }}
          </option>
        </select>
        <input v-model="dateFrom" class="date-input" type="date" title="Data inicial" />
        <input v-model="dateTo" class="date-input" type="date" title="Data final" />
        <button
          v-if="history.length > 0"
          class="clear-btn"
          :class="{ active: duplicatesOnly }"
          @click="duplicatesOnly = !duplicatesOnly"
        >
          <i class="pi pi-clone"></i>
          Duplicatas
        </button>
        <button v-if="visibleHistory.length > 0" class="export-btn" @click="exportHistory('csv')">
          <i class="pi pi-file-excel"></i>
          CSV
        </button>
        <button v-if="visibleHistory.length > 0" class="export-btn" @click="exportHistory('json')">
          <i class="pi pi-code"></i>
          JSON
        </button>
        <button v-if="history.length > 0" class="clear-btn" @click="handleClear">
          <i class="pi pi-trash"></i>
          Limpar
        </button>
      </div>
    </div>
    <div v-if="exportFeedback" class="export-feedback">
      {{ exportFeedback }}
    </div>

    <!-- Empty state -->
    <div v-if="visibleHistory.length === 0" class="empty-state">
      <div class="empty-icon">
        <svg
          xmlns="http://www.w3.org/2000/svg"
          viewBox="0 0 48 48"
          fill="none"
          width="44"
          height="44"
        >
          <circle cx="24" cy="24" r="21" stroke="currentColor" stroke-width="1.5" opacity="0.3" />
          <path
            d="M24 12 A12 12 0 0 1 36 24 A12 12 0 0 1 12 24 A12 12 0 0 1 24 12 Z"
            stroke="currentColor"
            stroke-width="1.5"
            fill="none"
            opacity="0.5"
          />
          <path
            d="M24 18 L24 24 L28 26"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
          />
        </svg>
      </div>
      <p class="empty-title">
        {{ history.length === 0 ? 'Histórico vazio' : 'Nenhum resultado' }}
      </p>
      <p class="empty-sub">
        {{
          history.length === 0
            ? 'Downloads concluídos aparecerão aqui'
            : 'Ajuste os filtros para ver outros resultados'
        }}
      </p>
      <div class="empty-actions">
        <button class="empty-primary" @click="$emit('redownload', '')">
          <i class="pi pi-link"></i>
          Capturar links
        </button>
        <button v-if="search" class="empty-secondary" @click="search = ''">
          <i class="pi pi-times"></i>
          Limpar busca
        </button>
      </div>
    </div>

    <!-- List -->
    <div v-else class="history-list">
      <div v-for="item in visibleHistory" :key="item.id" class="history-card">
        <!-- Thumbnail / placeholder -->
        <div class="thumb-wrapper">
          <img v-if="item.thumbnail" :src="item.thumbnail" class="thumb-img" alt="" />
          <div v-else class="thumb-placeholder">
            <i class="pi pi-file" style="font-size: 18px; opacity: 0.4"></i>
          </div>
        </div>

        <!-- Info -->
        <div class="card-body">
          <div class="card-title" :title="item.title || item.url">
            {{ item.title || item.url }}
          </div>
          <div class="card-meta">
            <span class="card-date">{{ formatDate(item.date) }}</span>
            <span v-if="item.host" class="card-format">{{ item.host }}</span>
            <span v-if="item.formatId" class="card-format">{{ item.formatId.toUpperCase() }}</span>
          </div>
          <div v-if="item.outputPath" class="card-path" :title="item.outputPath">
            <i class="pi pi-folder" style="font-size: 10px"></i>
            {{ item.outputPath }}
          </div>
        </div>

        <!-- Actions -->
        <div class="card-actions">
          <button
            v-if="item.outputPath"
            v-tooltip.bottom="'Abrir arquivo'"
            class="action-btn action-success"
            @click="openFile(item.outputPath!)"
          >
            <i class="pi pi-play-circle"></i>
          </button>
          <button
            v-if="item.outputPath"
            v-tooltip.bottom="'Mostrar na pasta'"
            class="action-btn action-secondary"
            @click="showInFolder(item.outputPath!)"
          >
            <i class="pi pi-folder-open"></i>
          </button>
          <button
            v-tooltip.bottom="'Baixar novamente'"
            class="action-btn action-accent"
            @click="$emit('redownload', item.url)"
          >
            <i class="pi pi-download"></i>
          </button>
          <button
            v-tooltip.bottom="'Remover do histórico'"
            class="action-btn action-danger"
            @click="removeItem(item.id)"
          >
            <i class="pi pi-times"></i>
          </button>
        </div>
      </div>
    </div>
    <div v-if="history.length > 0" class="history-pagination">
      <button class="page-btn" :disabled="page === 0 || loading" @click="goToPage(page - 1)">
        <i class="pi pi-chevron-left"></i>
        Anterior
      </button>
      <span>Página {{ page + 1 }}</span>
      <button class="page-btn" :disabled="!hasNextPage || loading" @click="goToPage(page + 1)">
        Próxima
        <i class="pi pi-chevron-right"></i>
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, watch } from 'vue'
import Tooltip from 'primevue/tooltip'
import type { DownloadHistoryItem } from '../../../shared/types'

const vTooltip = Tooltip

defineEmits<{
  (e: 'redownload', url: string): void
}>()

const history = ref<DownloadHistoryItem[]>([])
const search = ref('')
const hostFilter = ref('')
const dateFrom = ref('')
const dateTo = ref('')
const hostOptions = ref<string[]>([])
const duplicatesOnly = ref(false)
const loading = ref(false)
const page = ref(0)
const pageSize = 80
const exportFeedback = ref('')
let searchTimer: ReturnType<typeof setTimeout> | null = null

const duplicateHashes = computed(() => {
  const counts = new Map<string, number>()
  for (const item of history.value) {
    if (!item.sha256Hash) continue
    counts.set(item.sha256Hash, (counts.get(item.sha256Hash) ?? 0) + 1)
  }
  return new Set([...counts.entries()].filter(([, count]) => count > 1).map(([hash]) => hash))
})

const visibleHistory = computed(() => {
  const base = duplicatesOnly.value
    ? history.value.filter((item) => item.sha256Hash && duplicateHashes.value.has(item.sha256Hash))
    : history.value
  return base
})

const hasNextPage = computed(() => history.value.length >= pageSize)

onMounted(async () => {
  await loadHosts()
  await loadHistoryPage()
})

watch([hostFilter, dateFrom, dateTo], () => {
  page.value = 0
  void loadHistoryPage()
})

watch(search, () => {
  if (searchTimer) clearTimeout(searchTimer)
  searchTimer = setTimeout(() => {
    page.value = 0
    void loadHistoryPage()
  }, 250)
})

async function loadHosts(): Promise<void> {
  hostOptions.value = await window.api.historyHosts().catch(() => [])
}

async function loadHistoryPage(): Promise<void> {
  loading.value = true
  try {
    history.value = await window.api.loadHistory({
      q: search.value,
      host: hostFilter.value,
      from: dateFrom.value,
      to: dateTo.value,
      page: page.value,
      pageSize,
    })
  } catch {
    history.value = []
  } finally {
    loading.value = false
  }
}

function goToPage(nextPage: number): void {
  page.value = Math.max(0, nextPage)
  void loadHistoryPage()
}

function formatDate(iso: string): string {
  try {
    return new Date(iso).toLocaleString('pt-BR', {
      day: '2-digit',
      month: '2-digit',
      year: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    })
  } catch {
    return iso
  }
}

async function openFile(filePath: string): Promise<void> {
  await window.api.openPath(filePath)
}

function showInFolder(filePath: string): void {
  window.api.showInFolder(filePath)
}

async function removeItem(id: string): Promise<void> {
  history.value = history.value.filter((h) => h.id !== id)
  await window.api.removeHistoryItem(id).catch(() => null)
  void loadHosts()
}

async function handleClear(): Promise<void> {
  history.value = []
  await window.api.clearHistory()
  await loadHosts()
}

function addToHistory(item: DownloadHistoryItem): void {
  history.value.unshift(item)
  window.api.appendHistory(item).catch(() => null)
  void loadHosts()
}

function exportHistory(format: 'csv' | 'json'): void {
  const items = visibleHistory.value
  const content = format === 'json' ? JSON.stringify(items, null, 2) : toCsv(items)
  const ok = window.api.clipboard.writeText(content)
  exportFeedback.value =
    format === 'json'
      ? 'JSON copiado para a área de transferência'
      : 'CSV copiado para a área de transferência'
  void ok.finally(() => {
    setTimeout(() => {
      exportFeedback.value = ''
    }, 2500)
  })
}

function toCsv(items: DownloadHistoryItem[]): string {
  const headers = ['id', 'title', 'url', 'host', 'date', 'formatId', 'outputPath', 'sha256Hash']
  const rows = items.map((item) => [
    item.id,
    item.title,
    item.url,
    item.host ?? '',
    item.date,
    item.formatId,
    item.outputPath ?? '',
    item.sha256Hash ?? '',
  ])
  return [headers, ...rows].map((row) => row.map(csvCell).join(',')).join('\n')
}

function csvCell(value: string): string {
  return `"${String(value).replace(/"/g, '""')}"`
}

defineExpose({ addToHistory })
</script>

<style scoped>
/* ── Layout ─────────────────────────────────────────────────── */
.history-view {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

/* ── Toolbar ────────────────────────────────────────────────── */
.history-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  flex-wrap: wrap;
}

.history-count {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  color: var(--text-muted);
}

.toolbar-actions {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}

/* Search */
.search-wrapper {
  position: relative;
  display: flex;
  align-items: center;
}

.search-icon {
  position: absolute;
  left: 9px;
  font-size: 11px;
  color: var(--text-muted);
  pointer-events: none;
}

.search-input {
  width: 180px;
  height: 32px;
  padding: 0 10px 0 28px;
  background: var(--bg-primary);
  border: 1px solid var(--border-color);
  border-radius: 8px;
  color: var(--text-primary);
  font-size: 12px;
  outline: none;
  transition:
    border-color 0.15s,
    box-shadow 0.15s;
}

.search-input::placeholder {
  color: var(--text-muted);
}

.search-input:focus {
  border-color: var(--accent-color);
  box-shadow: 0 0 0 2px rgba(124, 111, 255, 0.15);
}

.filter-select,
.date-input {
  height: 32px;
  padding: 0 10px;
  background: var(--bg-primary);
  border: 1px solid var(--border-color);
  border-radius: 8px;
  color: var(--text-primary);
  font-size: 12px;
  outline: none;
}

.filter-select {
  max-width: 160px;
}

.export-btn,
.page-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  height: 32px;
  padding: 0 12px;
  border-radius: 8px;
  border: 1px solid color-mix(in srgb, var(--accent-color) 32%, var(--border-color));
  background: color-mix(in srgb, var(--accent-color) 10%, transparent);
  color: var(--accent-color);
  font-size: 12px;
  cursor: pointer;
}

.page-btn:disabled {
  cursor: not-allowed;
  opacity: 0.45;
}

.export-feedback {
  align-self: flex-end;
  font-size: 11px;
  color: #22c55e;
}

/* Clear button */
.clear-btn {
  display: flex;
  align-items: center;
  gap: 5px;
  height: 32px;
  padding: 0 12px;
  border-radius: 8px;
  border: 1px solid rgba(239, 68, 68, 0.3);
  background: rgba(239, 68, 68, 0.08);
  color: #f87171;
  font-size: 12px;
  cursor: pointer;
  transition: all 0.15s ease;
}

.clear-btn:hover {
  background: rgba(239, 68, 68, 0.18);
  border-color: rgba(239, 68, 68, 0.6);
  color: #ef4444;
}

.clear-btn.active {
  border-color: color-mix(in srgb, var(--accent-color) 45%, transparent);
  background: color-mix(in srgb, var(--accent-color) 14%, transparent);
  color: var(--accent-color);
}

/* ── Empty state ────────────────────────────────────────────── */
.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  min-height: 220px;
  gap: 8px;
  color: var(--text-muted);
}

.empty-icon {
  opacity: 0.4;
  margin-bottom: 6px;
}

.empty-title {
  font-size: 15px;
  font-weight: 600;
  margin: 0;
}

.empty-sub {
  font-size: 13px;
  margin: 0;
  opacity: 0.7;
}

.empty-actions {
  display: flex;
  gap: 8px;
  margin-top: 8px;
}

.empty-primary,
.empty-secondary {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  height: 32px;
  padding: 0 12px;
  border-radius: 7px;
  font-size: 12px;
  cursor: pointer;
}

.empty-primary {
  border: 1px solid var(--accent-color);
  background: var(--accent-color);
  color: white;
}

.empty-secondary {
  border: 1px solid var(--border-color);
  background: var(--bg-card);
  color: var(--text-primary);
}

/* ── List ───────────────────────────────────────────────────── */
.history-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.history-pagination {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 12px;
  color: var(--text-muted);
  font-size: 12px;
  padding-top: 4px;
}

/* ── Card ───────────────────────────────────────────────────── */
.history-card {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px 14px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: 10px;
  transition:
    border-color 0.15s ease,
    box-shadow 0.15s ease;
}

.history-card:hover {
  border-color: color-mix(in srgb, var(--accent-color) 30%, var(--border-color));
  box-shadow: var(--shadow-card);
}

/* Thumbnail */
.thumb-wrapper {
  flex-shrink: 0;
}

.thumb-img {
  width: 64px;
  height: 36px;
  border-radius: 6px;
  object-fit: cover;
  display: block;
}

.thumb-placeholder {
  width: 64px;
  height: 36px;
  background: var(--surface-section);
  border-radius: 6px;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--text-muted);
  border: 1px solid var(--border-color);
}

/* Card body */
.card-body {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 3px;
}

.card-title {
  font-size: 13px;
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--text-primary);
}

.card-meta {
  display: flex;
  align-items: center;
  gap: 8px;
}

.card-date {
  font-size: 11px;
  color: var(--text-muted);
}

.card-format {
  font-size: 10px;
  font-weight: 700;
  padding: 1px 6px;
  border-radius: 4px;
  background: rgba(124, 111, 255, 0.12);
  color: var(--accent-color);
  border: 1px solid rgba(124, 111, 255, 0.2);
  letter-spacing: 0.3px;
}

.card-path {
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: 10.5px;
  color: var(--text-muted);
  font-family: 'Courier New', monospace;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  opacity: 0.8;
}

/* Actions */
.card-actions {
  display: flex;
  align-items: center;
  gap: 4px;
  flex-shrink: 0;
}

.action-btn {
  width: 30px;
  height: 30px;
  border-radius: 7px;
  border: 1px solid var(--border-color);
  background: transparent;
  color: var(--text-muted);
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  font-size: 12px;
  transition: all 0.15s ease;
}

.action-success:hover {
  background: rgba(34, 197, 94, 0.12);
  border-color: rgba(34, 197, 94, 0.4);
  color: #22c55e;
}

.action-secondary:hover {
  background: var(--bg-hover);
  border-color: var(--accent-color);
  color: var(--text-primary);
}

.action-accent:hover {
  background: rgba(124, 111, 255, 0.12);
  border-color: rgba(124, 111, 255, 0.4);
  color: var(--accent-color);
}

.action-danger:hover {
  background: rgba(239, 68, 68, 0.12);
  border-color: rgba(239, 68, 68, 0.4);
  color: #ef4444;
}
</style>
