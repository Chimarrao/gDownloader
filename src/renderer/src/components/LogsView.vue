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
          <option value="warn">Aviso</option>
          <option value="error">Erro</option>
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
      <div v-if="filteredEntries.length === 0" class="logs-empty">
        Nenhum log encontrado
      </div>
      <div
        v-else
        v-for="(entry, index) in filteredEntries"
        :key="`${index}-${entry.raw}`"
        class="log-entry"
        :class="`is-${entry.level}`"
        :title="entry.raw"
      >
        <span class="log-level">{{ entry.levelLabel }}</span>
        <span class="log-module">{{ entry.module }}</span>
        <span class="log-message">{{ entry.friendly }}</span>
      </div>
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

type LogLevel = 'info' | 'warn' | 'error'

interface LogEntry {
  raw: string
  level: LogLevel
  levelLabel: string
  module: string
  friendly: string
}

const entries = computed(() => lines.value.map(parseLogLine))

const modules = computed(() => {
  const found = new Set<string>()
  for (const entry of entries.value) {
    if (entry.module) found.add(entry.module)
  }
  return [...found].sort((a, b) => a.localeCompare(b))
})

const filteredEntries = computed(() => {
  const q = search.value.trim().toLowerCase()
  return entries.value.filter((entry) => {
    const lower = `${entry.raw} ${entry.friendly} ${entry.module}`.toLowerCase()
    if (levelFilter.value !== 'all' && entry.level !== levelFilter.value) return false
    if (moduleFilter.value !== 'all' && entry.module !== moduleFilter.value) return false
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
})

onUnmounted(() => {
  disposeWatch?.()
})

async function refresh(): Promise<void> {
  const payload = await window.api.logs.tail(700).catch(() => ({ path: '', lines: [] }))
  logPath.value = payload.path
  lines.value = payload.lines
}

async function copyFiltered(): Promise<void> {
  await window.api.clipboard.writeText(filteredEntries.value.map((entry) => entry.raw).join('\n'))
}

function parseLogLine(line: string): LogEntry {
  const lower = line.toLowerCase()
  const level: LogLevel = lower.includes('error')
    ? 'error'
    : lower.includes('warn')
      ? 'warn'
      : 'info'
  const target = line.match(/\s([a-zA-Z0-9_:.-]+):/)?.[1] ?? line.match(/\[([^\]]+)\]/)?.[1] ?? 'Sistema'
  const module = target.split('::').filter(Boolean).pop() ?? target

  return {
    raw: line,
    level,
    levelLabel: level === 'error' ? 'Erro' : level === 'warn' ? 'Aviso' : 'Info',
    module,
    friendly: friendlyMessage(line),
  }
}

function friendlyMessage(line: string): string {
  const lower = line.toLowerCase()
  if (lower.includes('download adicionado')) return 'Download adicionado à fila.'
  if (lower.includes('iniciando tentativa de download')) return 'Download iniciado.'
  if (lower.includes('download concluído') || lower.includes('download concluido')) return 'Download concluído.'
  if (lower.includes('travado sem progresso')) return 'Sem progresso por muito tempo; uma nova tentativa será feita.'
  if (lower.includes('captcha')) return 'Captcha precisa de atenção.'
  if (lower.includes('rate') || lower.includes('limite')) return 'Servidor limitou a velocidade ou novas tentativas.'
  if (lower.includes('yt-dlp') && lower.includes('falhou')) return 'YouTube não conseguiu concluir a operação.'
  if (lower.includes('erro') || lower.includes('error')) return 'Ocorreu uma falha. Veja o detalhe técnico no tooltip.'
  if (lower.includes('warn')) return 'Atenção necessária, mas o app continua funcionando.'

  return line
    .replace(/^\S+\s+\S+\s+/, '')
    .replace(/\s+[a-zA-Z0-9_:.-]+:\s*/, '')
    .trim()
    .slice(0, 220) || 'Evento registrado.'
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

.log-entry {
  display: grid;
  grid-template-columns: 64px minmax(96px, 150px) minmax(0, 1fr);
  align-items: center;
  gap: 10px;
  min-height: 32px;
  padding: 6px 8px;
  border-radius: 6px;
  color: var(--text-secondary);
  font-size: 12px;
  line-height: 1.35;
}

.log-entry.is-error {
  color: #ef4444;
  background: rgba(239, 68, 68, 0.08);
}

.log-entry.is-warn {
  color: #f59e0b;
  background: rgba(245, 158, 11, 0.08);
}

.log-entry.is-info {
  color: var(--text-primary);
}

.log-level {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  height: 22px;
  border-radius: 999px;
  background: color-mix(in srgb, currentColor 12%, transparent);
  font-size: 11px;
  font-weight: 800;
}

.log-module {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--text-muted);
  font-size: 11px;
  font-weight: 700;
}

.log-message {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
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
