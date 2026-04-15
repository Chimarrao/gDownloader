<template>
  <div class="app-shell">
    <!-- ── Header ── -->
    <header class="app-header">
      <div class="header-brand">
        <div class="brand-icon">
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" width="20" height="20">
            <path d="M12 3 L12 15 M7 11 L12 16 L17 11" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"/>
            <path d="M4 19 H20" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
          </svg>
        </div>
        <h1 class="brand-name">gDownloader</h1>
      </div>

      <div class="header-controls">
        <div class="theme-select-wrapper">
          <i class="pi pi-palette theme-icon"></i>
          <select class="theme-select" :value="currentTheme" @change="onThemeChange">
            <option v-for="opt in themeOptions" :key="opt.id" :value="opt.id">{{ opt.label }}</option>
          </select>
        </div>

        <button class="icon-btn" title="Pasta de destino" @click="showDirPicker = !showDirPicker">
          <i class="pi pi-folder"></i>
        </button>
      </div>
    </header>

    <!-- ── Directory picker (collapsible) ── -->
    <div v-if="showDirPicker" class="dir-picker">
      <i class="pi pi-folder-open" style="color: var(--accent-color); font-size: 13px;"></i>
      <span class="dir-label">Destino:</span>
      <input
        v-model="outputDir"
        class="dir-input"
        placeholder="~/Downloads"
        @change="saveSettings"
      />
    </div>

    <!-- ── URL Input Section ── -->
    <section class="url-section">
      <div class="url-input-row">
        <div class="url-input-wrapper">
          <i class="pi pi-link url-input-icon"></i>
          <textarea
            v-model="urlsInput"
            class="url-textarea"
            placeholder="Cole uma ou mais URLs aqui (uma por linha)..."
            rows="1"
            @input="autoResizeTextarea"
            ref="textareaRef"
          ></textarea>
        </div>
        <button
          class="add-btn"
          :class="{ loading: adding, disabled: recognizedRows.length === 0 || adding }"
          :disabled="recognizedRows.length === 0 || adding"
          @click="addRecognizedToQueue"
        >
          <span v-if="!adding" class="add-btn-inner">
            <i class="pi pi-plus"></i>
            <span>Adicionar</span>
          </span>
          <span v-else class="add-btn-inner">
            <i class="pi pi-spin pi-spinner"></i>
            <span>Adicionando</span>
          </span>
        </button>
      </div>

      <!-- Provider chips -->
      <div v-if="urlRows.length > 0" class="provider-chips">
        <div
          v-for="row in urlRows"
          :key="row.url"
          class="provider-chip"
          :class="{ 'chip-unsupported': !row.module }"
          :style="row.module ? { '--chip-color': row.module.color } : {}"
          :title="row.url"
        >
          <span v-if="row.module" class="chip-dot" :style="{ background: row.module.color }"></span>
          <span v-else class="chip-dot chip-dot-error"></span>
          <span class="chip-label">
            <template v-if="row.module">{{ row.module.name }}: </template>
            <template v-else>Nao suportado: </template>
            {{ shortenFilename(row.url) }}
          </span>
        </div>
      </div>

      <!-- Error message -->
      <p v-if="lastError" class="error-msg">
        <i class="pi pi-exclamation-triangle"></i>
        {{ lastError }}
      </p>
    </section>

    <!-- ── Tab Navigation ── -->
    <nav class="tab-nav">
      <button
        class="tab-btn"
        :class="{ active: activeTab === 'downloads' }"
        @click="activeTab = 'downloads'"
      >
        <i class="pi pi-download"></i>
        Downloads
        <span v-if="downloadCount > 0" class="tab-badge">{{ downloadCount }}</span>
      </button>
      <button
        class="tab-btn"
        :class="{ active: activeTab === 'history' }"
        @click="activeTab = 'history'"
      >
        <i class="pi pi-history"></i>
        Histórico
      </button>
      <button
        class="tab-btn"
        :class="{ active: activeTab === 'stats' }"
        @click="activeTab = 'stats'"
      >
        <i class="pi pi-chart-bar"></i>
        Estatísticas
      </button>
    </nav>

    <!-- ── Main Content ── -->
    <main class="app-main">
      <DownloadList
        v-show="activeTab === 'downloads'"
        @count-change="downloadCount = $event"
        @download-complete="onDownloadComplete"
      />
      <HistoryView
        v-show="activeTab === 'history'"
        ref="historyViewRef"
        @redownload="onRedownload"
      />
      <StatsView v-show="activeTab === 'stats'" />
    </main>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from 'vue'
import DownloadList from './components/DownloadList.vue'
import HistoryView from './components/HistoryView.vue'
import StatsView from './components/StatsView.vue'
import { useTheme, type ThemeId } from './themes'

interface ModuleSummary {
  id: string
  name: string
  icon: string
  color: string
}

interface UrlRow {
  url: string
  module: ModuleSummary | null
}

interface HistoryItem {
  id: string
  url: string
  title: string
  thumbnail: string
  date: string
  formatId: string
  outputPath?: string
}

interface HistoryViewExposed {
  addToHistory: (item: HistoryItem) => void
}

interface PendingHistoryMeta {
  title: string
  url: string
  formatId: string
}

interface DownloadCompletePayload {
  id: string
  outputPath: string
}

// ── State ──────────────────────────────────────────────────
const urlsInput = ref('')
const urlRows = ref<UrlRow[]>([])
const adding = ref(false)
const lastError = ref('')
const activeTab = ref<'downloads' | 'history' | 'stats'>('downloads')
const downloadCount = ref(0)
const detectToken = ref(0)
const historyViewRef = ref<HistoryViewExposed | null>(null)
const pendingHistory = new Map<string, PendingHistoryMeta>()
const outputDir = ref('~/Downloads')
const showDirPicker = ref(false)
const textareaRef = ref<HTMLTextAreaElement | null>(null)

const { currentTheme, setTheme, initTheme, themeOptions } = useTheme()

const recognizedRows = computed(() => urlRows.value.filter((row) => row.module !== null))

// ── Watchers ───────────────────────────────────────────────
watch(urlsInput, () => {
  void detectProviders()
})

// ── Lifecycle ──────────────────────────────────────────────
onMounted(async () => {
  initTheme()

  const [savedSettings] = await Promise.all([
    window.api.settings.load().catch(() => null),
    window.api.modules.list().catch(() => [])
  ])

  if (savedSettings) {
    if (savedSettings.outputDir) outputDir.value = savedSettings.outputDir
    if (themeOptions.some((opt) => opt.id === (savedSettings.theme as ThemeId))) {
      setTheme(savedSettings.theme as ThemeId)
    }
  }

  downloadCount.value = (await window.api.downloads.list().catch(() => [])).length
})

// ── URL detection ──────────────────────────────────────────
function parseUniqueUrls(text: string): string[] {
  const unique = new Set<string>()
  for (const line of text.split('\n')) {
    const cleaned = line.trim()
    if (cleaned) unique.add(cleaned)
  }
  return [...unique]
}

async function detectProviders(): Promise<void> {
  const token = ++detectToken.value
  lastError.value = ''
  const urls = parseUniqueUrls(urlsInput.value)
  if (urls.length === 0) {
    urlRows.value = []
    return
  }

  const detected = await Promise.all(
    urls.map(async (url) => {
      try {
        const module = await window.api.modules.detect(url)
        return { url, module }
      } catch {
        return { url, module: null }
      }
    })
  )

  if (token === detectToken.value) {
    urlRows.value = detected
  }
}

// ── Add to queue ───────────────────────────────────────────
async function addRecognizedToQueue(): Promise<void> {
  if (recognizedRows.value.length === 0) return

  adding.value = true
  lastError.value = ''

  for (const row of recognizedRows.value) {
    const module = row.module
    if (!module) continue

    try {
      const info = await window.api.modules.fileInfo(module.id, row.url)
      const item = await window.api.downloads.add(
        row.url,
        module.id,
        info.name,
        info.size,
        outputDir.value
      )
      pendingHistory.set(item.id, {
        title: info.name,
        url: row.url,
        formatId: detectFormatId(info.name)
      })
    } catch (error) {
      lastError.value = `Falha ao adicionar: ${shortenFilename(row.url)} (${stringifyError(error)})`
    }
  }

  adding.value = false
  urlsInput.value = ''
  await detectProviders()
  downloadCount.value = (await window.api.downloads.list().catch(() => [])).length
  activeTab.value = 'downloads'

  await nextTick()
  if (textareaRef.value) {
    textareaRef.value.style.height = 'auto'
  }
}

// ── Helpers ────────────────────────────────────────────────
function detectFormatId(name: string): string {
  const idx = name.lastIndexOf('.')
  if (idx <= 0 || idx === name.length - 1) return 'arquivo'
  return name.slice(idx + 1).toLowerCase()
}

function onThemeChange(event: Event): void {
  const target = event.target as HTMLSelectElement | null
  if (!target) return
  setTheme(target.value as ThemeId)
}

function shortenFilename(url: string): string {
  try {
    const u = new URL(url)
    const parts = u.pathname.split('/').filter(Boolean)
    const name = parts[parts.length - 1] || u.hostname
    return name.length > 40 ? name.slice(0, 37) + '...' : name
  } catch {
    return url.length > 48 ? url.slice(0, 45) + '...' : url
  }
}

function onRedownload(url: string): void {
  urlsInput.value = urlsInput.value.trim() ? `${urlsInput.value.trim()}\n${url}` : url
  activeTab.value = 'downloads'
}

function onDownloadComplete(payload: DownloadCompletePayload): void {
  const meta = pendingHistory.get(payload.id)
  if (!meta || !historyViewRef.value) return

  historyViewRef.value.addToHistory({
    id: payload.id,
    title: meta.title,
    url: meta.url,
    thumbnail: '',
    date: new Date().toISOString(),
    formatId: meta.formatId,
    outputPath: payload.outputPath
  })
  pendingHistory.delete(payload.id)
}

function saveSettings(): void {
  window.api.settings.save({
    theme: currentTheme.value,
    locale: 'pt-BR',
    outputDir: outputDir.value,
    maxConcurrentDownloads: 3,
    fontSize: 14,
    fontFamily: 'Inter',
    uiZoom: 1,
    nativeNotification: true
  })
}

function autoResizeTextarea(event: Event): void {
  const el = event.target as HTMLTextAreaElement
  el.style.height = 'auto'
  el.style.height = Math.min(el.scrollHeight, 120) + 'px'
}

function stringifyError(error: unknown): string {
  if (error instanceof Error) return error.message
  return String(error)
}
</script>

<style scoped>
/* ── App Shell ─────────────────────────────────────────────── */
.app-shell {
  height: 100vh;
  display: flex;
  flex-direction: column;
  background: var(--bg-primary);
  color: var(--text-primary);
  overflow: hidden;
}

/* ── Header ─────────────────────────────────────────────────── */
.app-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 16px;
  background: var(--bg-secondary);
  border-bottom: 1px solid var(--border-color);
  flex-shrink: 0;
  -webkit-app-region: drag;
}

.header-brand {
  display: flex;
  align-items: center;
  gap: 10px;
}

.brand-icon {
  width: 32px;
  height: 32px;
  border-radius: 8px;
  background: var(--accent-gradient);
  display: flex;
  align-items: center;
  justify-content: center;
  color: #fff;
  flex-shrink: 0;
  box-shadow: 0 2px 8px rgba(124, 111, 255, 0.4);
}

.brand-name {
  font-size: 16px;
  font-weight: 700;
  margin: 0;
  background: var(--accent-gradient);
  -webkit-background-clip: text;
  -webkit-text-fill-color: transparent;
  background-clip: text;
  letter-spacing: -0.3px;
}

.header-controls {
  display: flex;
  align-items: center;
  gap: 8px;
  -webkit-app-region: no-drag;
}

/* Theme selector */
.theme-select-wrapper {
  display: flex;
  align-items: center;
  gap: 6px;
  background: var(--bg-hover);
  border: 1px solid var(--border-color);
  border-radius: 8px;
  padding: 5px 10px;
}

.theme-icon {
  font-size: 12px;
  color: var(--text-muted);
}

.theme-select {
  background: transparent;
  border: none;
  color: var(--text-primary);
  font-size: 12px;
  cursor: pointer;
  outline: none;
  padding: 0;
}

.theme-select option {
  background: var(--bg-secondary);
  color: var(--text-primary);
}

/* Icon button */
.icon-btn {
  width: 32px;
  height: 32px;
  border-radius: 8px;
  border: 1px solid var(--border-color);
  background: var(--bg-hover);
  color: var(--text-muted);
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  transition: all 0.15s ease;
  font-size: 13px;
}

.icon-btn:hover {
  color: var(--text-primary);
  border-color: var(--accent-color);
  background: var(--bg-hover);
}

/* ── Dir Picker ─────────────────────────────────────────────── */
.dir-picker {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 16px;
  background: var(--bg-secondary);
  border-bottom: 1px solid var(--border-color);
  flex-shrink: 0;
}

.dir-label {
  font-size: 12px;
  color: var(--text-muted);
  white-space: nowrap;
}

.dir-input {
  flex: 1;
  background: var(--bg-primary);
  border: 1px solid var(--border-color);
  border-radius: 6px;
  color: var(--text-primary);
  font-size: 12px;
  padding: 4px 10px;
  outline: none;
  font-family: monospace;
  transition: border-color 0.15s;
}

.dir-input:focus {
  border-color: var(--accent-color);
}

/* ── URL Section ────────────────────────────────────────────── */
.url-section {
  padding: 12px 16px;
  background: var(--bg-secondary);
  border-bottom: 1px solid var(--border-color);
  flex-shrink: 0;
}

.url-input-row {
  display: flex;
  gap: 10px;
  align-items: flex-start;
}

.url-input-wrapper {
  flex: 1;
  position: relative;
  display: flex;
  align-items: flex-start;
}

.url-input-icon {
  position: absolute;
  left: 12px;
  top: 12px;
  font-size: 13px;
  color: var(--text-muted);
  pointer-events: none;
  z-index: 1;
}

.url-textarea {
  width: 100%;
  min-height: 42px;
  max-height: 120px;
  background: var(--bg-primary);
  border: 1px solid var(--border-color);
  border-radius: 10px;
  color: var(--text-primary);
  font-size: 13px;
  font-family: inherit;
  padding: 10px 12px 10px 36px;
  resize: none;
  outline: none;
  overflow-y: auto;
  transition: border-color 0.15s ease, box-shadow 0.15s ease;
  line-height: 1.5;
}

.url-textarea::placeholder {
  color: var(--text-muted);
}

.url-textarea:focus {
  border-color: var(--accent-color);
  box-shadow: 0 0 0 3px rgba(124, 111, 255, 0.15);
}

/* Add button */
.add-btn {
  height: 42px;
  padding: 0 18px;
  border-radius: 10px;
  border: none;
  background: var(--accent-gradient);
  color: #fff;
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
  flex-shrink: 0;
  transition: opacity 0.15s ease, transform 0.1s ease, box-shadow 0.15s ease;
  box-shadow: 0 2px 12px rgba(124, 111, 255, 0.35);
}

.add-btn:hover:not(.disabled) {
  opacity: 0.92;
  transform: translateY(-1px);
  box-shadow: 0 4px 16px rgba(124, 111, 255, 0.5);
}

.add-btn:active:not(.disabled) {
  transform: translateY(0);
}

.add-btn.disabled {
  opacity: 0.45;
  cursor: not-allowed;
  box-shadow: none;
}

.add-btn-inner {
  display: flex;
  align-items: center;
  gap: 7px;
  white-space: nowrap;
}

/* Provider chips */
.provider-chips {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-top: 10px;
}

.provider-chip {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 4px 10px 4px 8px;
  border-radius: 999px;
  font-size: 11px;
  font-weight: 500;
  border: 1px solid var(--border-color);
  background: var(--bg-hover);
  color: var(--text-primary);
  max-width: 260px;
  overflow: hidden;
}

.provider-chip:not(.chip-unsupported) {
  border-color: color-mix(in srgb, var(--chip-color, var(--accent-color)) 40%, transparent);
  background: color-mix(in srgb, var(--chip-color, var(--accent-color)) 10%, transparent);
}

.chip-unsupported {
  background: rgba(239, 68, 68, 0.08);
  border-color: rgba(239, 68, 68, 0.3);
  color: #fca5a5;
}

.chip-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  flex-shrink: 0;
}

.chip-dot-error {
  background: #ef4444;
}

.chip-label {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* Error message */
.error-msg {
  display: flex;
  align-items: center;
  gap: 6px;
  margin: 8px 0 0;
  font-size: 12px;
  color: #fca5a5;
}

/* ── Tab Navigation ─────────────────────────────────────────── */
.tab-nav {
  display: flex;
  gap: 2px;
  padding: 8px 16px 0;
  background: var(--bg-secondary);
  border-bottom: 1px solid var(--border-color);
  flex-shrink: 0;
}

.tab-btn {
  display: flex;
  align-items: center;
  gap: 7px;
  padding: 8px 14px;
  border: none;
  border-bottom: 2px solid transparent;
  background: transparent;
  color: var(--text-muted);
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  border-radius: 8px 8px 0 0;
  transition: all 0.15s ease;
  position: relative;
  margin-bottom: -1px;
}

.tab-btn i {
  font-size: 12px;
}

.tab-btn:hover:not(.active) {
  color: var(--text-primary);
  background: var(--bg-hover);
}

.tab-btn.active {
  color: var(--accent-color);
  border-bottom-color: var(--accent-color);
  background: var(--bg-hover);
}

.tab-badge {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 18px;
  height: 18px;
  padding: 0 5px;
  border-radius: 999px;
  font-size: 10px;
  font-weight: 700;
  background: var(--accent-gradient);
  color: #fff;
  line-height: 1;
}

/* ── Main Content ───────────────────────────────────────────── */
.app-main {
  flex: 1;
  overflow-y: auto;
  padding: 14px 16px;
  background: var(--bg-primary);
}
</style>
