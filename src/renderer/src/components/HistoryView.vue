<template>
  <div class="history-view">
    <!-- Toolbar -->
    <div class="history-toolbar">
      <span class="history-count">
        <i class="pi pi-history" style="font-size: 12px;"></i>
        {{ history.length }} item{{ history.length !== 1 ? 's' : '' }} no histórico
      </span>
      <div class="toolbar-actions">
        <div class="search-wrapper">
          <i class="pi pi-search search-icon"></i>
          <input
            v-model="search"
            class="search-input"
            placeholder="Buscar..."
            type="text"
          />
        </div>
        <button
          v-if="history.length > 0"
          class="clear-btn"
          @click="handleClear"
        >
          <i class="pi pi-trash"></i>
          Limpar
        </button>
      </div>
    </div>

    <!-- Empty state -->
    <div v-if="filtered.length === 0" class="empty-state">
      <div class="empty-icon">
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48" fill="none" width="44" height="44">
          <circle cx="24" cy="24" r="21" stroke="currentColor" stroke-width="1.5" opacity="0.3"/>
          <path d="M24 12 A12 12 0 0 1 36 24 A12 12 0 0 1 12 24 A12 12 0 0 1 24 12 Z" stroke="currentColor" stroke-width="1.5" fill="none" opacity="0.5"/>
          <path d="M24 18 L24 24 L28 26" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
        </svg>
      </div>
      <p class="empty-title">
        {{ history.length === 0 ? 'Histórico vazio' : 'Nenhum resultado' }}
      </p>
      <p class="empty-sub">
        {{ history.length === 0 ? 'Downloads concluídos aparecerão aqui' : `Nenhum resultado para "${search}"` }}
      </p>
    </div>

    <!-- List -->
    <div v-else class="history-list">
      <div
        v-for="item in filtered"
        :key="item.id"
        class="history-card"
      >
        <!-- Thumbnail / placeholder -->
        <div class="thumb-wrapper">
          <img
            v-if="item.thumbnail"
            :src="item.thumbnail"
            class="thumb-img"
            alt=""
          />
          <div v-else class="thumb-placeholder">
            <i class="pi pi-file" style="font-size: 18px; opacity: 0.4;"></i>
          </div>
        </div>

        <!-- Info -->
        <div class="card-body">
          <div class="card-title" :title="item.title || item.url">
            {{ item.title || item.url }}
          </div>
          <div class="card-meta">
            <span class="card-date">{{ formatDate(item.date) }}</span>
            <span v-if="item.formatId" class="card-format">{{ item.formatId.toUpperCase() }}</span>
          </div>
          <div v-if="item.outputPath" class="card-path" :title="item.outputPath">
            <i class="pi pi-folder" style="font-size: 10px;"></i>
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
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import Tooltip from 'primevue/tooltip'

const vTooltip = Tooltip

interface HistoryItem {
  id: string
  url: string
  title: string
  thumbnail: string
  date: string
  formatId: string
  outputPath?: string
}

const emit = defineEmits<{
  (e: 'redownload', url: string): void
}>()

const history = ref<HistoryItem[]>([])
const search = ref('')

const filtered = computed(() => {
  if (!search.value.trim()) return history.value
  const q = search.value.toLowerCase()
  return history.value.filter(
    (h) => h.title.toLowerCase().includes(q) || h.url.toLowerCase().includes(q)
  )
})

onMounted(async () => {
  try {
    history.value = (await window.api.loadHistory()).reverse()
  } catch {
    history.value = []
  }
})

function formatDate(iso: string): string {
  try {
    return new Date(iso).toLocaleString('pt-BR', {
      day: '2-digit',
      month: '2-digit',
      year: 'numeric',
      hour: '2-digit',
      minute: '2-digit'
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

function removeItem(id: string): void {
  history.value = history.value.filter((h) => h.id !== id)
  window.api.saveHistory([...history.value].reverse())
}

async function handleClear(): Promise<void> {
  history.value = []
  await window.api.clearHistory()
}

function addToHistory(item: HistoryItem): void {
  history.value.unshift(item)
  window.api.saveHistory([...history.value].reverse())
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
  transition: border-color 0.15s, box-shadow 0.15s;
}

.search-input::placeholder {
  color: var(--text-muted);
}

.search-input:focus {
  border-color: var(--accent-color);
  box-shadow: 0 0 0 2px rgba(124, 111, 255, 0.15);
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

/* ── List ───────────────────────────────────────────────────── */
.history-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
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
  transition: border-color 0.15s ease, box-shadow 0.15s ease;
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
