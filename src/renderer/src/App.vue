<template>
  <div class="app-root">
    <aside class="sidebar">
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
        <div class="brand-text">
          <strong>{{ t('appName') }}</strong>
          <span class="brand-version">v2.0.0</span>
        </div>
      </div>
      <nav class="sidebar-nav" data-tour="tab-bar">
        <button
          class="nav-item"
          :class="{ active: activeTab === 'downloads' }"
          data-tour="downloads-tab"
          @click="activeTab = 'downloads'"
        >
          <i class="pi pi-download"></i>
          <span>{{ t('downloads') }}</span>
          <span v-if="downloadCount > 0" class="nav-badge">{{ downloadCount }}</span>
        </button>
        <button
          class="nav-item"
          :class="{ active: activeTab === 'grabber' }"
          data-tour="grabber-tab"
          @click="activeTab = 'grabber'"
        >
          <i class="pi pi-link"></i>
          <span>{{ t('linkGrabber') }}</span>
        </button>
        <button
          class="nav-item"
          :class="{ active: activeTab === 'account' }"
          data-tour="accounts-tab"
          @click="activeTab = 'account'"
        >
          <i class="pi pi-user"></i>
          <span>{{ t('account') }}</span>
        </button>
        <button
          class="nav-item"
          :class="{ active: activeTab === 'logs' }"
          data-tour="logs-tab"
          @click="activeTab = 'logs'"
        >
          <i class="pi pi-list"></i>
          <span>Logs</span>
        </button>
        <button
          class="nav-item"
          :class="{ active: activeTab === 'settings' }"
          data-tour="settings-tab"
          @click="activeTab = 'settings'"
        >
          <i class="pi pi-cog"></i>
          <span>{{ t('settings') }}</span>
        </button>
      </nav>

      <div v-if="diskUsage.total > 0" class="sidebar-disk-wrap">
        <button
          type="button"
          class="sidebar-disk"
          :class="{ warn: diskUsage.available < diskUsage.total * 0.1 || diskQueueOverflows }"
          :title="diskTooltip"
          @click="toggleDisksPopover"
        >
          <span class="sidebar-disk-title">Espaço em disco</span>
          <div class="sidebar-disk-stats">
            <div><strong>{{ formatBytes(diskUsage.used) }}</strong><span>{{ t('diskUsed') }}</span></div>
            <div><strong>{{ formatBytes(diskUsage.available) }}</strong><span>{{ t('diskFreeLabel') }}</span></div>
          </div>
          <div class="sidebar-disk-bar">
            <span class="seg-used" :style="{ width: `${diskUsedPercent}%` }"></span>
            <span
              class="seg-queued"
              :class="{ overflow: diskQueueOverflows }"
              :style="{ width: `${diskQueuedPercent}%` }"
            ></span>
          </div>
          <span class="sidebar-disk-total">Total: {{ formatBytes(diskUsage.total) }}</span>
        </button>

        <div v-if="disksPopoverOpen" class="disks-popover sidebar-disks-popover" @click.stop>
          <div class="disks-popover-head">
            <span>{{ t('disksAndVolumes') }}</span>
            <button class="disks-popover-close" @click="disksPopoverOpen = false"><i class="pi pi-times"></i></button>
          </div>
          <div class="disks-legend">
            <span><i class="dot used"></i>{{ t('diskUsed') }}</span>
            <span><i class="dot queued"></i>{{ t('diskQueued') }}</span>
            <span><i class="dot free"></i>{{ t('diskFreeLabel') }}</span>
          </div>
          <div v-if="allDisks.length === 0" class="disks-empty">{{ t('noDiskInfo') }}</div>
          <div
            v-for="disk in allDisks"
            :key="disk.mount"
            class="disk-row"
            :class="{ active: disk.mount === diskUsage.mount }"
          >
            <div class="disk-row-head">
              <i class="pi" :class="disk.removable ? 'pi-usb' : 'pi-database'"></i>
              <span class="disk-name" :title="disk.mount">{{ disk.name }}</span>
              <span class="disk-kind">{{ disk.kind }}{{ disk.removable ? ` · ${t('removableDisk')}` : '' }}</span>
            </div>
            <div class="disk-row-bar">
              <span class="seg-used" :style="{ width: `${diskPercent(disk.used, disk.total)}%` }"></span>
              <span
                v-if="disk.mount === diskUsage.mount && queuedBytes > 0"
                class="seg-queued"
                :style="{ width: `${diskPercent(Math.min(queuedBytes, disk.available), disk.total)}%` }"
              ></span>
            </div>
            <div class="disk-row-sub">
              <span>{{ formatBytes(disk.used) }} usados</span>
              <span>{{ formatBytes(disk.available) }} livres de {{ formatBytes(disk.total) }}</span>
            </div>
            <div class="disk-row-io">
              <span class="io-read" title="Leitura">
                <i class="pi pi-arrow-down"></i>{{ formatBytes(disk.readBps ?? 0) }}/s
              </span>
              <span class="io-write" title="Escrita">
                <i class="pi pi-arrow-up"></i>{{ formatBytes(disk.writeBps ?? 0) }}/s
              </span>
            </div>
          </div>
        </div>
      </div>
    </aside>
    <div class="app-body">
    <header class="topbar">
      <div class="topbar-actions">
        <button
          class="quick-toggle-btn"
          :class="{ active: clipboardMonitorEnabled }"
          :title="clipboardMonitorEnabled ? t('clipboardCaptureOn') : t('clipboardCaptureOff')"
          @click="toggleClipboardMonitor"
        >
          <i class="pi pi-link"></i>
          <span>{{ t('clipboardCapture') }}</span>
        </button>
        <button class="quick-icon-btn" :title="t('toggleTheme')" @click="toggleQuickTheme">
          <i :class="effectiveTheme === 'light' ? 'pi pi-moon' : 'pi pi-sun'"></i>
        </button>
        <div class="top-speed" :title="t('aggregateSpeed')">
          <canvas ref="topSpeedCanvasRef" class="top-speed-chart" width="128" height="28" aria-hidden="true"></canvas>
          <span>{{ formatSpeed(currentSpeed) }}</span>
        </div>
        <div class="tor-widget" :class="[`tor-${torState.state}`, { open: torPanelOpen }]" data-tour="tor-widget">
          <button class="tor-main-btn" :disabled="torBusy" @click="toggleTorPanel">
            <span
              class="tor-icon"
              :class="{ 'tor-icon-busy': torState.state === 'connecting' || torState.state === 'disconnecting' }"
              :style="{ '--tor-progress': `${torBootstrap}%` }"
              aria-hidden="true"
            >
              <span class="tor-icon-glyph" v-html="torIconSvg"></span>
            </span>
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
                <span class="tor-node-dot">
                  <span v-if="flagClass(node.code)" :class="flagClass(node.code)"></span>
                  <span v-else>{{ node.code }}</span>
                </span>
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


    <main class="app-main">
      <section v-show="activeTab === 'downloads'" class="panel downloads-panel" data-tour="download-queue">
        <DownloadList
          :skeleton-count="skeletonCount"
          :tor-active="torState.state === 'connected'"
          @count-change="onDownloadCountChange"
          @download-complete="onDownloadComplete"
          @global-speed="onGlobalSpeed"
          @queued-bytes="onQueuedBytes"
          @open-grabber="activeTab = 'grabber'"
          @tor-changed="refreshTorStatus"
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

      <section v-show="activeTab === 'logs'" class="panel" data-tour="logs-panel">
        <LogsView />
      </section>
    </main>
    <footer class="status-bar">
      <span class="status-item">
        <i class="status-dot" :class="{ on: currentSpeed > 0 }"></i>
        {{ t('aggregateSpeed') }}: ↓ {{ formatSpeed(currentSpeed) }}
      </span>
      <span class="status-item">
        <i class="status-dot" :class="{ on: torState.state === 'connected' }"></i>
        Proxy/Tor: {{ torState.state === 'connected' ? 'Ativo' : 'Inativo' }}
      </span>
    </footer>
    </div>
    <OnboardingTour
      v-if="showOnboarding"
      :active-tab="activeTab"
      @navigate="activeTab = $event"
      @complete="completeOnboarding"
    />
    <Toast position="bottom-right" />
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
import Toast from 'primevue/toast'
import { useToast } from 'primevue/usetoast'
import { setLocale, useI18n } from './i18n'
import { applyUiPreferences, useTheme, type ThemeId } from './themes'
import { pushRingBuffer } from './utils/ring-buffer'
import { flagClass } from './utils/flag'
import torIconSvg from './assets/tor.svg?raw'

type AppTab = 'downloads' | 'grabber' | 'settings' | 'account' | 'logs'

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
let skeletonSafetyTimer: ReturnType<typeof setTimeout> | null = null
const speedHistory = ref<number[]>(new Array(600).fill(0))
const currentSpeed = ref(0)
const topSpeedCanvasRef = ref<HTMLCanvasElement | null>(null)
const clipboardIncomingUrl = ref('')
const showOnboarding = ref(false)
const helpMenuOpen = ref(false)
const torPanelOpen = ref(false)
const torBusy = ref(false)
const torError = ref('')
const torPulseIndex = ref(0)
const torBootstrap = ref(0)
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
let diskTicker: ReturnType<typeof setInterval> | null = null
let disposeClipboardDetected: (() => void) | null = null
let disposeToastComplete: (() => void) | null = null
let disposeToastStatus: (() => void) | null = null
let appMounted = true

const torRouteNodes = computed(() => torState.value.route)
const torStatusLabel = computed(() => {
  if (torBusy.value && torState.value.state === 'connecting') {
    return torBootstrap.value > 0 ? `conectando ${torBootstrap.value}%` : 'conectando'
  }
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
  if (diskTicker) clearInterval(diskTicker)
  if (disksPollTimer) clearInterval(disksPollTimer)
  disposeClipboardDetected?.()
  disposeToastComplete?.()
  disposeToastStatus?.()
  window.removeEventListener('stats-tick', statsTickHandler)
  disposeTheme()
})
const { t } = useI18n()
const toast = useToast()

function fileNameFromPath(path?: string): string {
  if (!path) return ''
  return path.split(/[\\/]/).pop() ?? ''
}

// Toasts in-app para eventos relevantes (concluído, erro, captcha). Os demais
// status (baixando, pausado, etc.) não geram toast para não poluir.
function registerEventToasts(): void {
  disposeToastComplete = window.api.downloads.on('download:complete', (data: unknown) => {
    const ev = data as { path?: string }
    toast.add({
      severity: 'success',
      summary: t('toastDownloadCompleted'),
      detail: fileNameFromPath(ev.path),
      life: 4000,
    })
  })
  disposeToastStatus = window.api.downloads.on('download:status', (data: unknown) => {
    const ev = data as { status?: string; error?: string }
    if (ev.status === 'waiting_captcha') {
      toast.add({ severity: 'warn', summary: t('toastCaptchaNeeded'), life: 6000 })
    } else if (ev.status === 'error' || ev.status === 'corrupted' || ev.status === 'disk_full') {
      toast.add({
        severity: 'error',
        summary: t('toastDownloadFailed'),
        detail: ev.error ?? undefined,
        life: 6000,
      })
    }
  })
}
const { initTheme, disposeTheme, setTheme, themeOptions, effectiveTheme } = useTheme()

onMounted(async () => {
  initTheme()
  // Start speed ticker regardless of settings availability
  speedTicker = setInterval(() => {
    if (!appMounted) return
    speedHistory.value = pushRingBuffer(speedHistory.value, currentSpeed.value, 600)
    drawTopSpeedChart()
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
  registerEventToasts()

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
  void refreshDiskUsage()
  // Atualiza com frequência (statvfs é um único syscall, não pesa) para refletir
  // rápido o espaço consumido durante os downloads.
  diskTicker = setInterval(() => {
    if (appMounted) void refreshDiskUsage()
  }, 5_000)
  if (!settings.onboardingCompleted) {
    showOnboarding.value = true
  }
  await refreshTorStatus()
})

watch(speedHistory, () => {
  void nextTick(drawTopSpeedChart)
})

function formatSpeed(bps: number): string {
  if (bps >= 1024 * 1024) return `${(bps / (1024 * 1024)).toFixed(1)} MB/s`
  if (bps >= 1024) return `${(bps / 1024).toFixed(0)} KB/s`
  return `${bps} B/s`
}

// Widget de disco: total/usado do volume da pasta de download.
const diskUsage = ref<{ total: number; available: number; used: number; mount: string }>({
  total: 0,
  available: 0,
  used: 0,
  mount: '',
})
// Bytes que ainda serão gravados pelos downloads na fila (segmento amarelo).
const queuedBytes = ref(0)

// Multi-disco: balão com todos os discos/volumes ao clicar no widget.
const disksPopoverOpen = ref(false)
const allDisks = ref<
  Array<{
    name: string
    mount: string
    total: number
    available: number
    used: number
    removable: boolean
    kind: string
    readBps?: number
    writeBps?: number
  }>
>([])

function diskPercent(part: number, total: number): number {
  return total > 0 ? Math.min(100, (part / total) * 100) : 0
}

let disksPollTimer: ReturnType<typeof setInterval> | null = null
async function toggleDisksPopover(): Promise<void> {
  disksPopoverOpen.value = !disksPopoverOpen.value
  if (disksPopoverOpen.value) {
    allDisks.value = await window.api.getAllDisks().catch(() => [])
    // Atualiza o I/O ao vivo (leitura/escrita por disco) enquanto o painel está aberto.
    if (disksPollTimer) clearInterval(disksPollTimer)
    disksPollTimer = setInterval(async () => {
      if (!disksPopoverOpen.value) return
      allDisks.value = await window.api.getAllDisks().catch(() => allDisks.value)
    }, 1500)
  } else if (disksPollTimer) {
    clearInterval(disksPollTimer)
    disksPollTimer = null
  }
}

const diskUsedPercent = computed(() =>
  diskUsage.value.total > 0
    ? Math.min(100, (diskUsage.value.used / diskUsage.value.total) * 100)
    : 0,
)
// Segmento amarelo: espaço que a fila vai ocupar, limitado ao que ainda está livre.
const diskQueuedPercent = computed(() => {
  if (diskUsage.value.total <= 0) return 0
  const willUse = Math.min(queuedBytes.value, diskUsage.value.available)
  return Math.min(100 - diskUsedPercent.value, (willUse / diskUsage.value.total) * 100)
})
// A fila não cabe no espaço livre? (barra amarela fica vermelha + aviso.)
const diskQueueOverflows = computed(
  () => diskUsage.value.total > 0 && queuedBytes.value > diskUsage.value.available,
)

const diskTooltip = computed(() => {
  const freeOf = t('diskFreeOf', {
    available: formatBytes(diskUsage.value.available),
    total: formatBytes(diskUsage.value.total),
  })
  return `Disco ${diskUsage.value.mount}: ${formatBytes(diskUsage.value.used)} ${t('diskUsed').toLowerCase()} · ${formatBytes(queuedBytes.value)} · ${freeOf}${diskQueueOverflows.value ? t('diskQueueOverflow') : ''}${t('diskClickAll')}`
})

function onQueuedBytes(bytes: number): void {
  queuedBytes.value = Math.max(0, bytes)
}

function formatBytes(bytes: number): string {
  if (bytes >= 1024 ** 4) return `${(bytes / 1024 ** 4).toFixed(1)} TB`
  if (bytes >= 1024 ** 3) return `${(bytes / 1024 ** 3).toFixed(1)} GB`
  if (bytes >= 1024 ** 2) return `${(bytes / 1024 ** 2).toFixed(0)} MB`
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(0)} KB`
  return `${bytes} B`
}

async function refreshDiskUsage(): Promise<void> {
  const dir = currentSettings?.outputDir || undefined
  const usage = await window.api.getDiskUsage(dir).catch(() => null)
  if (usage) diskUsage.value = usage
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
  torBootstrap.value = 0
  torState.value = { ...torState.value, state: 'connecting' }
  // Acompanha o bootstrap real do Tor para a animação mostrar a porcentagem.
  const bootstrapPoll = setInterval(() => {
    void window.api.tor
      .bootstrapProgress()
      .then((percent) => {
        if (typeof percent === 'number') torBootstrap.value = Math.max(torBootstrap.value, percent)
      })
      .catch(() => null)
  }, 600)
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
    clearInterval(bootstrapPoll)
    torBootstrap.value = 0
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
  if (skeletonSafetyTimer) {
    clearTimeout(skeletonSafetyTimer)
    skeletonSafetyTimer = null
  }
  if (count <= 0) {
    skeletonCount.value = 0
    skeletonTargetCount.value = 0
    skeletonBaseCount.value = downloadCount.value
    return
  }
  skeletonBaseCount.value = downloadCount.value
  skeletonTargetCount.value = count
  skeletonCount.value = count
  // Rede de segurança: se por qualquer motivo o skeleton não zerar (duplicados,
  // erro, contagem que não sobe), força limpar após um tempo — nada de skeleton eterno.
  skeletonSafetyTimer = setTimeout(() => {
    skeletonCount.value = 0
    skeletonTargetCount.value = 0
    skeletonBaseCount.value = downloadCount.value
    skeletonSafetyTimer = null
  }, 30_000)
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
  flex-direction: row;
  background: var(--bg-primary);
  color: var(--text-primary);
  overflow: hidden;
}

/* ── Sidebar (navegação lateral) ─────────────────────────────── */
.sidebar {
  width: 244px;
  flex: 0 0 244px;
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 16px 12px;
  background: var(--bg-secondary);
  border-right: 1px solid var(--border-color);
  overflow-y: auto;
}

.app-body {
  flex: 1;
  min-width: 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.topbar {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  padding: 12px 18px;
  border-bottom: 1px solid var(--border-color);
  background: var(--bg-secondary);
}

.status-bar {
  display: flex;
  align-items: center;
  gap: 22px;
  flex: 0 0 auto;
  padding: 7px 18px;
  border-top: 1px solid var(--border-color);
  background: var(--bg-secondary);
  font-size: 12px;
  color: var(--text-muted);
}

.status-item {
  display: inline-flex;
  align-items: center;
  gap: 7px;
}

.status-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--text-muted);
  opacity: 0.5;
}

.status-dot.on {
  background: #22c55e;
  opacity: 1;
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

.top-disk {
  height: 34px;
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding: 0 10px;
  border: 1px solid var(--border-color);
  border-radius: 9px;
  background: var(--bg-card);
  color: var(--text-secondary);
  font-size: 11px;
}

.top-disk > i {
  font-size: 13px;
  color: var(--text-secondary);
}

.top-disk.warn > i,
.top-disk.warn .top-disk-label {
  color: #dc2626;
}

.top-disk-info {
  display: flex;
  flex-direction: column;
  gap: 3px;
  min-width: 92px;
}

.top-disk-bar {
  height: 4px;
  border-radius: 3px;
  background: color-mix(in srgb, var(--border-color) 60%, transparent);
  overflow: hidden;
  display: flex;
}

/* Azul = usado, Amarelo = a fila vai ocupar, Branco (fundo) = livre depois. */
.top-disk-bar .seg-used {
  height: 100%;
  background: var(--accent-color);
  transition: width 0.4s ease;
}

.top-disk-bar .seg-queued {
  height: 100%;
  background: #f5b301;
  transition: width 0.4s ease;
}

.top-disk-bar .seg-queued.overflow {
  background: #dc2626;
}

.top-disk.warn .top-disk-bar .seg-used {
  background: #dc2626;
}

.top-disk-label {
  font-weight: 700;
  color: var(--text-primary);
  line-height: 1;
}

.top-disk-wrap {
  position: relative;
}

.top-disk {
  cursor: pointer;
  font: inherit;
}

.sidebar-disk-wrap {
  margin-top: auto;
  position: relative;
  padding-top: 12px;
}

.sidebar-disk {
  display: flex;
  flex-direction: column;
  gap: 9px;
  width: 100%;
  padding: 12px;
  border: 1px solid var(--border-color);
  border-radius: 12px;
  background: var(--bg-primary);
  cursor: pointer;
  text-align: left;
}

.sidebar-disk.warn {
  border-color: #e0a800;
}

.sidebar-disk-title {
  font-size: 10.5px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.6px;
  color: var(--text-muted);
}

.sidebar-disk-stats {
  display: flex;
  justify-content: space-between;
  gap: 8px;
}

.sidebar-disk-stats > div {
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.sidebar-disk-stats strong {
  font-size: 13px;
  color: var(--text-primary);
}

.sidebar-disk-stats span {
  font-size: 11px;
  color: var(--text-muted);
}

.sidebar-disk-bar {
  height: 7px;
  border-radius: 999px;
  background: color-mix(in srgb, var(--text-primary) 9%, transparent);
  overflow: hidden;
  display: flex;
}

.sidebar-disk-bar .seg-used {
  height: 100%;
  background: var(--accent-color);
}

.sidebar-disk-bar .seg-queued {
  height: 100%;
  background: #f5b301;
}

.sidebar-disk-bar .seg-queued.overflow {
  background: #dc2626;
}

.sidebar-disk-total {
  font-size: 11px;
  color: var(--text-muted);
}

.sidebar-disks-popover {
  top: auto;
  right: 0;
  left: 0;
  bottom: calc(100% + 8px);
  width: auto;
}

.disks-popover {
  position: absolute;
  top: calc(100% + 8px);
  right: 0;
  z-index: 50;
  width: 340px;
  max-height: 60vh;
  overflow-y: auto;
  padding: 12px;
  border-radius: 12px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  box-shadow: 0 20px 48px rgba(15, 23, 42, 0.28);
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.disks-popover-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-weight: 700;
  font-size: 13px;
  color: var(--text-primary);
}

.disks-popover-close {
  border: none;
  background: transparent;
  color: var(--text-secondary);
  cursor: pointer;
  font-size: 12px;
}

.disks-legend {
  display: flex;
  gap: 12px;
  font-size: 11px;
  color: var(--text-secondary);
}

.disks-legend span {
  display: inline-flex;
  align-items: center;
  gap: 4px;
}

.disks-legend .dot {
  width: 9px;
  height: 9px;
  border-radius: 3px;
  display: inline-block;
}

.disks-legend .dot.used {
  background: var(--accent-color);
}
.disks-legend .dot.queued {
  background: #f5b301;
}
.disks-legend .dot.free {
  background: color-mix(in srgb, var(--border-color) 60%, transparent);
}

.disks-empty {
  font-size: 12px;
  color: var(--text-secondary);
  padding: 6px 0;
}

.disk-row {
  display: flex;
  flex-direction: column;
  gap: 5px;
  padding: 8px;
  border-radius: 9px;
  border: 1px solid transparent;
}

.disk-row.active {
  border-color: color-mix(in srgb, var(--accent-color) 40%, transparent);
  background: color-mix(in srgb, var(--accent-color) 8%, transparent);
}

.disk-row-head {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
}

.disk-row-head .disk-name {
  font-weight: 700;
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  flex: 1;
}

.disk-row-head .disk-kind {
  font-size: 10px;
  color: var(--text-secondary);
  flex-shrink: 0;
}

.disk-row-bar {
  height: 6px;
  border-radius: 4px;
  background: color-mix(in srgb, var(--border-color) 60%, transparent);
  overflow: hidden;
  display: flex;
}

.disk-row-bar .seg-used {
  height: 100%;
  background: var(--accent-color);
}
.disk-row-bar .seg-queued {
  height: 100%;
  background: #f5b301;
}

.disk-row-sub {
  display: flex;
  justify-content: space-between;
  font-size: 11px;
  color: var(--text-secondary);
}

.disk-row-io {
  display: flex;
  gap: 12px;
  margin-top: 3px;
  font-size: 11px;
  font-variant-numeric: tabular-nums;
}

.disk-row-io .io-read {
  color: #2e9b59;
}

.disk-row-io .io-write {
  color: #d9822b;
}

.disk-row-io i {
  font-size: 10px;
  margin-right: 2px;
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
  position: relative;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  color: #7d4698;
}

.tor-icon-glyph {
  display: inline-flex;
  width: 19px;
  height: 19px;
}

.tor-icon :deep(svg) {
  display: block;
  width: 19px;
  height: 19px;
}

/* Animação de conexão: a cebola NÃO gira mais (ficava esquisito tombando).
   Agora ela "respira" suavemente enquanto um anel de progresso real (movido
   pelo bootstrap do Tor) preenche ao redor dela. */
.tor-icon-busy .tor-icon-glyph {
  animation: tor-breathe 1.6s ease-in-out infinite;
}

/* Trilho do anel (faint). */
.tor-icon-busy::before {
  content: '';
  position: absolute;
  inset: -3px;
  border-radius: 50%;
  border: 2px solid color-mix(in srgb, var(--accent-color, #f59e0b) 22%, transparent);
}

/* Preenchimento do anel até a porcentagem do bootstrap. */
.tor-icon-busy::after {
  content: '';
  position: absolute;
  inset: -3px;
  border-radius: 50%;
  background: conic-gradient(
    var(--accent-color, #f59e0b) var(--tor-progress, 0%),
    transparent 0
  );
  -webkit-mask: radial-gradient(farthest-side, transparent calc(100% - 2px), #000 calc(100% - 2px));
  mask: radial-gradient(farthest-side, transparent calc(100% - 2px), #000 calc(100% - 2px));
  transition: background 0.4s ease;
  animation: tor-ring-sheen 1.4s ease-in-out infinite;
}

@keyframes tor-breathe {
  0%,
  100% {
    transform: scale(1);
    opacity: 0.85;
  }
  50% {
    transform: scale(1.12);
    opacity: 1;
  }
}

@keyframes tor-ring-sheen {
  0%,
  100% {
    opacity: 0.7;
  }
  50% {
    opacity: 1;
  }
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
  transition: transform 0.25s ease, opacity 0.25s ease;
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
  transition: transform 0.25s ease, opacity 0.25s ease;
  overflow: hidden;
}

.tor-node.active .tor-node-dot {
  border-color: rgba(34, 197, 94, 0.45);
  background: rgba(34, 197, 94, 0.13);
  color: #16a34a;
}

.tor-node.pulse .tor-node-dot {
  animation: tor-pulse 1.2s ease-in-out infinite;
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

@keyframes tor-pulse {
  0%,
  100% {
    transform: scale(1);
    opacity: 0.6;
  }
  50% {
    transform: scale(1.08);
    opacity: 1;
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
  padding: 6px 8px 14px;
}

.brand-text {
  display: flex;
  align-items: baseline;
  gap: 7px;
}

.brand-version {
  font-size: 11px;
  font-weight: 500;
  color: var(--text-muted);
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

.sidebar-nav {
  display: flex;
  flex-direction: column;
  gap: 3px;
}

.nav-item {
  display: flex;
  align-items: center;
  gap: 11px;
  width: 100%;
  padding: 10px 12px;
  border: none;
  border-radius: 9px;
  background: transparent;
  color: var(--text-muted);
  cursor: pointer;
  font-size: 14px;
  text-align: left;
  transition:
    color 0.15s ease,
    background 0.15s ease;
}

.nav-item i {
  font-size: 16px;
  width: 18px;
  text-align: center;
}

.nav-item:hover {
  color: var(--text-primary);
  background: color-mix(in srgb, var(--text-primary) 6%, transparent);
}

.nav-item.active {
  color: var(--accent-color);
  background: color-mix(in srgb, var(--accent-color) 12%, transparent);
  font-weight: 600;
}

.nav-badge {
  min-width: 20px;
  height: 20px;
  margin-left: auto;
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
