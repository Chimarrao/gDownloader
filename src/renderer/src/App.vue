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
        <button
          class="quick-toggle-btn"
          :class="{ active: clipboardMonitorEnabled }"
          :title="clipboardMonitorEnabled ? 'Captura de links ativa' : 'Captura de links desligada'"
          @click="toggleClipboardMonitor"
        >
          <i class="pi pi-link"></i>
          <span>Captura</span>
        </button>
        <button class="quick-icon-btn" title="Alternar tema claro/escuro" @click="toggleQuickTheme">
          <i :class="effectiveTheme === 'light' ? 'pi pi-moon' : 'pi pi-sun'"></i>
        </button>
        <div class="top-speed" title="Velocidade agregada">
          <canvas ref="topSpeedCanvasRef" class="top-speed-chart" width="128" height="28" aria-hidden="true"></canvas>
          <span>{{ formatSpeed(currentSpeed) }}</span>
        </div>
        <div class="tor-widget" :class="[`tor-${torState.state}`, { open: torPanelOpen }]">
          <button class="tor-main-btn" :disabled="torBusy" @click="toggleTorPanel">
            <span class="tor-icon" aria-hidden="true" v-html="torIconSvg"></span>
            <strong>Tor</strong>
            <em>{{ torStatusLabel }}</em>
          </button>
          <div v-if="torPanelOpen" class="tor-panel">
            <div class="tor-panel-head">
              <div>
                <strong>{{ torPanelTitle }}</strong>
                <span>{{ torEndpointLabel }}</span>
              </div>
              <button
                class="tor-power-btn"
                :disabled="torBusy"
                @click="torState.state === 'connected' ? disconnectTor() : connectTor()"
              >
                <i :class="torState.state === 'connected' ? 'pi pi-power-off' : 'pi pi-play'"></i>
                <span>{{ torPowerLabel }}</span>
              </button>
            </div>
            <div class="tor-route" :class="{ empty: torRouteNodes.length === 0 }">
              <div
                v-for="(node, index) in torRouteNodes"
                :key="node.role"
                class="tor-node"
                :class="{ active: torState.state === 'connected' || torState.state === 'connecting', pulse: torState.state === 'connecting' && index === torPulseIndex }"
              >
                <span class="tor-node-dot">{{ node.code }}</span>
                <strong>{{ node.role }}</strong>
                <em>{{ node.country }}</em>
              </div>
              <div v-if="torRouteNodes.length === 0" class="tor-empty-state">
                <strong>{{ torState.state === 'connected' ? 'Circuito confirmado' : 'Não conectado' }}</strong>
                <span>{{ torState.state === 'connected' ? 'Saída validada; rota detalhada indisponível' : 'Nenhum circuito Tor ativo' }}</span>
              </div>
            </div>
            <div class="tor-panel-meta">
              <span>{{ torExitLabel }}</span>
              <span class="tor-test-result" :class="{ ok: torState.isTor === true, warn: torState.isTor === false }">
                {{ torTestLabel }}
              </span>
            </div>
            <div class="tor-panel-actions">
              <button :disabled="torBusy || torState.state !== 'connected'" @click="testTorConnection">
                <i class="pi pi-bolt"></i>
                <span>Testar conexão</span>
              </button>
              <button :disabled="torBusy || torState.state !== 'connected'" @click="newTorIdentity">
                <i class="pi pi-refresh"></i>
                <span>Nova identidade</span>
              </button>
            </div>
            <p v-if="torError" class="tor-error">{{ torError }}</p>
          </div>
        </div>
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
      <button
        class="tab-btn"
        :class="{ active: activeTab === 'speed-history' }"
        @click="activeTab = 'speed-history'"
      >
        <i class="pi pi-chart-line"></i>
        <span>Histórico de Velocidade</span>
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
          :tor-active="torState.state === 'connected'"
          @count-change="onDownloadCountChange"
          @download-complete="onDownloadComplete"
          @global-speed="onGlobalSpeed"
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

      <section v-show="activeTab === 'speed-history'" class="panel speed-history-panel">
        <div class="speed-history-header">
          <div>
            <h2>Histórico de Velocidade</h2>
            <span>Velocidade global agregada de todos os downloads</span>
          </div>
          <strong>{{ formatSpeed(currentSpeed) }}</strong>
        </div>
        <canvas
          ref="speedHistoryCanvasRef"
          class="speed-history-canvas"
          width="1120"
          height="320"
          aria-label="Histórico de velocidade global"
        ></canvas>
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
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import type { DownloadHistoryItem } from '../../shared/types'
import DownloadList from './components/DownloadList.vue'
import LinkGrabber from './components/LinkGrabber.vue'
import AppSettings from './components/AppSettings.vue'
import AccountSettings from './components/AccountSettings.vue'
import LogsView from './components/LogsView.vue'
import OnboardingTour from './components/OnboardingTour.vue'
import { setLocale, useI18n } from './i18n'
import { applyUiPreferences, useTheme, type ThemeId } from './themes'
import { pushRingBuffer } from './utils/ring-buffer'
import torIconSvg from './assets/tor.svg?raw'

type AppTab = 'downloads' | 'grabber' | 'settings' | 'account' | 'speed-history' | 'logs'

interface DownloadCompletePayload {
  id: string
  outputPath: string
  url?: string
  title?: string
  sha256Hash?: string
}

type TorStateName = 'disconnected' | 'connecting' | 'connected' | 'disconnecting'

interface TorRouteNode {
  role: string
  country: string
  code: string
}

const activeTab = ref<AppTab>('downloads')
const downloadCount = ref(0)
const skeletonCount = ref(0)
const skeletonBaseCount = ref(0)
const skeletonTargetCount = ref(0)
const speedHistory = ref<number[]>(new Array(600).fill(0))
const currentSpeed = ref(0)
const topSpeedCanvasRef = ref<HTMLCanvasElement | null>(null)
const speedHistoryCanvasRef = ref<HTMLCanvasElement | null>(null)
const clipboardIncomingUrl = ref('')
const showOnboarding = ref(false)
const helpMenuOpen = ref(false)
const torPanelOpen = ref(false)
const torBusy = ref(false)
const torError = ref('')
const torPulseIndex = ref(0)
const clipboardMonitorEnabled = ref(false)
const torState = ref<{
  state: TorStateName
  host: string
  port: number
  route: TorRouteNode[]
  ip?: string
  country?: string
  countryCode?: string
  isTor?: boolean
}>({
  state: 'disconnected',
  host: '127.0.0.1',
  port: 9150,
  route: [],
})
let currentSettings: Awaited<ReturnType<typeof window.api.settings.load>> | null = null
let speedTicker: ReturnType<typeof setInterval> | null = null
let torPulseTimer: ReturnType<typeof setInterval> | null = null
let disposeClipboardDetected: (() => void) | null = null
let appMounted = true

const torRouteNodes = computed(() => torState.value.route)
const torStatusLabel = computed(() => {
  if (torBusy.value && torState.value.state === 'connecting') return 'conectando'
  if (torBusy.value && torState.value.state === 'disconnecting') return 'desconectando'
  if (torState.value.state === 'connected') return 'conectado'
  return 'desconectado'
})
const torPowerLabel = computed(() => {
  if (torBusy.value && torState.value.state === 'connecting') return 'Conectando'
  if (torBusy.value && torState.value.state === 'disconnecting') return 'Desconectando'
  return torState.value.state === 'connected' ? 'Desconectar' : 'Conectar'
})
const torPanelTitle = computed(() =>
  torState.value.state === 'connected' ? 'Downloads via Tor ativos' : 'Rede Tor',
)
const torEndpointLabel = computed(() =>
  torState.value.state === 'connected'
    ? `${torState.value.host}:${torState.value.port}`
    : 'Clique para conectar',
)
const torExitLabel = computed(() => {
  if (torState.value.state !== 'connected') return 'Saída: indisponível'
  if (!torState.value.ip) return 'Saída: aguardando teste'
  return torState.value.country
    ? `Saída: ${torState.value.ip} - ${torState.value.country}`
    : `Saída: ${torState.value.ip}`
})
const torTestLabel = computed(() => {
  if (torState.value.state !== 'connected') return 'Teste: desconectado'
  if (torState.value.isTor === true) return 'Teste: tráfego via Tor'
  if (torState.value.isTor === false) return 'Teste: rota não confirmada'
  return 'Teste: pendente'
})

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
  if (torPulseTimer) clearInterval(torPulseTimer)
  disposeClipboardDetected?.()
  window.removeEventListener('stats-tick', statsTickHandler)
  disposeTheme()
})
const { t } = useI18n()
const { initTheme, disposeTheme, setTheme, themeOptions, effectiveTheme } = useTheme()

onMounted(async () => {
  initTheme()
  // Start speed ticker regardless of settings availability
  speedTicker = setInterval(() => {
    if (!appMounted) return
    speedHistory.value = pushRingBuffer(speedHistory.value, currentSpeed.value, 600)
    drawTopSpeedChart()
    drawSpeedHistoryChart()
  }, 120)
  torPulseTimer = setInterval(() => {
    if (!appMounted) return
    torPulseIndex.value = (torPulseIndex.value + 1) % 3
  }, 520)

  disposeClipboardDetected = window.api.clipboard.onLinkDetected((payload) => {
    if (!payload.url) return
    const urls = payload.urls?.length ? payload.urls : [payload.url]
    const nextValue = urls.join('\n')
    clipboardIncomingUrl.value = ''
    nextTick(() => {
      clipboardIncomingUrl.value = nextValue
    })
    activeTab.value = 'grabber'
    if (currentSettings?.nativeNotification) {
      const shortUrl = urls.length > 1
        ? `${urls.length} links capturados`
        : payload.url.length > 90 ? `${payload.url.slice(0, 87)}...` : payload.url
      void window.api.system.notify('Link capturado', shortUrl).catch(() => null)
    }
  })
  window.addEventListener('stats-tick', statsTickHandler)

  const settings = await window.api.settings.load().catch(() => null)
  if (!settings) return
  currentSettings = settings
  clipboardMonitorEnabled.value = Boolean(settings.clipboardMonitorEnabled)
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
  await refreshTorStatus()
})

watch(speedHistory, () => {
  void nextTick(drawTopSpeedChart)
  void nextTick(drawSpeedHistoryChart)
})

function formatSpeed(bps: number): string {
  if (bps >= 1024 * 1024) return `${(bps / (1024 * 1024)).toFixed(1)} MB/s`
  if (bps >= 1024) return `${(bps / 1024).toFixed(0)} KB/s`
  return `${bps} B/s`
}

function drawTopSpeedChart(): void {
  const canvas = topSpeedCanvasRef.value
  if (!canvas) return
  const ctx = canvas.getContext('2d')
  if (!ctx) return
  const width = canvas.width
  const height = canvas.height
  const values = speedHistory.value
  const max = Math.max(...values, 1)
  ctx.clearRect(0, 0, width, height)
  ctx.strokeStyle = '#5b6cff'
  ctx.lineWidth = 2
  ctx.beginPath()
  values.forEach((value, index) => {
    const x = values.length <= 1 ? 0 : (index / (values.length - 1)) * width
    const y = height - (value / max) * (height - 4) - 2
    if (index === 0) ctx.moveTo(x, y)
    else ctx.lineTo(x, y)
  })
  ctx.stroke()
}

function drawSpeedHistoryChart(): void {
  const canvas = speedHistoryCanvasRef.value
  if (!canvas) return
  const ctx = canvas.getContext('2d')
  if (!ctx) return
  const width = canvas.width
  const height = canvas.height
  const values = speedHistory.value
  const max = Math.max(...values, 1)
  ctx.clearRect(0, 0, width, height)
  ctx.strokeStyle = 'rgba(148, 163, 184, 0.28)'
  ctx.lineWidth = 1
  for (let i = 1; i < 5; i += 1) {
    const y = (height / 5) * i
    ctx.beginPath()
    ctx.moveTo(0, y)
    ctx.lineTo(width, y)
    ctx.stroke()
  }
  const gradient = ctx.createLinearGradient(0, 0, 0, height)
  gradient.addColorStop(0, 'rgba(91, 108, 255, 0.26)')
  gradient.addColorStop(1, 'rgba(91, 108, 255, 0.02)')
  ctx.beginPath()
  values.forEach((value, index) => {
    const x = values.length <= 1 ? 0 : (index / (values.length - 1)) * width
    const y = height - (value / max) * (height - 24) - 12
    if (index === 0) ctx.moveTo(x, y)
    else ctx.lineTo(x, y)
  })
  ctx.lineTo(width, height)
  ctx.lineTo(0, height)
  ctx.closePath()
  ctx.fillStyle = gradient
  ctx.fill()
  ctx.strokeStyle = '#5b6cff'
  ctx.lineWidth = 2.4
  ctx.beginPath()
  values.forEach((value, index) => {
    const x = values.length <= 1 ? 0 : (index / (values.length - 1)) * width
    const y = height - (value / max) * (height - 24) - 12
    if (index === 0) ctx.moveTo(x, y)
    else ctx.lineTo(x, y)
  })
  ctx.stroke()
}

async function toggleQuickTheme(): Promise<void> {
  const settings = currentSettings ?? await window.api.settings.load().catch(() => null)
  if (!settings) return
  const nextTheme: ThemeId = effectiveTheme.value === 'light' ? 'dark-default' : 'light'
  setTheme(nextTheme)
  currentSettings = await window.api.settings.save({ ...settings, theme: nextTheme }).catch(() => settings)
  window.dispatchEvent(new CustomEvent('gdownloader-settings-updated', { detail: currentSettings }))
}

async function toggleClipboardMonitor(): Promise<void> {
  const settings = currentSettings ?? await window.api.settings.load().catch(() => null)
  if (!settings) return
  const next = !clipboardMonitorEnabled.value
  clipboardMonitorEnabled.value = next
  currentSettings = await window.api.settings.save({ ...settings, clipboardMonitorEnabled: next }).catch(() => settings)
  window.dispatchEvent(new CustomEvent('gdownloader-settings-updated', { detail: currentSettings }))
}

function startOnboarding(): void {
  helpMenuOpen.value = false
  showOnboarding.value = true
}

function applyTorPayload(payload: {
  state: 'disconnected' | 'connected'
  host: string
  port: number
  route: TorRouteNode[]
  ip?: string
  country?: string
  countryCode?: string
  isTor?: boolean
}): void {
  torState.value = {
    state: payload.state,
    host: payload.host,
    port: payload.port,
    route: payload.route,
    ip: payload.ip,
    country: payload.country,
    countryCode: payload.countryCode,
    isTor: payload.isTor,
  }
}

async function refreshTorStatus(): Promise<void> {
  const payload = await window.api.tor.status().catch(() => null)
  if (payload) applyTorPayload(payload)
}

function toggleTorPanel(): void {
  torPanelOpen.value = !torPanelOpen.value
  if (torPanelOpen.value) void refreshTorStatus()
}

async function connectTor(): Promise<void> {
  torBusy.value = true
  torError.value = ''
  torState.value = { ...torState.value, state: 'connecting' }
  try {
    const payload = await withTimeout(
      window.api.tor.connect(),
      45_000,
      'Tempo esgotado conectando ao Tor. O processo foi iniciado, mas não completou a conexão com a rede.',
    )
    applyTorPayload(payload)
  } catch (error) {
    void window.api.tor.disconnect().catch(() => null)
    torState.value = { ...torState.value, state: 'disconnected', route: [] }
    torError.value = error instanceof Error ? error.message : String(error)
  } finally {
    torBusy.value = false
  }
}

async function disconnectTor(): Promise<void> {
  torBusy.value = true
  torError.value = ''
  torState.value = { ...torState.value, state: 'disconnecting' }
  try {
    const payload = await window.api.tor.disconnect()
    applyTorPayload(payload)
  } catch (error) {
    torError.value = error instanceof Error ? error.message : String(error)
  } finally {
    torBusy.value = false
  }
}

async function testTorConnection(): Promise<void> {
  torBusy.value = true
  torError.value = ''
  try {
    const payload = await withTimeout(
      window.api.tor.testConnection(),
      25_000,
      'Tempo esgotado testando a conexão Tor.',
    )
    applyTorPayload(payload)
  } catch (error) {
    torError.value = error instanceof Error ? error.message : String(error)
  } finally {
    torBusy.value = false
  }
}

async function newTorIdentity(): Promise<void> {
  torBusy.value = true
  torError.value = ''
  try {
    const payload = await withTimeout(
      window.api.tor.newIdentity(),
      35_000,
      'Tempo esgotado trocando o circuito Tor.',
    )
    applyTorPayload(payload)
  } catch (error) {
    torError.value = error instanceof Error ? error.message : String(error)
  } finally {
    torBusy.value = false
  }
}

function withTimeout<T>(promise: Promise<T>, timeoutMs: number, message: string): Promise<T> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error(message)), timeoutMs)
    promise
      .then((value) => {
        clearTimeout(timer)
        resolve(value)
      })
      .catch((error) => {
        clearTimeout(timer)
        reject(error)
      })
  })
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
  }
  if (typeof detail.total_speed_bps === 'number') currentSpeed.value = detail.total_speed_bps
}

function onAddingUrls(count: number): void {
  if (count <= 0) {
    skeletonCount.value = 0
    skeletonTargetCount.value = 0
    skeletonBaseCount.value = downloadCount.value
    return
  }
  skeletonBaseCount.value = downloadCount.value
  skeletonTargetCount.value = count
  skeletonCount.value = count
}

function onDownloadCountChange(count: number): void {
  downloadCount.value = count
  if (skeletonTargetCount.value > 0) {
    const appeared = Math.max(0, count - skeletonBaseCount.value)
    skeletonCount.value = Math.max(0, skeletonTargetCount.value - appeared)
    if (skeletonCount.value === 0) {
      skeletonTargetCount.value = 0
      skeletonBaseCount.value = count
    }
  }
  updateTrayStats()
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

.quick-toggle-btn,
.quick-icon-btn {
  height: 34px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 7px;
  border: 1px solid var(--border-color);
  border-radius: 9px;
  background: var(--bg-card);
  color: var(--text-primary);
  cursor: pointer;
  font-size: 12px;
  font-weight: 800;
}

.quick-toggle-btn {
  padding: 0 11px;
}

.quick-icon-btn {
  width: 36px;
}

.quick-toggle-btn.active {
  border-color: color-mix(in srgb, var(--accent-color) 48%, var(--border-color));
  background: color-mix(in srgb, var(--accent-color) 12%, var(--bg-card));
  color: var(--accent-color);
}

.top-speed {
  height: 34px;
  min-width: 204px;
  display: inline-flex;
  align-items: center;
  gap: 10px;
  padding: 0 10px;
  border: 1px solid var(--border-color);
  border-radius: 9px;
  background: var(--bg-card);
  color: var(--text-primary);
  font-size: 12px;
  font-weight: 800;
}

.top-speed-chart {
  width: 128px;
  height: 28px;
  flex: 0 0 128px;
}

.tor-widget {
  position: relative;
}

.tor-main-btn {
  height: 34px;
  min-width: 132px;
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding: 0 10px;
  border: 1px solid var(--border-color);
  border-radius: 9px;
  background: var(--bg-card);
  color: var(--text-primary);
  cursor: pointer;
  font-size: 12px;
}

.tor-main-btn strong {
  font-size: 12px;
}

.tor-main-btn em {
  color: var(--text-muted);
  font-style: normal;
  font-size: 11px;
}

.tor-widget.tor-connected .tor-main-btn {
  border-color: rgba(34, 197, 94, 0.45);
  background: color-mix(in srgb, #16a34a 12%, var(--bg-card));
}

.tor-widget.tor-connecting .tor-main-btn,
.tor-widget.tor-disconnecting .tor-main-btn {
  border-color: rgba(245, 158, 11, 0.55);
}

.tor-icon {
  display: inline-flex;
  width: 19px;
  height: 19px;
  color: #7d4698;
}

.tor-icon :deep(svg) {
  display: block;
  width: 19px;
  height: 19px;
}

.tor-widget.tor-connecting .tor-icon,
.tor-widget.tor-disconnecting .tor-icon {
  animation: tor-spin 0.95s linear infinite;
}

.tor-panel {
  position: absolute;
  right: 0;
  top: calc(100% + 8px);
  z-index: 30;
  width: 360px;
  padding: 14px;
  border: 1px solid var(--border-color);
  border-radius: 10px;
  background: var(--bg-card);
  box-shadow: 0 18px 54px rgba(0, 0, 0, 0.24);
}

.tor-panel-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.tor-panel-head div {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 3px;
}

.tor-panel-head strong {
  color: var(--text-primary);
  font-size: 13px;
}

.tor-panel-head span {
  color: var(--text-muted);
  font-size: 12px;
}

.tor-power-btn {
  height: 32px;
  display: inline-flex;
  align-items: center;
  gap: 7px;
  padding: 0 10px;
  border: 1px solid var(--border-color);
  border-radius: 8px;
  background: color-mix(in srgb, var(--accent-color) 10%, transparent);
  color: var(--accent-color);
  cursor: pointer;
  font-size: 12px;
  font-weight: 800;
}

.tor-power-btn:disabled,
.tor-main-btn:disabled {
  cursor: wait;
  opacity: 0.75;
}

.tor-route {
  position: relative;
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 8px;
  margin-top: 14px;
}

.tor-route::before {
  content: '';
  position: absolute;
  left: 17%;
  right: 17%;
  top: 19px;
  height: 2px;
  background: color-mix(in srgb, var(--accent-color) 32%, var(--border-color));
}

.tor-route.empty {
  grid-template-columns: 1fr;
}

.tor-route.empty::before {
  display: none;
}

.tor-node {
  position: relative;
  z-index: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 5px;
  padding: 4px;
  text-align: center;
}

.tor-node-dot {
  width: 38px;
  height: 38px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: 999px;
  border: 1px solid var(--border-color);
  background: var(--bg-secondary);
  color: var(--text-muted);
  font-size: 11px;
  font-weight: 900;
}

.tor-node.active .tor-node-dot {
  border-color: rgba(34, 197, 94, 0.45);
  background: rgba(34, 197, 94, 0.13);
  color: #16a34a;
}

.tor-node.pulse .tor-node-dot {
  animation: tor-pulse 0.9s ease-in-out infinite;
}

.tor-node strong {
  color: var(--text-primary);
  font-size: 11px;
}

.tor-node em {
  max-width: 100%;
  color: var(--text-muted);
  font-size: 11px;
  font-style: normal;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.tor-empty-state {
  min-height: 74px;
  display: flex;
  flex-direction: column;
  justify-content: center;
  gap: 4px;
  padding: 12px;
  border: 1px dashed var(--border-color);
  border-radius: 8px;
  color: var(--text-muted);
  text-align: center;
}

.tor-empty-state strong {
  color: var(--text-primary);
  font-size: 12px;
}

.tor-empty-state span {
  font-size: 11px;
}

.tor-panel-meta {
  display: grid;
  gap: 4px;
  margin-top: 12px;
  color: var(--text-muted);
  font-size: 11px;
}

.tor-test-result {
  width: fit-content;
  padding: 4px 8px;
  border-radius: 999px;
  background: color-mix(in srgb, var(--bg-card) 70%, transparent);
}

.tor-test-result.ok {
  color: #15803d;
  background: rgba(34, 197, 94, 0.14);
}

.tor-test-result.warn {
  color: #b45309;
  background: rgba(245, 158, 11, 0.14);
}

.tor-panel-actions {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 8px;
  margin-top: 12px;
}

.tor-panel-actions button {
  min-width: 0;
  height: 32px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  border: 1px solid var(--border-color);
  border-radius: 8px;
  background: var(--bg-secondary);
  color: var(--text-primary);
  font-size: 11px;
  font-weight: 800;
  cursor: pointer;
}

.tor-panel-actions button:disabled {
  cursor: not-allowed;
  opacity: 0.55;
}

.tor-error {
  margin: 10px 0 0;
  color: #dc2626;
  font-size: 12px;
  line-height: 1.35;
}

@keyframes tor-spin {
  to {
    transform: rotate(360deg);
  }
}

@keyframes tor-pulse {
  50% {
    transform: scale(1.08);
    box-shadow: 0 0 0 6px rgba(245, 158, 11, 0.14);
  }
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

.speed-history-panel {
  flex-direction: column;
  gap: 14px;
  padding: 18px;
  overflow: hidden;
}

.speed-history-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
}

.speed-history-header h2 {
  margin: 0;
  color: var(--text-primary);
  font-size: 18px;
}

.speed-history-header span {
  color: var(--text-muted);
  font-size: 12px;
}

.speed-history-header strong {
  color: var(--accent-color);
  font-size: 18px;
}

.speed-history-canvas {
  width: 100%;
  flex: 1;
  min-height: 260px;
  border: 1px solid var(--border-color);
  border-radius: 8px;
  background: var(--bg-card);
}
</style>
