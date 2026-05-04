<template>
  <div class="app-root">
    <header class="topbar">
      <div class="brand">
        <div class="brand-mark">
          <svg viewBox="0 0 24 24" fill="none" width="18" height="18">
            <path
              d="M12 3 L12 15 M7 11 L12 16 L17 11"
              stroke="currentColor"
              stroke-width="2.1"
              stroke-linecap="round"
              stroke-linejoin="round"
            />
            <path d="M4 19 H20" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
          </svg>
        </div>
        <strong>{{ t('appName') }}</strong>
      </div>
      <div class="topbar-actions">
        <SpeedWidget
          :speed-history="speedHistory"
          :current-speed="currentSpeed"
          :per-host-speed="perHostSpeed"
        />
        <div class="help-menu-wrap" data-tour="help-tour">
          <button class="help-btn" title="Ajuda" @click="helpMenuOpen = !helpMenuOpen">
            <i class="pi pi-question-circle"></i>
            <span>Ajuda</span>
          </button>
          <div v-if="helpMenuOpen" class="help-menu">
            <button @click="startOnboarding">
              <i class="pi pi-map"></i>
              <span>Refazer tour</span>
            </button>
          </div>
        </div>
      </div>
    </header>

    <nav class="tab-bar">
      <button
        class="tab-btn"
        :class="{ active: activeTab === 'downloads' }"
        data-tour="downloads-tab"
        @click="activeTab = 'downloads'"
      >
        <i class="pi pi-download"></i>
        <span>{{ t('downloads') }}</span>
        <span v-if="downloadCount > 0" class="tab-badge">{{ downloadCount }}</span>
      </button>
      <button
        class="tab-btn"
        :class="{ active: activeTab === 'grabber' }"
        data-tour="grabber-tab"
        @click="activeTab = 'grabber'"
      >
        <i class="pi pi-link"></i>
        <span>{{ t('linkGrabber') }}</span>
      </button>
      <button
        class="tab-btn"
        :class="{ active: activeTab === 'settings' }"
        data-tour="settings-tab"
        @click="activeTab = 'settings'"
      >
        <i class="pi pi-cog"></i>
        <span>{{ t('settings') }}</span>
      </button>
      <button
        class="tab-btn"
        :class="{ active: activeTab === 'account' }"
        @click="activeTab = 'account'"
      >
        <i class="pi pi-user"></i>
        <span>{{ t('account') }}</span>
      </button>
      <button class="tab-btn" :class="{ active: activeTab === 'logs' }" @click="activeTab = 'logs'">
        <i class="pi pi-list"></i>
        <span>Logs</span>
      </button>
    </nav>

    <main class="app-main">
      <section v-show="activeTab === 'downloads'" class="panel downloads-panel" data-tour="download-queue">
        <DownloadList
          :skeleton-count="skeletonCount"
          @count-change="
            downloadCount = $event
            updateTrayStats()
          "
          @download-complete="onDownloadComplete"
          @global-speed="onGlobalSpeed"
          @skeleton-done="skeletonCount = 0"
          @open-grabber="activeTab = 'grabber'"
        />
      </section>

      <section v-show="activeTab === 'grabber'" class="panel">
        <LinkGrabber
          :incoming-url="clipboardIncomingUrl"
          @added="handleAddedToQueue"
          @adding-urls="onAddingUrls"
        />
      </section>

      <section v-show="activeTab === 'settings'" class="panel">
        <AppSettings />
      </section>

      <section v-show="activeTab === 'account'" class="panel">
        <AccountSettings />
      </section>

      <section v-show="activeTab === 'logs'" class="panel">
        <LogsView />
      </section>
    </main>
    <OnboardingTour
      v-if="showOnboarding"
      :active-tab="activeTab"
      @navigate="activeTab = $event"
      @complete="completeOnboarding"
    />
  </div>
</template>

<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue'
import type { DownloadHistoryItem } from '../../shared/types'
import DownloadList from './components/DownloadList.vue'
import LinkGrabber from './components/LinkGrabber.vue'
import AppSettings from './components/AppSettings.vue'
import AccountSettings from './components/AccountSettings.vue'
import LogsView from './components/LogsView.vue'
import OnboardingTour from './components/OnboardingTour.vue'
import SpeedWidget from './components/SpeedWidget.vue'
import { setLocale, useI18n } from './i18n'
import { applyUiPreferences, useTheme, type ThemeId } from './themes'
import { pushRingBuffer } from './utils/ring-buffer'

type AppTab = 'downloads' | 'grabber' | 'settings' | 'account' | 'logs'

interface DownloadCompletePayload {
  id: string
  outputPath: string
  url?: string
  title?: string
  sha256Hash?: string
}

const activeTab = ref<AppTab>('downloads')
const downloadCount = ref(0)
const skeletonCount = ref(0)
const speedHistory = ref<number[]>(new Array(60).fill(0))
const currentSpeed = ref(0)
const perHostSpeed = ref<Record<string, number>>({})
const clipboardIncomingUrl = ref('')
const showOnboarding = ref(false)
const helpMenuOpen = ref(false)
let currentSettings: Awaited<ReturnType<typeof window.api.settings.load>> | null = null
let speedTicker: ReturnType<typeof setInterval> | null = null
let disposeClipboardDetected: (() => void) | null = null
let appMounted = true

const statsTickHandler = (event: Event): void => {
  onStatsTick(event as CustomEvent)
}

function onGlobalSpeed(bps: number): void {
  currentSpeed.value = bps
  updateTrayStats()
}

function updateTrayStats(): void {
  try {
    const bps = currentSpeed.value
    let speed: string
    if (bps >= 1024 * 1024) {
      speed = `${(bps / (1024 * 1024)).toFixed(1)} MB/s`
    } else if (bps >= 1024) {
      speed = `${(bps / 1024).toFixed(0)} KB/s`
    } else {
      speed = `${bps} B/s`
    }
    window.api.tray.updateStats({ activeCount: downloadCount.value, speed })
  } catch {
    // tray API may not be available in some environments
  }
}

onUnmounted(() => {
  appMounted = false
  if (speedTicker) clearInterval(speedTicker)
  disposeClipboardDetected?.()
  window.removeEventListener('stats-tick', statsTickHandler)
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

  disposeClipboardDetected = window.api.clipboard.onLinkDetected((payload) => {
    if (!payload.url) return
    clipboardIncomingUrl.value = payload.url
    activeTab.value = 'grabber'
  })
  window.addEventListener('stats-tick', statsTickHandler)

  const settings = await window.api.settings.load().catch(() => null)
  if (!settings) return
  currentSettings = settings
  if (settings.locale) {
    setLocale(settings.locale)
  }
  if (themeOptions.some((option) => option.id === settings.theme)) {
    setTheme(settings.theme as ThemeId)
  }
  applyUiPreferences(settings)
  if (!settings.onboardingCompleted) {
    showOnboarding.value = true
  }
})

function startOnboarding(): void {
  helpMenuOpen.value = false
  showOnboarding.value = true
}

async function completeOnboarding(): Promise<void> {
  showOnboarding.value = false
  const settings = currentSettings ?? await window.api.settings.load().catch(() => null)
  if (!settings) return
  currentSettings = await window.api.settings
    .save({ ...settings, onboardingCompleted: true })
    .catch(() => settings)
}

function onStatsTick(event: CustomEvent): void {
  const detail = event.detail as {
    total_speed_bps?: number
    per_host_speed?: Record<string, number>
  }
  if (detail.per_host_speed) perHostSpeed.value = detail.per_host_speed
  if (typeof detail.total_speed_bps === 'number') currentSpeed.value = detail.total_speed_bps
}

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

  const history = await window.api.loadHistory({ pageSize: 500 }).catch(() => [])
  const existing = history.find((item: DownloadHistoryItem) => item.id === payload.id)
  if (existing) return

  const item = {
    id: payload.id,
    url: payload.url ?? '',
    title: payload.title || payload.outputPath.split('/').pop() || payload.outputPath,
    thumbnail: '',
    date: new Date().toISOString(),
    formatId: '',
    outputPath: payload.outputPath,
    sha256Hash: payload.sha256Hash,
  }

  await window.api.appendHistory(item).catch(() => null)
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

.topbar-actions {
  display: inline-flex;
  align-items: center;
  gap: 10px;
}

.help-menu-wrap {
  position: relative;
}

.help-btn {
  height: 34px;
  display: inline-flex;
  align-items: center;
  gap: 7px;
  padding: 0 11px;
  border: 1px solid var(--border-color);
  border-radius: 9px;
  background: var(--bg-card);
  color: var(--text-primary);
  cursor: pointer;
  font-size: 12px;
  font-weight: 700;
}

.help-btn:hover {
  border-color: color-mix(in srgb, var(--accent-color) 35%, var(--border-color));
  color: var(--accent-color);
}

.help-menu {
  position: absolute;
  right: 0;
  top: calc(100% + 8px);
  z-index: 20;
  min-width: 176px;
  padding: 6px;
  border: 1px solid var(--border-color);
  border-radius: 10px;
  background: var(--bg-card);
  box-shadow: 0 16px 48px rgba(0, 0, 0, 0.22);
}

.help-menu button {
  width: 100%;
  height: 34px;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 0 10px;
  border: 0;
  border-radius: 8px;
  background: transparent;
  color: var(--text-primary);
  cursor: pointer;
  font-size: 13px;
  font-weight: 700;
  text-align: left;
}

.help-menu button:hover {
  background: color-mix(in srgb, var(--accent-color) 12%, transparent);
  color: var(--accent-color);
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
  transition:
    color 0.15s ease,
    border-color 0.15s ease;
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
