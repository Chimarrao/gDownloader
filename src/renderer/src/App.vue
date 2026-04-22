<template>
  <div class="app-root">
    <header class="topbar">
      <div class="brand">
        <div class="brand-mark">
          <svg viewBox="0 0 24 24" fill="none" width="18" height="18">
            <path d="M12 3 L12 15 M7 11 L12 16 L17 11" stroke="currentColor" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"/>
            <path d="M4 19 H20" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
          </svg>
        </div>
        <strong>{{ t('appName') }}</strong>
      </div>
      <SpeedWidget
        :speed-history="speedHistory"
        :current-speed="currentSpeed"
      />
    </header>

    <nav class="tab-bar">
      <button class="tab-btn" :class="{ active: activeTab === 'downloads' }" @click="activeTab = 'downloads'">
        <i class="pi pi-download"></i>
        <span>{{ t('downloads') }}</span>
        <span v-if="downloadCount > 0" class="tab-badge">{{ downloadCount }}</span>
      </button>
      <button class="tab-btn" :class="{ active: activeTab === 'grabber' }" @click="activeTab = 'grabber'">
        <i class="pi pi-link"></i>
        <span>{{ t('linkGrabber') }}</span>
      </button>
      <button class="tab-btn" :class="{ active: activeTab === 'settings' }" @click="activeTab = 'settings'">
        <i class="pi pi-cog"></i>
        <span>{{ t('settings') }}</span>
      </button>
      <button class="tab-btn" :class="{ active: activeTab === 'account' }" @click="activeTab = 'account'">
        <i class="pi pi-user"></i>
        <span>{{ t('account') }}</span>
      </button>
    </nav>

    <main class="app-main">
      <section v-show="activeTab === 'downloads'" class="panel downloads-panel">
        <DownloadList
          :skeleton-count="skeletonCount"
          @count-change="downloadCount = $event"
          @download-complete="onDownloadComplete"
          @global-speed="onGlobalSpeed"
          @skeleton-done="skeletonCount = 0"
        />
      </section>

      <section v-show="activeTab === 'grabber'" class="panel">
        <LinkGrabber @added="handleAddedToQueue" @adding-urls="onAddingUrls" />
      </section>

      <section v-show="activeTab === 'settings'" class="panel">
        <AppSettings />
      </section>

      <section v-show="activeTab === 'account'" class="panel">
        <AccountSettings />
      </section>
    </main>
  </div>
</template>

<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue'
import type { DownloadHistoryItem } from '../../shared/types'
import DownloadList from './components/DownloadList.vue'
import LinkGrabber from './components/LinkGrabber.vue'
import AppSettings from './components/AppSettings.vue'
import AccountSettings from './components/AccountSettings.vue'
import SpeedWidget from './components/SpeedWidget.vue'
import { setLocale, useI18n } from './i18n'
import { applyUiPreferences, useTheme, type ThemeId } from './themes'
import { pushRingBuffer } from './utils/ring-buffer'

type AppTab = 'downloads' | 'grabber' | 'settings' | 'account'

interface DownloadCompletePayload {
  id: string
  outputPath: string
}

const activeTab = ref<AppTab>('downloads')
const downloadCount = ref(0)
const skeletonCount = ref(0)
const speedHistory = ref<number[]>(new Array(60).fill(0))
const currentSpeed = ref(0)
let speedTicker: ReturnType<typeof setInterval> | null = null
let appMounted = true

function onGlobalSpeed(bps: number): void {
  currentSpeed.value = bps
}

onUnmounted(() => {
  appMounted = false
  if (speedTicker) clearInterval(speedTicker)
  disposeTheme()
})
const { t } = useI18n()
const { initTheme, disposeTheme, setTheme, themeOptions } = useTheme()

onMounted(async () => {
  initTheme()
  // Start speed ticker regardless of settings availability
  speedTicker = setInterval(() => {
    if (!appMounted) return
    speedHistory.value = pushRingBuffer(speedHistory.value, currentSpeed.value, 60)
  }, 120)
  const settings = await window.api.settings.load().catch(() => null)
  if (!settings) return
  if (settings.locale) {
    setLocale(settings.locale)
  }
  if (themeOptions.some((option) => option.id === settings.theme)) {
    setTheme(settings.theme as ThemeId)
  }
  applyUiPreferences(settings)
})

function onAddingUrls(count: number): void {
  skeletonCount.value = count
}

function handleAddedToQueue(): void {
  activeTab.value = 'downloads'
}

async function onDownloadComplete(payload: DownloadCompletePayload): Promise<void> {
  const settings = await window.api.settings.load().catch(() => null)
  if (settings?.nativeNotification) {
    const title = payload.outputPath.split('/').pop() || payload.outputPath
    await window.api.system.notify('Download concluído', title).catch(() => false)
  }

  const history = await window.api.loadHistory().catch(() => [])
  const existing = history.find((item: DownloadHistoryItem) => item.id === payload.id)
  if (existing) return

  const item = {
    id: payload.id,
    url: '',
    title: payload.outputPath.split('/').pop() || payload.outputPath,
    thumbnail: '',
    date: new Date().toISOString(),
    formatId: '',
    outputPath: payload.outputPath,
  }

  await window.api.saveHistory([...(history as DownloadHistoryItem[]), item]).catch(() => null)
}
</script>

<style scoped>
.app-root {
  height: 100vh;
  display: flex;
  flex-direction: column;
  background: var(--bg-primary);
  color: var(--text-primary);
  overflow: hidden;
}

.topbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 14px 18px;
  border-bottom: 1px solid var(--border-color);
  background: var(--bg-secondary);
}

.brand {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 15px;
}

.brand-mark {
  width: 30px;
  height: 30px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: 9px;
  background: color-mix(in srgb, var(--accent-color) 16%, transparent);
  color: var(--accent-color);
}

.tab-bar {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 18px 0;
  background: var(--bg-primary);
  border-bottom: 1px solid var(--border-color);
}

.tab-btn {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding: 10px 12px;
  border: none;
  border-bottom: 2px solid transparent;
  background: transparent;
  color: var(--text-muted);
  cursor: pointer;
  font-size: 13px;
  transition: color 0.15s ease, border-color 0.15s ease;
}

.tab-btn:hover {
  color: var(--text-primary);
}

.tab-btn.active {
  color: var(--accent-color);
  border-bottom-color: var(--accent-color);
}

.tab-badge {
  min-width: 18px;
  height: 18px;
  padding: 0 6px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: 999px;
  background: var(--accent-color);
  color: #fff;
  font-size: 11px;
  font-weight: 700;
}

.app-main {
  flex: 1;
  min-height: 0;
  display: flex;
  overflow: hidden;
  padding: 18px;
}

.panel {
  flex: 1;
  min-width: 0;
  min-height: 0;
  display: flex;
  align-items: stretch;
  overflow: auto;
}

.panel > * {
  flex: 1 1 auto;
  width: 100%;
  min-width: 0;
  min-height: 0;
}

.downloads-panel {
  width: 100%;
  max-width: none;
  margin: 0;
  align-self: stretch;
  min-width: 0;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}
</style>
