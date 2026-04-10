<template>
  <div
    style="
      height: 100vh;
      display: flex;
      flex-direction: column;
      background: var(--bg-primary);
      color: var(--text-primary);
      padding: 16px;
      gap: 14px;
    "
  >
    <header
      style="
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 12px;
      "
    >
      <div style="display: flex; align-items: center; gap: 10px">
        <i class="pi pi-download" style="font-size: 20px; color: var(--accent-color)"></i>
        <h1 style="font-size: 18px; margin: 0">gDownloader</h1>
      </div>
      <label style="display: flex; align-items: center; gap: 8px; font-size: 12px">
        Tema
        <select
          :value="currentTheme"
          style="
            background: var(--bg-secondary);
            color: var(--text-primary);
            border: 1px solid var(--border-color);
            border-radius: 8px;
            padding: 6px 8px;
          "
          @change="onThemeChange"
        >
          <option v-for="opt in themeOptions" :key="opt.id" :value="opt.id">{{ opt.label }}</option>
        </select>
      </label>
    </header>

    <section
      style="
        background: var(--bg-secondary);
        border: 1px solid var(--border-color);
        border-radius: 12px;
        padding: 12px;
      "
    >
      <div style="display: flex; gap: 10px; align-items: flex-start">
        <Textarea
          v-model="urlsInput"
          rows="4"
          autoResize
          placeholder="Cole uma URL por linha"
          style="flex: 1"
        />
        <Button
          label="Adicionar a fila"
          icon="pi pi-plus"
          :loading="adding"
          :disabled="recognizedRows.length === 0 || adding"
          @click="addRecognizedToQueue"
        />
      </div>

      <div v-if="urlRows.length > 0" style="display: flex; flex-wrap: wrap; gap: 8px; margin-top: 10px">
        <div
          v-for="row in urlRows"
          :key="row.url"
          :style="{
            border: '1px solid var(--border-color)',
            borderRadius: '999px',
            fontSize: '11px',
            padding: '4px 8px',
            background: row.module ? 'var(--bg-hover)' : 'rgba(239, 68, 68, 0.15)',
            color: row.module ? 'var(--text-primary)' : '#fca5a5'
          }"
          :title="row.url"
        >
          {{ shortenUrl(row.url) }}
          <span v-if="row.module" :style="{ marginLeft: '6px', color: row.module.color }">
            {{ row.module.name }}
          </span>
          <span v-else style="margin-left: 6px">URL nao suportada</span>
        </div>
      </div>

      <p v-if="lastError" style="margin: 10px 0 0; font-size: 12px; color: #fca5a5">{{ lastError }}</p>
    </section>

    <nav style="display: flex; gap: 8px">
      <Button
        label="Downloads"
        :badge="String(downloadCount)"
        :severity="activeTab === 'downloads' ? 'primary' : 'secondary'"
        text
        @click="activeTab = 'downloads'"
      />
      <Button
        label="Historico"
        :severity="activeTab === 'history' ? 'primary' : 'secondary'"
        text
        @click="activeTab = 'history'"
      />
      <Button
        label="Estatisticas"
        :severity="activeTab === 'stats' ? 'primary' : 'secondary'"
        text
        @click="activeTab = 'stats'"
      />
    </nav>

    <main
      style="
        flex: 1;
        overflow: auto;
        background: var(--bg-secondary);
        border: 1px solid var(--border-color);
        border-radius: 12px;
        padding: 12px;
      "
    >
      <DownloadList
        v-show="activeTab === 'downloads'"
        @count-change="downloadCount = $event"
        @download-complete="onDownloadComplete"
      />
      <HistoryView v-show="activeTab === 'history'" ref="historyViewRef" @redownload="onRedownload" />
      <StatsView v-show="activeTab === 'stats'" />
    </main>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import Button from 'primevue/button'
import Textarea from 'primevue/textarea'
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

const urlsInput = ref('')
const urlRows = ref<UrlRow[]>([])
const adding = ref(false)
const lastError = ref('')
const activeTab = ref<'downloads' | 'history' | 'stats'>('downloads')
const downloadCount = ref(0)
const detectToken = ref(0)
const historyViewRef = ref<HistoryViewExposed | null>(null)
const pendingHistory = new Map<string, PendingHistoryMeta>()
const settings = ref({ outputDir: '~/Downloads' })

const { currentTheme, setTheme, initTheme, themeOptions } = useTheme()

const recognizedRows = computed(() => urlRows.value.filter((row) => row.module !== null))

watch(urlsInput, () => {
  void detectProviders()
})

onMounted(async () => {
  initTheme()

  const [savedSettings] = await Promise.all([
    window.api.settings.load().catch(() => null),
    window.api.modules.list().catch(() => [])
  ])

  if (savedSettings) {
    settings.value.outputDir = savedSettings.outputDir
    if (themeOptions.some((opt) => opt.id === (savedSettings.theme as ThemeId))) {
      setTheme(savedSettings.theme as ThemeId)
    }
  }

  downloadCount.value = (await window.api.downloads.list().catch(() => [])).length
})

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
        settings.value.outputDir
      )
      pendingHistory.set(item.id, {
        title: info.name,
        url: row.url,
        formatId: detectFormatId(info.name)
      })
    } catch (error) {
      lastError.value = `Falha ao adicionar: ${row.url} (${stringifyError(error)})`
    }
  }

  adding.value = false
  urlsInput.value = ''
  await detectProviders()
  downloadCount.value = (await window.api.downloads.list().catch(() => [])).length
  activeTab.value = 'downloads'
}

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

function shortenUrl(url: string): string {
  if (url.length <= 48) return url
  return `${url.slice(0, 45)}...`
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

function stringifyError(error: unknown): string {
  if (error instanceof Error) return error.message
  return String(error)
}
</script>

