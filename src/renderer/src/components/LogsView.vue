<template>
  <div class="logs-view">
    <div class="logs-toolbar">
      <div class="logs-title">
        <i class="pi pi-list"></i>
        <span>Logs</span>
        <small :title="logPath">{{ logPath || 'backend/logs/app.log' }}</small>
      </div>
      <div class="logs-filters">
        <select v-model="levelFilter" class="logs-select">
          <option value="all">Todos</option>
          <option value="info">Info</option>
          <option value="warn">Warn</option>
          <option value="error">Error</option>
        </select>
        <select v-model="moduleFilter" class="logs-select">
          <option value="all">Módulos</option>
          <option v-for="module in modules" :key="module" :value="module">{{ module }}</option>
        </select>
        <input v-model="search" class="logs-search" type="text" placeholder="Buscar..." />
        <button class="logs-btn" @click="copyFiltered">
          <i class="pi pi-copy"></i>
          Exportar
        </button>
      </div>
    </div>

    <div class="logs-list">
      <div v-if="filteredLines.length === 0" class="logs-empty">
        Nenhum log encontrado
      </div>
      <pre v-else v-for="(line, index) in filteredLines" :key="`${index}-${line}`" class="log-line" :class="lineClass(line)">{{ line }}</pre>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'

const lines = ref<string[]>([])
const logPath = ref('')
const levelFilter = ref('all')
const moduleFilter = ref('all')
const search = ref('')
let disposeWatch: (() => void) | null = null
let refreshTimer: ReturnType<typeof setInterval> | null = null

const modules = computed(() => {
  const found = new Set<string>()
  for (const line of lines.value) {
    const target = line.match(/\s([a-zA-Z0-9_:.-]+):/)?.[1] ?? line.match(/\[([^\]]+)\]/)?.[1]
    if (target) found.add(target)
  }
  return [...found].sort((a, b) => a.localeCompare(b))
})

const filteredLines = computed(() => {
  const q = search.value.trim().toLowerCase()
  return lines.value.filter((line) => {
    const lower = line.toLowerCase()
    if (levelFilter.value !== 'all' && !lower.includes(levelFilter.value)) return false
    if (moduleFilter.value !== 'all' && !line.includes(moduleFilter.value)) return false
    if (q && !lower.includes(q)) return false
    return true
  })
})

onMounted(async () => {
  await refresh()
  disposeWatch = window.api.logs.watch((payload) => {
    logPath.value = payload.path
    lines.value = payload.lines
  })
  refreshTimer = setInterval(refresh, 2500)
})

onUnmounted(() => {
  disposeWatch?.()
  if (refreshTimer) clearInterval(refreshTimer)
})

async function refresh(): Promise<void> {
  const payload = await window.api.logs.tail(700).catch(() => ({ path: '', lines: [] }))
  logPath.value = payload.path
  lines.value = payload.lines
}

async function copyFiltered(): Promise<void> {
  await window.api.clipboard.writeText(filteredLines.value.join('\n'))
}

function lineClass(line: string): string {
  const lower = line.toLowerCase()
  if (lower.includes('error')) return 'is-error'
  if (lower.includes('warn')) return 'is-warn'
  return 'is-info'
}
</script>

<style scoped>
.logs-view {
  display: flex;
  flex-direction: column;
  min-height: 0;
  height: 100%;
  gap: 10px;
}

.logs-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  flex-wrap: wrap;
}

.logs-title {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
  font-weight: 700;
}

.logs-title small {
  max-width: 360px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--text-muted);
  font-size: 11px;
  font-weight: 500;
}

.logs-filters {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}

.logs-select,
.logs-search,
.logs-btn {
  height: 32px;
  border: 1px solid var(--border-color);
  border-radius: 7px;
  background: var(--bg-card);
  color: var(--text-primary);
  font-size: 12px;
}

.logs-select {
  padding: 0 8px;
}

.logs-search {
  width: 190px;
  padding: 0 10px;
}

.logs-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 0 10px;
  cursor: pointer;
}

.logs-list {
  flex: 1;
  min-height: 0;
  overflow: auto;
  border: 1px solid var(--border-color);
  border-radius: 8px;
  background: color-mix(in srgb, var(--bg-card) 92%, #000);
  padding: 8px;
}

.log-line {
  margin: 0;
  padding: 3px 4px;
  border-radius: 4px;
  white-space: pre-wrap;
  word-break: break-word;
  font-size: 11px;
  line-height: 1.45;
  color: var(--text-muted);
}

.log-line.is-error {
  color: #ef4444;
  background: rgba(239, 68, 68, 0.08);
}

.log-line.is-warn {
  color: #f59e0b;
}

.log-line.is-info {
  color: var(--text-primary);
}

.logs-empty {
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 180px;
  color: var(--text-muted);
  font-size: 13px;
}
</style>
