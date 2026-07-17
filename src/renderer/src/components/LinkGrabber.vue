<template>
  <div class="link-grabber">
    <LinkInputPanel
      v-model="urlsInput"
      @resize="autoResize"
      @imported-links="appendImportedLinks"
      @imported-hashes="appendImportedHashes"
      @import-error="showImportError"
    />

    <div v-if="rows.length > 0" class="global-destination">
      <div>
        <span>Pasta para novos downloads</span>
        <strong :title="globalDestDir">{{ globalDestDir || defaultOutputDir() }}</strong>
      </div>
      <div class="global-destination-actions">
        <button type="button" class="destination-btn" @click="chooseGlobalDestination">
          Escolher pasta
        </button>
        <button type="button" class="destination-btn" @click="applyGlobalDestinationToRows">
          Aplicar a todos
        </button>
      </div>
    </div>

    <CapturedResultsPanel
      v-if="rows.length > 0"
      :rows="rows"
      :all-selectable-checked="allSelectableChecked"
      :some-selectable-checked="someSelectableChecked"
      :selected-count="selectedCount"
      :available-count="availableCount"
      :online-count="onlineCount"
      :offline-count="offlineCount"
      :folder-count="folderCount"
      :file-count="fileCount"
      :duplicate-count="duplicateCount"
      :error-count="errorCount"
      :active-mirror-row-url="activeMirrorTarget?.rowUrl ?? ''"
      :is-row-checked="isRowChecked"
      :is-row-indeterminate="isRowIndeterminate"
      :row-selectable-unit-count="rowSelectableUnitCount"
      :supports-child-selection="supportsChildSelection"
      :child-nodes="childNodes"
      :folder-node-selectable-count="folderNodeSelectableCount"
      :folder-node-selected-count="folderNodeSelectedCount"
      :is-folder-node-checked="isFolderNodeChecked"
      :is-folder-node-indeterminate="isFolderNodeIndeterminate"
      :can-search-mirrors="canSearchMirrors"
      :fmt-bytes="fmtBytes"
      :truncate-url="truncateUrl"
      @toggle-all="toggleAllChecked"
      @toggle-row="toggleRowChecked"
      @toggle-child="toggleChildChecked"
      @toggle-folder-node="toggleFolderNodeChecked"
      @set-row-selection="setRowSelectionChecked"
      @select-youtube-format="selectYouTubeFormat"
      @update-youtube-option="updateYouTubeOption"
      @toggle-expanded="toggleExpanded"
      @open-mirrors="openMirrors"
      @choose-destination="chooseRowDestination"
      @rename-row="onRenameRow"
      @filtered-change="visibleFilteredUrls = new Set($event)"
    />

    <p v-if="capacityShortfall > 0" class="capacity-warn">
      <i class="pi pi-exclamation-triangle"></i>
      Os downloads selecionados somam {{ fmtBytes(totalSelectedBytes) }} e podem não caber no disco — faltam ~{{ fmtBytes(capacityShortfall) }} de espaço livre.
    </p>

    <p v-if="lastError" class="error-msg">
      <i class="pi pi-exclamation-triangle"></i>
      {{ lastError }}
    </p>

    <MirrorSearchModal
      v-if="activeMirrorTarget"
      :filename="activeMirrorTarget.filename"
      :searching="mirrorsSearching"
      :current-searcher="mirrorCurrentSearcher"
      :phase-label="mirrorPhaseLabel"
      :progress-text="mirrorProgressText"
      :progress-percent="mirrorProgressPercent"
      :hoster-text="mirrorHosterText"
      :elapsed-label="mirrorElapsedLabel"
      :timing-label="mirrorTimingLabel"
      :progress-headline="mirrorProgressHeadline"
      :results="mirrorResults"
      :log="mirrorsLog"
      @close="closeMirrors"
      @search="searchMirrors"
      @copy-all="copyMirrorResults"
      @copy-one="copyMirrorResult"
    />

    <div ref="actionsRef">
      <LinkGrabberActionsBar
        :disable-clear="!urlsInput.trim() && rows.length === 0"
        :disable-add="selectedCount === 0 || adding"
        :adding="adding"
        :selected-count="selectedCount"
        :add-button-label="addButtonLabel"
        @clear="clear"
        @add="addAll"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import type { AppSettingsSnapshot, ExpectedHash, FileInfo } from '../../../shared/types'
import { useI18n } from '../i18n'
import { buildChildTree, flattenChildTree, type DerivedChildNode } from '../utils/child-tree'
import { formatBytes, formatDuration } from '../utils/format'
import { parseUrls as parseCapturedUrls, truncateUrl as shortenUrl } from '../utils/link-grabber'
import { pruneCapturedRows } from '../utils/capture-selection'
import CapturedResultsPanel from './CapturedResultsPanel.vue'
import LinkInputPanel from './LinkInputPanel.vue'
import LinkGrabberActionsBar from './LinkGrabberActionsBar.vue'
import MirrorSearchModal from './MirrorSearchModal.vue'
import type { CapturedRow, MirrorViewResult, ModuleSummary, RowFileInfo, SelectableChild } from './link-grabber-model'

type MirrorSearchPhase = 'idle' | 'starting' | 'running' | 'completed' | 'done' | 'error'

interface MirrorSearchState {
  filename: string
  totalSearchers: number
  current: number
  currentSearcher: string
  phase: MirrorSearchPhase
  totalResults: number
  newResults: number
  hosters: number
  durationMs: number
  startedAt: number
}

interface ActiveMirrorTarget {
  rowUrl: string
  filename: string
}

interface StoredMirrorSession {
  searching: boolean
  log: string[]
  results: MirrorViewResult[]
  state: MirrorSearchState
}

const props = defineProps<{
  incomingUrl?: string
}>()

const emit = defineEmits<{
  (e: 'added'): void
  (e: 'adding-urls', count: number): void
}>()
const { t } = useI18n()

const urlsInput = ref('')
const rows = ref<CapturedRow[]>([])
const visibleFilteredUrls = ref<Set<string> | null>(null)
const adding = ref(false)
const lastError = ref('')
const detectToken = ref(0)
const selectAllIntent = ref(true)
const actionsRef = ref<HTMLElement | null>(null)
const addQueueDone = ref(0)
const addQueueTotal = ref(0)
const expectedHashesByFilename = ref<Record<string, ExpectedHash>>({})
const currentSettings = ref<AppSettingsSnapshot | null>(null)
const globalDestDir = ref('')
let detectTimer: number | null = null
const childNodeCache = new WeakMap<SelectableChild[], DerivedChildNode<SelectableChild>[]>()
const selectableChildrenCache = new WeakMap<SelectableChild[], SelectableChild[]>()
const nodeSelectableChildrenCache = new WeakMap<DerivedChildNode<SelectableChild>, SelectableChild[]>()

// ── Mirrors ───────────────────────────────────────────────────────────────
const mirrorsSearching = ref(false)
const mirrorsLog = ref<string[]>([])
const mirrorResults = ref<MirrorViewResult[]>([])
const mirrorNow = ref(Date.now())
const activeMirrorTarget = ref<ActiveMirrorTarget | null>(null)
const mirrorSessions = ref<Record<string, StoredMirrorSession>>({})
const mirrorState = ref<MirrorSearchState>({
  filename: '',
  totalSearchers: 0,
  current: 0,
  currentSearcher: '',
  phase: 'idle',
  totalResults: 0,
  newResults: 0,
  hosters: 0,
  durationMs: 0,
  startedAt: 0,
})

let mirrorsCleanup: (() => void) | null = null
let mirrorsTicker: number | null = null

const mirrorProgressPercent = computed(() => {
  const total = mirrorState.value.totalSearchers
  if (total <= 0) return 0
  return Math.min(100, Math.round((mirrorState.value.current / total) * 100))
})

const mirrorProgressText = computed(() => {
  const total = mirrorState.value.totalSearchers
  if (total <= 0) return '0/0'
  return `${Math.min(mirrorState.value.current, total)}/${total}`
})

const mirrorCurrentSearcher = computed(() => {
  if (mirrorState.value.currentSearcher) return mirrorState.value.currentSearcher
  return mirrorsSearching.value ? t('mirrorsPreparing') : t('mirrorsWaiting')
})

const mirrorPhaseLabel = computed(() => {
  switch (mirrorState.value.phase) {
    case 'starting':
      return t('mirrorsPhaseStarting')
    case 'running':
      return t('mirrorsPhaseRunning')
    case 'completed':
      return mirrorState.value.newResults > 0
        ? `+${mirrorState.value.newResults} ${t('mirrorsPhaseRoundResults')}`
        : t('mirrorsPhaseRoundDone')
    case 'done':
      return t('mirrorsPhaseDone')
    case 'error':
      return t('mirrorsPhaseInterrupted')
    default:
      return t('mirrorsReady')
  }
})

const mirrorDurationMs = computed(() => {
  if (mirrorsSearching.value && mirrorState.value.startedAt > 0) {
    return Math.max(0, mirrorNow.value - mirrorState.value.startedAt)
  }
  return mirrorState.value.durationMs
})

const mirrorElapsedLabel = computed(() => formatDuration(mirrorDurationMs.value))

const mirrorTimingLabel = computed(() => {
  if (mirrorsSearching.value) return t('mirrorsLiveUpdating')
  if (mirrorState.value.durationMs > 0) return t('mirrorsLastSearch')
  return t('mirrorsWaitingSearch')
})

const mirrorHosterText = computed(() => {
  if (mirrorState.value.hosters > 0 || mirrorState.value.phase === 'done') {
    return `${mirrorState.value.hosters} hoster(s)`
  }
  return `${mirrorState.value.totalResults} encontrado(s)`
})

const mirrorProgressHeadline = computed(() => {
  if (mirrorState.value.currentSearcher) {
    return `${t('mirrorsSearchingIn')} ${mirrorState.value.currentSearcher}`
  }
  return mirrorsSearching.value ? t('mirrorsPreparingScan') : t('mirrorsReady')
})

function startMirrorsTicker(): void {
  stopMirrorsTicker()
  mirrorNow.value = Date.now()
  mirrorsTicker = window.setInterval(() => {
    mirrorNow.value = Date.now()
  }, 400)
}

function stopMirrorsTicker(): void {
  if (mirrorsTicker !== null) {
    window.clearInterval(mirrorsTicker)
    mirrorsTicker = null
  }
}

function currentMirrorSession(): StoredMirrorSession {
  return {
    searching: mirrorsSearching.value,
    log: [...mirrorsLog.value],
    results: [...mirrorResults.value],
    state: { ...mirrorState.value },
  }
}

function restoreMirrorSession(target: ActiveMirrorTarget): void {
  const session = mirrorSessions.value[target.rowUrl]
  if (!session) {
    mirrorsSearching.value = false
    mirrorsLog.value = []
    mirrorResults.value = []
    mirrorState.value = {
      filename: target.filename,
      totalSearchers: 0,
      current: 0,
      currentSearcher: '',
      phase: 'idle',
      totalResults: 0,
      newResults: 0,
      hosters: 0,
      durationMs: 0,
      startedAt: 0,
    }
    stopMirrorsTicker()
    return
  }

  mirrorsSearching.value = session.searching
  mirrorsLog.value = [...session.log]
  mirrorResults.value = [...session.results]
  mirrorState.value = { ...session.state }
  if (session.searching) {
    startMirrorsTicker()
  } else {
    stopMirrorsTicker()
  }
}

function persistMirrorSession(): void {
  const target = activeMirrorTarget.value
  if (!target) return
  mirrorSessions.value = {
    ...mirrorSessions.value,
    [target.rowUrl]: currentMirrorSession(),
  }
}

function scrollMirrorsLog(): void {
  nextTick(() => undefined)
}

function resetMirrorSearch(): void {
  window.api.mirrors.abort()
  mirrorsCleanup?.()
  mirrorsCleanup = null
  stopMirrorsTicker()
  mirrorsSearching.value = false
  mirrorsLog.value = []
  mirrorResults.value = []
  mirrorState.value = {
    filename: activeMirrorTarget.value?.filename ?? '',
    totalSearchers: 0,
    current: 0,
    currentSearcher: '',
    phase: 'idle',
    totalResults: 0,
    newResults: 0,
    hosters: 0,
    durationMs: 0,
    startedAt: 0,
  }
  persistMirrorSession()
}

function finishMirrorSearch(phase: MirrorSearchPhase): void {
  mirrorsSearching.value = false
  mirrorState.value.phase = phase
  stopMirrorsTicker()
  mirrorsCleanup?.()
  mirrorsCleanup = null
  persistMirrorSession()
}

function pushMirrorLog(message: string): void {
  mirrorsLog.value.push(message)
  persistMirrorSession()
  scrollMirrorsLog()
}

function pushMirrorResult(result: MirrorViewResult): void {
  if (mirrorResults.value.some((item) => item.url === result.url)) {
    return
  }
  mirrorResults.value = [...mirrorResults.value, result].sort((a, b) => {
    if (b.score !== a.score) return b.score - a.score
    return a.url.localeCompare(b.url)
  })
  persistMirrorSession()
}

function canSearchMirrors(row: CapturedRow): boolean {
  return row.module?.id !== 'youtube' && !!row.info?.name && !row.info.isFolder && !row.loading && !row.error
}

function openMirrors(row: CapturedRow): void {
  if (!canSearchMirrors(row) || !row.info?.name) {
    return
  }

  const nextTarget: ActiveMirrorTarget = {
    rowUrl: row.url,
    filename: row.info.name,
  }
  const changedTarget = activeMirrorTarget.value?.rowUrl !== nextTarget.rowUrl
    || activeMirrorTarget.value?.filename !== nextTarget.filename

  if (changedTarget) {
    persistMirrorSession()
    window.api.mirrors.abort()
    mirrorsCleanup?.()
    mirrorsCleanup = null
    stopMirrorsTicker()
  }

  activeMirrorTarget.value = nextTarget
  restoreMirrorSession(nextTarget)
}

function closeMirrors(): void {
  persistMirrorSession()
  window.api.mirrors.abort()
  mirrorsCleanup?.()
  mirrorsCleanup = null
  stopMirrorsTicker()
  activeMirrorTarget.value = null
}

async function searchMirrors(): Promise<void> {
  const filename = activeMirrorTarget.value?.filename
  if (!filename || mirrorsSearching.value) return

  resetMirrorSearch()
  mirrorsSearching.value = true
  mirrorState.value = {
    filename,
    totalSearchers: 0,
    current: 0,
    currentSearcher: '',
    phase: 'starting',
    totalResults: 0,
    newResults: 0,
    hosters: 0,
    durationMs: 0,
    startedAt: Date.now(),
  }
  startMirrorsTicker()

  mirrorsCleanup = window.api.mirrors.onEvent((event) => {
    if (event.type === 'start') {
      mirrorState.value.filename = event.payload.filename || filename
      mirrorState.value.totalSearchers = event.payload.total
      mirrorState.value.phase = 'starting'
    } else if (event.type === 'progress') {
      mirrorState.value.current = event.payload.current
      mirrorState.value.totalSearchers = event.payload.total
      mirrorState.value.currentSearcher = event.payload.searcher
      mirrorState.value.phase = event.payload.phase === 'completed' ? 'completed' : 'running'
      mirrorState.value.newResults = event.payload.newResults
      mirrorState.value.totalResults = event.payload.totalResults
    } else if (event.type === 'log') {
      pushMirrorLog(event.payload)
    } else if (event.type === 'result') {
      pushMirrorResult(event.payload)
      mirrorState.value.totalResults = Math.max(mirrorState.value.totalResults, mirrorResults.value.length)
    } else if (event.type === 'done') {
      mirrorState.value.filename = event.payload.filename || filename
      mirrorState.value.totalSearchers = event.payload.searchers
      mirrorState.value.current = event.payload.searchers
      mirrorState.value.currentSearcher = t('mirrorsDoneLabel')
      mirrorState.value.totalResults = event.payload.total
      mirrorState.value.hosters = event.payload.hosters
      mirrorState.value.durationMs = event.payload.durationMs
      finishMirrorSearch('done')
    } else if (event.type === 'error') {
      pushMirrorLog(`[erro] ${event.payload}`)
      finishMirrorSearch('error')
    }
  })

  await window.api.mirrors.search(filename).catch((err: Error) => {
    pushMirrorLog(`[erro] ${err.message}`)
    finishMirrorSearch('error')
  })
}

async function copyMirrorResults(): Promise<void> {
  await window.api.clipboard.writeText(mirrorResults.value.map((result) => result.url).join('\n'))
}

async function copyMirrorResult(url: string): Promise<void> {
  await window.api.clipboard.writeText(url)
}

interface QueueEntry {
  url: string
  module: ModuleSummary
  title: string
  size: number
  sourceLabel: string
  destDir: string
  selectedChildren?: string[]
  expectedHash?: ExpectedHash
  filename?: string
}

const selectableRows = computed(() => rows.value.filter((row) => rowSelectableUnitCount(row) > 0))
const selectedEntries = computed<QueueEntry[]>(() => {
  const entries: QueueEntry[] = []

  for (const row of rows.value) {
    if (visibleFilteredUrls.value && !visibleFilteredUrls.value.has(row.url)) {
      continue
    }
    if (!row.module || row.loading || row.error || !row.info) {
      continue
    }

    if (supportsChildSelection(row)) {
      const children = selectableChildren(row)
      const chosen = children.filter((child) => child.selected !== false)
      if (chosen.length === 0) {
        continue
      }

      if (isYouTubeFormatRow(row)) {
        const child = chosen[0]
        entries.push({
          url: row.url,
          module: row.module,
          title: row.info.name,
          size: child.size || row.info.size,
          sourceLabel: row.module.name,
          destDir: row.destDir || defaultOutputDir(),
          selectedChildren: [buildYouTubeSelectionUrl(row, child)]
            .filter((sourceUrl): sourceUrl is string => !!sourceUrl),
          expectedHash: row.expectedHash,
          filename: row.customName,
        })
        continue
      }

      if (chosen.length === children.length) {
        entries.push({
          url: row.url,
          module: row.module,
          title: row.info.name,
          size: row.info.size,
          sourceLabel: row.module.name,
          destDir: row.destDir || defaultOutputDir(),
          expectedHash: row.expectedHash,
          filename: row.customName,
        })
        continue
      }

      entries.push({
        url: row.url,
        module: row.module,
        title: row.info.name,
        size: chosen.reduce((sum, child) => sum + child.size, 0),
        sourceLabel: row.module.name,
        destDir: row.destDir || defaultOutputDir(),
        selectedChildren: chosen
          .map((child) => child.sourceUrl)
          .filter((sourceUrl): sourceUrl is string => !!sourceUrl),
        expectedHash: row.expectedHash,
        filename: row.customName,
      })
      continue
    }

    if (row.selected) {
      entries.push({
        url: row.url,
        module: row.module,
        title: row.info.name,
        size: row.info.size,
        sourceLabel: row.module.name,
        destDir: row.destDir || defaultOutputDir(),
        expectedHash: row.expectedHash,
        filename: row.customName,
      })
    }
  }

  return entries
})
const selectedCount = computed(() => selectedEntries.value.length)
// Espaço em disco livre na pasta de destino padrão (mesmo cálculo da barra do topo),
// usado para avisar se o TOTAL selecionado não cabe (A1 parte 2).
const captureDiskFreeBytes = ref(0)
async function refreshCaptureDiskFree(): Promise<void> {
  const settings = currentSettings.value ?? (await window.api.settings.load().catch(() => null))
  const dir = settings?.outputDir || '~/Downloads'
  const disk = await window.api.system.diskSpace(dir).catch(() => null)
  captureDiskFreeBytes.value = disk?.freeBytes ?? 0
}
const totalSelectedBytes = computed(() =>
  selectedEntries.value.reduce((sum, entry) => sum + (entry.size || 0), 0),
)
const capacityShortfall = computed(() =>
  captureDiskFreeBytes.value > 0 && totalSelectedBytes.value > captureDiskFreeBytes.value
    ? totalSelectedBytes.value - captureDiskFreeBytes.value
    : 0,
)
watch(() => rows.value.length, () => void refreshCaptureDiskFree())
const availableCount = computed(() => rows.value.reduce((sum, row) => sum + rowSelectableUnitCount(row), 0))
const selectedUnitCount = computed(() => rows.value.reduce((sum, row) => sum + rowSelectedUnitCount(row), 0))
const allSelectableChecked = computed(() => availableCount.value > 0 && selectedUnitCount.value === availableCount.value)
const someSelectableChecked = computed(() => selectedUnitCount.value > 0)
const onlineCount = computed(() => rows.value.filter((row) => row.availability === 'online').length)
const offlineCount = computed(() => rows.value.filter((row) => row.availability === 'offline').length)
const errorCount = computed(() => rows.value.filter((row) => !!row.error).length)
const folderCount = computed(() => rows.value.filter((row) => row.info?.isFolder).length)
const fileCount = computed(() => rows.value.filter((row) => row.info && !row.info.isFolder).length)
const duplicateCount = computed(() => rows.value.filter((row) => row.sourceUrls.length > 1).length)
const addButtonLabel = computed(() => {
  if (!adding.value) {
    return `${t('linkGrabberAddSelected')} ${selectedCount.value} ${t('linkGrabberSelectedSuffix')}`
  }
  const total = addQueueTotal.value || selectedEntries.value.length
  return `${t('linkGrabberQueueing')} ${Math.min(addQueueDone.value, total)}/${total}`
})

onMounted(async () => {
  currentSettings.value = await window.api.settings.load().catch(() => null)
  globalDestDir.value = defaultOutputDir()
  void refreshCaptureDiskFree()
  for (const row of rows.value) {
    row.destDir ||= defaultOutputDir()
  }
})

onUnmounted(() => {
  if (detectTimer !== null) {
    window.clearTimeout(detectTimer)
    detectTimer = null
  }
  window.api.mirrors.abort()
  mirrorsCleanup?.()
  mirrorsCleanup = null
  stopMirrorsTicker()
})

watch(urlsInput, () => {
  if (detectTimer !== null) {
    window.clearTimeout(detectTimer)
  }
  detectTimer = window.setTimeout(() => {
    void detectProviders()
  }, 220)
})

watch(
  () => props.incomingUrl,
  (url) => {
    if (!url) return
    appendImportedLinks(parseUrls(url))
  }
)

watch(
  rows,
  () => {
    const target = activeMirrorTarget.value
    if (!target) {
      return
    }

    const row = rows.value.find((item) => item.url === target.rowUrl)
    if (!row || !canSearchMirrors(row) || !row.info?.name) {
      closeMirrors()
      return
    }

    if (row.info.name !== target.filename && !mirrorsSearching.value) {
      activeMirrorTarget.value = {
        ...target,
        filename: row.info.name,
      }
      mirrorState.value.filename = row.info.name
    }
  },
  { deep: true }
)

watch(activeMirrorTarget, (next, prev) => {
  if (!next) {
    return
  }

  if (!prev || next.rowUrl !== prev.rowUrl || next.filename !== prev.filename) {
    nextTick(() => undefined)
  }
})

watch(
  () => rows.value.every((r) => !r.loading),
  (allDone) => {
    if (allDone && rows.value.length > 0) {
      nextTick(() => actionsRef.value?.scrollIntoView({ behavior: 'smooth', block: 'nearest' }))
    }
  }
)

function parseUrls(text: string): string[] {
  return parseCapturedUrls(text)
}

function appendImportedLinks(urls: string[]): void {
  const currentUrls = new Set(parseUrls(urlsInput.value))
  const uniqueUrls = urls.filter((url) => !currentUrls.has(url))
  if (uniqueUrls.length === 0) {
    lastError.value = ''
    return
  }
  const current = urlsInput.value.trim()
  const imported = uniqueUrls.join('\n')
  urlsInput.value = current ? `${current}\n${imported}` : imported
  lastError.value = ''
}

function showImportError(message: string): void {
  lastError.value = message
}

function nextFrame(): Promise<void> {
  return new Promise((resolve) => {
    window.requestAnimationFrame(() => resolve())
  })
}

async function detectProviders(): Promise<void> {
  const token = ++detectToken.value
  selectAllIntent.value = true
  lastError.value = ''
  const urls = parseUrls(urlsInput.value)

  if (urls.length === 0) {
    rows.value = []
    return
  }

  rows.value = urls.map((url) => ({
    url,
    displayName: truncateUrl(url),
    module: null,
    info: null,
    loading: true,
    error: '',
    availability: 'checking',
    cachedInfo: false,
    selected: true,
    expanded: false,
    sourceUrls: [url],
    sourceLabels: [],
    destDir: defaultOutputDir(),
    expectedHash: expectedHashFromUrl(url),
    youtubeOutputFormat: defaultYouTubeOutputFormat(),
    youtubeDownloadThumbnail: false,
    youtubeDownloadSubtitles: false,
    youtubeMultiAudio: false,
  }))

  const queue = rows.value.map((_, index) => index)
  let processed = 0
  const workerCount = Math.min(4, queue.length)

  const worker = async (): Promise<void> => {
    while (queue.length > 0) {
      const index = queue.shift()
      if (typeof index !== 'number') {
        return
      }

      const row = rows.value[index]
      if (!row) {
        continue
      }

      try {
        const module = await window.api.modules.detect(row.url)
        if (token !== detectToken.value) return
        rows.value[index].module = module

        if (!module) {
          rows.value[index].loading = false
          rows.value[index].error = t('linkGrabberUnsupportedServer')
          rows.value[index].availability = 'unknown'
          rows.value[index].selected = false
          return
        }

        const cached = await window.api.modules.cachedFileInfo(module.id, row.url).catch(() => null)
        if (token !== detectToken.value) return

        if (cached) {
          rows.value[index].info = hydrateInfoForSelection(cached)
          rows.value[index].displayName = cached.name
          applyExpectedHash(rows.value[index])
          normalizeProviderSelection(rows.value[index])
          rows.value[index].cachedInfo = true
          rows.value[index].availability = 'checking'
          await applyDiskAvailability(rows.value[index])
        }

        try {
          const info = await window.api.modules.fileInfo(module.id, row.url)
          if (token !== detectToken.value) return
          rows.value[index].info = hydrateInfoForSelection(info)
          rows.value[index].displayName = info.name
          applyExpectedHash(rows.value[index])
          normalizeProviderSelection(rows.value[index])
          rows.value[index].loading = false
          if (!selectAllIntent.value) {
            setRowSelection(rows.value[index], false)
          } else {
            rows.value[index].selected = isRowChecked(rows.value[index])
          }
          rows.value[index].availability = 'online'
          rows.value[index].cachedInfo = false
          rows.value[index].error = ''
          await applyDiskAvailability(rows.value[index])
        } catch (error) {
          if (token !== detectToken.value) return
          rows.value[index].loading = false
          rows.value[index].selected = false
          rows.value[index].availability = cached ? 'offline' : 'unknown'
          rows.value[index].error = error instanceof Error ? error.message : String(error)
          if (!cached) {
            throw error
          }
        }
      } catch (error) {
        if (token !== detectToken.value) return
        rows.value[index].loading = false
        rows.value[index].selected = false
        rows.value[index].error = error instanceof Error ? error.message : String(error)
        rows.value[index].availability = rows.value[index].info ? 'offline' : 'unknown'
      } finally {
        processed += 1
        if (processed % 3 === 0) {
          await nextFrame()
        }
      }
    }
  }

  await Promise.all(Array.from({ length: workerCount }, () => worker()))

  if (token === detectToken.value) {
    rows.value = groupDuplicateRows(rows.value)
    void refreshKnownStatus(token)
  }
}

/// Marca as linhas cuja URL já está na fila ou no histórico de concluídos e as
/// desmarca por padrão, para o usuário não re-adicionar sem querer (badge "já
/// baixado"/"na fila"). O backend também deduplica na adição como rede de segurança.
async function refreshKnownStatus(token?: number): Promise<void> {
  const activeToken = token ?? detectToken.value
  const allUrls = Array.from(new Set(rows.value.flatMap((row) => row.sourceUrls)))
  if (allUrls.length === 0) return
  const known = await window.api.downloads.checkKnownUrls(allUrls)
  if (activeToken !== detectToken.value) return
  for (const row of rows.value) {
    const hit = row.sourceUrls.map((url) => known[url]).find(Boolean)
    if (hit) {
      row.alreadyKnown = hit.location === 'history' ? 'history' : 'queue'
      setRowSelection(row, false)
      row.selected = false
    } else {
      row.alreadyKnown = undefined
    }
  }
}

async function addAll(): Promise<void> {
  if (selectedEntries.value.length === 0 || adding.value) return
  const entries = [...selectedEntries.value]
  adding.value = true
  addQueueDone.value = 0
  addQueueTotal.value = entries.length
  lastError.value = ''
  let addedCount = 0
  emit('adding-urls', entries.length)
  emit('added')
  const current = await window.api.settings.load().catch(() => null)
  currentSettings.value = current
  for (const entry of entries) {
    try {
      await window.api.downloads.add(
        entry.url,
        entry.module.id,
        entry.title,
        entry.size,
        entry.destDir,
        entry.selectedChildren,
        entry.expectedHash,
        undefined,
        entry.filename
      )
      addedCount += 1
      addQueueDone.value = addedCount
      await nextFrame()
    } catch (err) {
      lastError.value = `${t('linkGrabberAddError')} ${truncateUrl(entry.url)} — ${err instanceof Error ? err.message : String(err)}`
    }
  }

  adding.value = false
  addQueueDone.value = 0
  addQueueTotal.value = 0
  // Fim do processo de adicionar: zera o skeleton de forma confiável, mesmo que
  // alguns links tenham sido duplicados/erro (aí o total não sobe e o skeleton
  // antigo ficava preso pra sempre).
  emit('adding-urls', 0)
  if (addedCount > 0) {
    rows.value = pruneCapturedRows<CapturedRow>(rows.value)
    if (rows.value.length === 0) {
      urlsInput.value = ''
    }
  }
}

async function applyDiskAvailability(row: CapturedRow): Promise<void> {
  const info = row.info
  if (!info || info.size <= 0) return
  const settings = currentSettings.value ?? await window.api.settings.load().catch(() => null)
  currentSettings.value = settings
  const outputDir = row.destDir || settings?.outputDir || '~/Downloads'
  const disk = await window.api.system.diskSpace(outputDir).catch(() => null)
  if (!disk) return
  if (info.size > disk.freeBytes) {
    row.selected = false
    row.availability = 'offline'
    row.error = `Espaço insuficiente em disco: precisa de ${fmtBytes(info.size)}, disponível ${fmtBytes(disk.freeBytes)}.`
  } else if (row.error.startsWith('Espaço insuficiente em disco')) {
    row.error = ''
    row.availability = 'online'
    row.selected = rowSelectableUnitCount(row) > 0
  }
}

function toggleAllChecked(checked: boolean): void {
  selectAllIntent.value = checked
  for (const row of selectableRows.value) {
    setRowSelection(row, checked)
  }
}

function clear(): void {
  urlsInput.value = ''
  rows.value = []
  lastError.value = ''
  expectedHashesByFilename.value = {}
  closeMirrors()
}

function appendImportedHashes(hashes: Array<{ filename: string; value: string }>): void {
  const next = { ...expectedHashesByFilename.value }
  for (const item of hashes) {
    next[normalizeFilename(item.filename)] = {
      algorithm: 'crc32',
      value: item.value,
    }
  }
  expectedHashesByFilename.value = next
  for (const row of rows.value) {
    applyExpectedHash(row)
  }
  lastError.value = `${hashes.length} CRC32 importado(s) do .sfv`
}

function normalizeFilename(filename: string): string {
  return filename.trim().toLowerCase()
}

function expectedHashFromUrl(url: string): ExpectedHash | undefined {
  const fragment = url.split('#')[1] ?? ''
  for (const part of fragment.split('&')) {
    const [key, value] = part.split('=')
    const algorithm = key?.toLowerCase()
    if (!value || !['md5', 'sha1', 'sha256', 'crc32'].includes(algorithm)) {
      continue
    }
    const normalized = value.replace(/[^a-fA-F0-9]/g, '').toLowerCase()
    if (normalized) {
      return {
        algorithm: algorithm as ExpectedHash['algorithm'],
        value: normalized,
      }
    }
  }
  return undefined
}

function applyExpectedHash(row: CapturedRow): void {
  row.expectedHash = expectedHashFromUrl(row.url) ?? row.expectedHash
  const filename = row.info?.name ?? row.displayName
  const sfvHash = expectedHashesByFilename.value[normalizeFilename(filename)]
  if (sfvHash) {
    row.expectedHash = sfvHash
  }
}

function autoResize(event: Event): void {
  const el = event.target as HTMLTextAreaElement
  el.style.height = 'auto'
  el.style.height = Math.min(el.scrollHeight, 200) + 'px'
}

function groupDuplicateRows(inputRows: CapturedRow[]): CapturedRow[] {
  const grouped = new Map<string, CapturedRow>()

  for (const row of inputRows) {
    const info = row.info
    const key = info
      ? `${(info.name ?? row.displayName).toLowerCase()}::${info.size}::${info.isFolder ? 'folder' : 'file'}`
      : `url::${row.url}`

    const existing = grouped.get(key)
    const sourceLabel = row.module?.name ?? t('linkGrabberUnsupported')

    if (!existing) {
      grouped.set(key, {
        ...row,
        sourceUrls: [row.url],
        sourceLabels: [sourceLabel],
      })
      continue
    }

    existing.sourceUrls.push(row.url)
    existing.sourceLabels.push(sourceLabel)
    existing.expanded = existing.expanded || row.expanded
    existing.selected = existing.selected || row.selected
    if (existing.availability !== 'online' && row.availability === 'online') existing.availability = 'online'
    if (existing.availability !== 'online' && existing.availability !== 'offline' && row.availability === 'offline') existing.availability = 'offline'
    existing.cachedInfo = existing.cachedInfo || row.cachedInfo

    if (!existing.module && row.module) existing.module = row.module
    if ((!existing.info || !existing.info.children?.length) && row.info) existing.info = row.info
    if (!existing.error && row.error) existing.error = row.error
    if (!existing.expectedHash && row.expectedHash) existing.expectedHash = row.expectedHash
    if (!existing.destDir && row.destDir) existing.destDir = row.destDir
  }

  for (const row of grouped.values()) {
    row.url = row.sourceUrls[0]
  }

  return [...grouped.values()]
}

function defaultOutputDir(): string {
  return globalDestDir.value || currentSettings.value?.outputDir || '~/Downloads'
}

async function chooseRowDestination(row: CapturedRow): Promise<void> {
  const chosen = await window.api.settings.chooseDirectory().catch(() => '')
  if (!chosen) return
  row.destDir = chosen
  await applyDiskAvailability(row)
}

// Renomear antes de baixar (A3): guarda o nome custom (ou limpa se voltar ao original).
function onRenameRow(payload: { row: CapturedRow; name: string }): void {
  const name = payload.name.trim()
  const original = payload.row.info?.name ?? payload.row.displayName
  payload.row.customName = name && name !== original ? name : undefined
}

async function chooseGlobalDestination(): Promise<void> {
  const chosen = await window.api.settings.chooseDirectory().catch(() => '')
  if (!chosen) return
  globalDestDir.value = chosen
  applyGlobalDestinationToRows()
}

function applyGlobalDestinationToRows(): void {
  const target = globalDestDir.value || defaultOutputDir()
  for (const row of rows.value) {
    row.destDir = target
    void applyDiskAvailability(row)
  }
}

function hydrateInfoForSelection(info: FileInfo): RowFileInfo {
  const next: RowFileInfo = {
    ...info,
    children: info.children?.map((child) => ({
      ...child,
      selected: !!child.sourceUrl && !child.isFolder,
    })),
  }
  return next
}

function isYouTubeRow(row: CapturedRow): boolean {
  return row.module?.id === 'youtube'
}

function isYouTubeFormatRow(row: CapturedRow): boolean {
  return isYouTubeRow(row) && !row.info?.isFolder
}

function defaultYouTubeOutputFormat(): string {
  const value = currentSettings.value?.youtubeMergeFormat?.trim().toLowerCase() ?? ''
  return ['mp4', 'mkv', 'webm'].includes(value) ? value : 'mp4'
}

function ensureYouTubeOptions(row: CapturedRow): void {
  if (!isYouTubeFormatRow(row)) return
  row.youtubeOutputFormat = normalizeYouTubeOutputFormat(row.youtubeOutputFormat)
  row.youtubeDownloadThumbnail = Boolean(row.youtubeDownloadThumbnail)
  row.youtubeDownloadSubtitles = Boolean(row.youtubeDownloadSubtitles)
  row.youtubeMultiAudio = Boolean(row.youtubeMultiAudio)
  applyYouTubeOutputExtension(row)
}

function normalizeYouTubeOutputFormat(value: string | undefined): string {
  const normalized = value?.trim().toLowerCase() ?? ''
  return ['mp4', 'mkv', 'webm'].includes(normalized) ? normalized : defaultYouTubeOutputFormat()
}

function applyYouTubeOutputExtension(row: CapturedRow): void {
  if (!isYouTubeFormatRow(row) || !row.info?.name) return
  const format = normalizeYouTubeOutputFormat(row.youtubeOutputFormat)
  row.youtubeOutputFormat = format
  row.info.name = replaceFileExtension(row.info.name, format)
  row.displayName = row.info.name
}

function replaceFileExtension(filename: string, extension: string): string {
  const cleaned = extension.replace(/^\.+/, '')
  const slashIndex = Math.max(filename.lastIndexOf('/'), filename.lastIndexOf('\\'))
  const dotIndex = filename.lastIndexOf('.')
  if (dotIndex > slashIndex) {
    return `${filename.slice(0, dotIndex)}.${cleaned}`
  }
  return `${filename}.${cleaned}`
}

function normalizeProviderSelection(row: CapturedRow): void {
  if (!isYouTubeFormatRow(row) || !row.info?.children?.length) {
    return
  }
  ensureYouTubeOptions(row)
  let selectedAssigned = false
  for (const child of row.info.children) {
    const selectable = !!child.sourceUrl && !child.isFolder
    child.selected = selectable && !selectedAssigned
    if (child.selected) {
      selectedAssigned = true
    }
  }
  row.selected = selectedAssigned
}

function supportsChildSelection(row: CapturedRow): boolean {
  return (!!row.info?.isFolder || isYouTubeFormatRow(row)) && selectableChildren(row).length > 0
}

function childNodes(row: CapturedRow): DerivedChildNode<SelectableChild>[] {
  const children = row.info?.children
  if (!children?.length) {
    return []
  }
  const cached = childNodeCache.get(children)
  if (cached) {
    return cached
  }
  const nodes = flattenChildTree(buildChildTree(children))
  childNodeCache.set(children, nodes)
  return nodes
}

function selectableChildren(row: CapturedRow): SelectableChild[] {
  const children = row.info?.children
  if (!children?.length) {
    return []
  }
  const cached = selectableChildrenCache.get(children)
  if (cached) {
    return cached
  }
  const selectable = children.filter((child) => !!child.sourceUrl && !child.isFolder)
  selectableChildrenCache.set(children, selectable)
  return selectable
}

function selectableChildrenFromNode(node: DerivedChildNode<SelectableChild>): SelectableChild[] {
  const cached = nodeSelectableChildrenCache.get(node)
  if (cached) {
    return cached
  }

  const children: SelectableChild[] = []

  const visit = (current: DerivedChildNode<SelectableChild>): void => {
    if (current.isFolder) {
      for (const child of current.children) {
        visit(child)
      }
      return
    }

    if (current.original?.sourceUrl && !current.original.isFolder) {
      children.push(current.original)
    }
  }

  visit(node)
  nodeSelectableChildrenCache.set(node, children)
  return children
}

function rowSelectableUnitCount(row: CapturedRow): number {
  if (!row.module || row.loading || row.error) {
    return 0
  }
  if (isYouTubeFormatRow(row)) {
    return selectableChildren(row).length > 0 ? 1 : 0
  }
  const children = supportsChildSelection(row) ? selectableChildren(row) : []
  return children.length > 0 ? children.length : 1
}

function rowSelectedUnitCount(row: CapturedRow): number {
  if (isYouTubeFormatRow(row)) {
    return row.selected && selectableChildren(row).some((child) => child.selected !== false) ? 1 : 0
  }
  const children = supportsChildSelection(row) ? selectableChildren(row) : []
  if (children.length > 0) {
    return children.filter((child) => child.selected !== false).length
  }
  return row.selected && rowSelectableUnitCount(row) > 0 ? 1 : 0
}

function isRowChecked(row: CapturedRow): boolean {
  const total = rowSelectableUnitCount(row)
  return total > 0 && rowSelectedUnitCount(row) === total
}

function isRowIndeterminate(row: CapturedRow): boolean {
  if (isYouTubeFormatRow(row)) {
    return false
  }
  const selected = rowSelectedUnitCount(row)
  const total = rowSelectableUnitCount(row)
  return selected > 0 && selected < total
}

function folderNodeSelectableCount(node: DerivedChildNode<SelectableChild>): number {
  return selectableChildrenFromNode(node).length
}

function folderNodeSelectedCount(node: DerivedChildNode<SelectableChild>): number {
  return selectableChildrenFromNode(node).filter((child) => child.selected !== false).length
}

function isFolderNodeChecked(node: DerivedChildNode<SelectableChild>): boolean {
  const total = folderNodeSelectableCount(node)
  return total > 0 && folderNodeSelectedCount(node) === total
}

function isFolderNodeIndeterminate(node: DerivedChildNode<SelectableChild>): boolean {
  const selected = folderNodeSelectedCount(node)
  const total = folderNodeSelectableCount(node)
  return selected > 0 && selected < total
}

function setRowSelection(row: CapturedRow, checked: boolean): void {
  if (isYouTubeFormatRow(row) && supportsChildSelection(row)) {
    const children = selectableChildren(row)
    if (checked && !children.some((child) => child.selected !== false)) {
      const first = children[0]
      if (first) first.selected = true
    }
    row.selected = checked && children.length > 0
    return
  }

  if (supportsChildSelection(row)) {
    for (const child of selectableChildren(row)) {
      child.selected = checked
    }
  }
  row.selected = checked
}

function setRowSelectionChecked(payload: { row: CapturedRow; checked: boolean }): void {
  setRowSelection(payload.row, payload.checked)
}

function toggleExpanded(row: CapturedRow): void {
  row.expanded = !row.expanded
}

function toggleRowChecked(payload: { row: CapturedRow; checked: boolean }): void {
  setRowSelection(payload.row, payload.checked)
}

function toggleChildChecked(payload: { row: CapturedRow; child: SelectableChild; checked: boolean }): void {
  if (isYouTubeFormatRow(payload.row)) {
    for (const child of selectableChildren(payload.row)) {
      child.selected = child === payload.child ? payload.checked : false
    }
    payload.row.selected = payload.checked
    return
  }
  payload.child.selected = payload.checked
  payload.row.selected = isRowChecked(payload.row)
}

function selectYouTubeFormat(payload: { row: CapturedRow; sourceUrl: string }): void {
  if (!isYouTubeFormatRow(payload.row)) return
  let selectedAssigned = false
  for (const child of selectableChildren(payload.row)) {
    child.selected = child.sourceUrl === payload.sourceUrl
    selectedAssigned ||= !!child.selected
  }
  payload.row.selected = selectedAssigned
}

function updateYouTubeOption(payload: {
  row: CapturedRow
  key: 'youtubeOutputFormat' | 'youtubeDownloadThumbnail' | 'youtubeDownloadSubtitles' | 'youtubeMultiAudio'
  value: string | boolean
}): void {
  if (!isYouTubeFormatRow(payload.row)) return
  if (payload.key === 'youtubeOutputFormat') {
    payload.row.youtubeOutputFormat = normalizeYouTubeOutputFormat(String(payload.value))
    applyYouTubeOutputExtension(payload.row)
    return
  }
  payload.row[payload.key] = Boolean(payload.value)
}

function buildYouTubeSelectionUrl(row: CapturedRow, child: SelectableChild): string | undefined {
  if (!child.sourceUrl) return undefined
  const [base, fragment = ''] = child.sourceUrl.split('#')
  const params = new URLSearchParams(fragment)
  params.set('ytdlp_merge_format', normalizeYouTubeOutputFormat(row.youtubeOutputFormat))
  if (row.youtubeDownloadThumbnail) params.set('ytdlp_write_thumbnail', '1')
  if (row.youtubeDownloadSubtitles) params.set('ytdlp_write_subs', '1')
  if (row.youtubeMultiAudio) params.set('ytdlp_multi_audio', '1')
  return `${base}#${params.toString()}`
}

function toggleFolderNodeChecked(payload: { row: CapturedRow; node: DerivedChildNode<SelectableChild>; checked: boolean }): void {
  if (isYouTubeFormatRow(payload.row)) {
    const first = selectableChildrenFromNode(payload.node)[0]
    if (first) {
      toggleChildChecked({ row: payload.row, child: first, checked: payload.checked })
    }
    return
  }
  for (const child of selectableChildrenFromNode(payload.node)) {
    child.selected = payload.checked
  }
  payload.row.selected = isRowChecked(payload.row)
}

function fmtBytes(n: number): string {
  return formatBytes(n)
}

function truncateUrl(url: string): string {
  return shortenUrl(url)
}
</script>

<style scoped>
.link-grabber {
  padding: 24px 28px;
  display: flex;
  flex-direction: column;
  gap: 20px;
  max-width: none;
  width: 100%;
  min-width: 0;
  flex: 1;
  min-height: 0;
  overflow-y: auto;
}

.global-destination {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 14px;
  padding: 12px 14px;
  border: 1px solid var(--border-color);
  border-radius: 8px;
  background: var(--bg-card);
}

.global-destination div:first-child {
  display: flex;
  min-width: 0;
  flex-direction: column;
  gap: 3px;
}

.global-destination span {
  font-size: 11px;
  font-weight: 700;
  color: var(--text-muted);
  text-transform: uppercase;
}

.global-destination strong {
  min-width: 0;
  max-width: 100%;
  overflow: hidden;
  color: var(--text-primary);
  font-size: 13px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.global-destination-actions {
  display: flex;
  flex-shrink: 0;
  gap: 8px;
}

.destination-btn {
  border: 1px solid var(--border-color);
  border-radius: 7px;
  background: var(--bg-card);
  color: var(--text-primary);
  cursor: pointer;
  font-size: 12px;
  font-weight: 700;
  padding: 7px 10px;
}

.grabber-header {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.grabber-title {
  margin: 0;
  font-size: 18px;
  font-weight: 700;
  color: var(--text-primary);
}

.grabber-sub {
  margin: 0;
  font-size: 12px;
  color: var(--text-muted);
}

.url-field {
  display: flex;
  flex-direction: column;
  gap: 7px;
}

.field-label {
  font-size: 11px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.6px;
  color: var(--text-muted);
  display: flex;
  align-items: center;
  gap: 5px;
}

.textarea-wrapper {
  position: relative;
}

.textarea-icon {
  position: absolute;
  left: 12px;
  top: 13px;
  font-size: 13px;
  color: var(--text-muted);
  pointer-events: none;
}

.url-textarea {
  width: 100%;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: 10px;
  color: var(--text-primary);
  font-size: 13px;
  outline: none;
}

.url-textarea {
  min-height: 120px;
  font-family: 'JetBrains Mono', 'Fira Code', 'Courier New', monospace;
  padding: 12px 14px 12px 36px;
  resize: none;
  line-height: 1.6;
}

.captured-panel {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  border: 1px solid var(--border-color);
  border-radius: 12px;
  background: var(--bg-card);
}

.captured-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 14px;
  border-bottom: 1px solid rgba(126, 139, 164, 0.16);
}

.status-chips {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  padding: 12px 14px 0;
}

.status-chip {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 6px 10px;
  border-radius: 999px;
  font-size: 11px;
  color: var(--text-muted);
  background: rgba(126, 139, 164, 0.1);
}

.status-chip strong {
  color: var(--text-primary);
}

.status-chip.is-online {
  background: rgba(46, 204, 113, 0.12);
}

.status-chip.is-folder {
  background: rgba(255, 193, 7, 0.12);
}

.status-chip.is-file {
  background: rgba(59, 130, 246, 0.12);
}

.status-chip.is-duplicate {
  background: rgba(124, 111, 255, 0.12);
}

.status-chip.is-error {
  background: rgba(239, 83, 80, 0.12);
}

.master-check {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
  color: var(--text-primary);
}

.captured-meta {
  font-size: 12px;
  color: var(--text-muted);
}

.captured-list {
  display: flex;
  flex-direction: column;
  overflow-y: auto;
  flex: 1;
  min-height: 0;
}

.captured-row + .captured-row {
  border-top: 1px solid rgba(126, 139, 164, 0.14);
}

.captured-row.unavailable {
  opacity: 0.74;
}

.row-main {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px 14px;
}

.row-check {
  display: inline-flex;
}

.row-icon {
  width: 26px;
  height: 26px;
  display: inline-block;
  flex-shrink: 0;
  background-size: contain;
  background-position: center;
  background-repeat: no-repeat;
}

.row-copy {
  min-width: 0;
  flex: 1;
}

.row-title-line {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.row-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  flex: 1;
}

.row-badge {
  display: inline-flex;
  align-items: center;
  padding: 4px 8px;
  border-radius: 999px;
  font-size: 10px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  color: var(--text-muted);
  background: rgba(126, 139, 164, 0.12);
}

.row-badge.is-loading {
  background: rgba(99, 102, 241, 0.12);
  color: #6f63ff;
}

.row-badge.is-online {
  background: rgba(46, 204, 113, 0.12);
  color: #1c9b59;
}

.row-badge.is-folder {
  background: rgba(255, 193, 7, 0.12);
  color: #b27a00;
}

.row-badge.is-error {
  background: rgba(239, 83, 80, 0.12);
  color: #d64541;
}

.row-sub {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 6px;
  margin-top: 3px;
  font-size: 11px;
  color: var(--text-muted);
}

.row-actions {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  margin-left: auto;
}

.row-action-btn {
  width: 28px;
  height: 28px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border: 1px solid rgba(126, 139, 164, 0.22);
  border-radius: 8px;
  background: var(--bg-card);
  color: #66758a;
  cursor: pointer;
}

.row-action-btn:hover:not(:disabled) {
  border-color: color-mix(in srgb, var(--accent-color) 30%, var(--border-color));
  color: var(--accent-color);
}

.row-action-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.row-action-btn.is-mirror {
  color: #5b7cff;
  background: color-mix(in srgb, #5b7cff 8%, var(--bg-card));
  border-color: rgba(91, 124, 255, 0.2);
}

.row-action-btn.is-mirror.is-active {
  color: #fff;
  background: linear-gradient(90deg, #5b7cff, #6f63ff);
  border-color: transparent;
  box-shadow: 0 6px 18px rgba(99, 102, 241, 0.22);
}

.child-panel {
  padding: 0 14px 12px 52px;
}

.source-panel {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding-bottom: 10px;
}

.source-heading {
  font-size: 11px;
  font-weight: 700;
  color: var(--text-muted);
  text-transform: uppercase;
  letter-spacing: 0.5px;
}

.source-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  min-height: 26px;
}

.source-provider {
  font-size: 11px;
  font-weight: 600;
  color: var(--text-primary);
}

.source-url {
  font-size: 11px;
  color: var(--text-muted);
}

.children-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 12px;
  border: 1px solid color-mix(in srgb, var(--border-color) 86%, transparent);
  border-radius: 14px;
  background: color-mix(in srgb, var(--bg-primary) 92%, var(--bg-secondary));
}

.children-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding-bottom: 10px;
  margin-bottom: 4px;
  border-bottom: 1px solid color-mix(in srgb, var(--border-color) 84%, transparent);
}

.tree-master-check {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
  color: var(--text-secondary);
}

.tree-master-check input,
.child-check input {
  accent-color: var(--accent-color);
}

.tree-action-btn {
  border: 1px solid var(--border-color);
  background: transparent;
  color: var(--text-secondary);
  border-radius: 999px;
  padding: 6px 10px;
  font-size: 11px;
  font-weight: 600;
  cursor: pointer;
}

.child-row {
  min-height: 30px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 6px 0;
}

.child-row + .child-row {
  border-top: 1px solid rgba(126, 139, 164, 0.1);
}

.child-name {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
  flex: 1;
}

.child-name span {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.child-check {
  display: inline-flex;
  align-items: center;
  width: 16px;
  justify-content: center;
}

.child-check-placeholder {
  opacity: 0;
}

.child-icon {
  width: 17px;
  height: 17px;
  display: inline-block;
  flex-shrink: 0;
  background-size: contain;
  background-position: center;
  background-repeat: no-repeat;
}

.child-folder-badge {
  display: inline-flex;
  align-items: center;
  padding: 2px 7px;
  border-radius: 999px;
  font-size: 10px;
  font-weight: 700;
  color: #b27a00;
  background: rgba(255, 193, 7, 0.12);
}

.is-folder-row {
  background: color-mix(in srgb, var(--bg-card) 82%, rgba(255, 193, 7, 0.07));
  border-radius: 10px;
}

.child-side {
  display: inline-flex;
  align-items: center;
  gap: 10px;
  color: var(--text-secondary);
  font-size: 12px;
}

.child-selection-meta {
  min-width: 48px;
  text-align: right;
  color: var(--accent-color);
  font-weight: 700;
}

.error-msg {
  margin: 0;
  color: #ef5350;
  font-size: 12px;
}

.capacity-warn {
  margin: 0;
  display: flex;
  align-items: center;
  gap: 6px;
  color: #b45309;
  background: color-mix(in srgb, #f5b301 16%, transparent);
  border: 1px solid color-mix(in srgb, #f5b301 45%, transparent);
  border-radius: 8px;
  padding: 8px 10px;
  font-size: 12px;
  font-weight: 600;
}

/* ── Mirrors modal ─────────────────────────────────────────────────────── */
.mirrors-modal-backdrop {
  position: fixed;
  inset: 0;
  z-index: 35;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 22px;
  background: rgba(15, 23, 42, 0.42);
  backdrop-filter: blur(7px);
}

.mirrors-modal {
  width: min(1180px, 100%);
  max-height: calc(100vh - 44px);
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 18px;
  border: 1px solid rgba(91, 124, 255, 0.22);
  border-radius: 20px;
  background: color-mix(in srgb, var(--bg-card) 97%, #5b7cff 3%);
  box-shadow: 0 26px 70px rgba(15, 23, 42, 0.26);
  overflow: hidden;
}

.mirrors-modal-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  flex-wrap: wrap;
}

.mirrors-modal-copy {
  display: flex;
  flex-direction: column;
  gap: 6px;
  min-width: 0;
  flex: 1;
}

.mirrors-modal-subtitle {
  margin: 0;
  font-size: 12px;
  color: var(--text-muted);
}

.mirrors-modal-actions {
  display: inline-flex;
  align-items: center;
  gap: 8px;
}

.mirrors-title-row {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
  flex: 1;
}

.mirrors-title-row .pi {
  color: #6f63ff;
  font-size: 14px;
  flex-shrink: 0;
}

.mirrors-title {
  font-size: 13px;
  font-weight: 700;
  color: var(--text-primary);
}

.mirrors-file {
  font-size: 11px;
  color: var(--text-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
}

.mirrors-file-card {
  padding: 11px 13px;
  border-radius: 14px;
  border: 1px solid rgba(91, 124, 255, 0.14);
  background: color-mix(in srgb, var(--bg-primary) 88%, var(--bg-secondary));
  color: var(--text-secondary);
  font-size: 12px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.btn-find-mirrors {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 8px 14px;
  border-radius: 10px;
  border: 1px solid rgba(91, 124, 255, 0.35);
  background: linear-gradient(90deg, #5b7cff, #6f63ff);
  color: #fff;
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
  white-space: nowrap;
  flex-shrink: 0;
  box-shadow: 0 2px 8px rgba(99, 102, 241, 0.3);
}

.btn-find-mirrors:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.btn-find-mirrors:not(:disabled):hover {
  box-shadow: 0 4px 16px rgba(99, 102, 241, 0.45);
  transform: translateY(-1px);
  transition: all 0.15s ease;
}

.mirrors-body {
  display: flex;
  flex-direction: column;
  gap: 12px;
  min-height: 0;
}

.mirrors-stats {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 10px;
}

.mirrors-stat-card {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
  padding: 12px;
  border-radius: 12px;
  border: 1px solid rgba(91, 124, 255, 0.12);
  background: color-mix(in srgb, var(--bg-card) 92%, #f4f7ff 8%);
}

.mirrors-stat-label {
  font-size: 10px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.6px;
  color: var(--text-muted);
}

.mirrors-stat-card strong {
  font-size: 14px;
  font-weight: 700;
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.mirrors-stat-card small {
  font-size: 11px;
  color: var(--text-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.mirrors-progress-wrap {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 12px;
  border-radius: 12px;
  border: 1px solid rgba(91, 124, 255, 0.12);
  background: color-mix(in srgb, var(--bg-card) 94%, #eef3ff 6%);
}

.mirrors-progress-copy {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  font-size: 12px;
  color: var(--text-secondary);
}

.mirrors-progress-track {
  width: 100%;
  height: 8px;
  border-radius: 999px;
  background: rgba(126, 139, 164, 0.14);
  overflow: hidden;
}

.mirrors-progress-fill {
  height: 100%;
  border-radius: inherit;
  background: linear-gradient(90deg, #5b7cff, #6f63ff);
  transition: width 0.25s ease;
}

.mirrors-grid {
  display: grid;
  grid-template-columns: minmax(0, 1.1fr) minmax(0, 0.9fr);
  gap: 12px;
  min-height: 0;
}

.mirrors-log-wrap {
  position: relative;
  display: flex;
  flex-direction: column;
  min-height: 0;
  border-radius: 10px;
  overflow: hidden;
  border: 1px solid rgba(126, 139, 164, 0.15);
}

.mirrors-log-head,
.mirrors-results-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  padding: 10px 12px;
  border-bottom: 1px solid rgba(126, 139, 164, 0.12);
  background: color-mix(in srgb, var(--bg-card) 95%, #eef2fb 5%);
}

.mirrors-log-head span,
.mirrors-results-head span {
  font-size: 12px;
  font-weight: 700;
  color: var(--text-primary);
}

.mirrors-log-head small {
  font-size: 11px;
  color: var(--text-muted);
}

.mirrors-log {
  display: block;
  margin: 0;
  padding: 10px 12px;
  background: color-mix(in srgb, var(--bg-primary) 90%, #000 10%);
  color: #a0b0c8;
  font-family: 'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace;
  font-size: 11px;
  line-height: 1.6;
  max-height: 200px;
  overflow-y: auto;
  white-space: pre-wrap;
  word-break: break-all;
}

.mirrors-results-wrap {
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
  border-radius: 10px;
  overflow: hidden;
  border: 1px solid rgba(126, 139, 164, 0.15);
  background: color-mix(in srgb, var(--bg-card) 98%, #fff 2%);
}

.mirrors-results-label {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 11px;
  font-weight: 600;
  color: #1c9b59;
}

.mirrors-results-label .pi {
  font-size: 12px;
}

.mirrors-copy-btn {
  border: 1px solid rgba(46, 204, 113, 0.22);
  background: rgba(46, 204, 113, 0.08);
  color: #1c9b59;
  border-radius: 999px;
  padding: 6px 10px;
  font-size: 11px;
  font-weight: 700;
  cursor: pointer;
}

.mirrors-results-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 12px;
  max-height: 280px;
  overflow-y: auto;
}

.mirror-result-card {
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 12px;
  border: 1px solid rgba(46, 204, 113, 0.18);
  border-radius: 12px;
  background: color-mix(in srgb, var(--bg-card) 92%, #ecfff4 8%);
  text-align: left;
  cursor: pointer;
  transition:
    transform 0.15s ease,
    border-color 0.15s ease,
    box-shadow 0.15s ease;
}

.mirror-result-card:hover {
  transform: translateY(-1px);
  border-color: rgba(46, 204, 113, 0.34);
  box-shadow: 0 8px 22px rgba(46, 204, 113, 0.1);
}

.mirror-result-top,
.mirror-result-meta {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}

.mirror-result-source,
.mirror-result-score,
.mirror-result-meta {
  font-size: 11px;
}

.mirror-result-source {
  font-weight: 700;
  color: #1c9b59;
  text-transform: uppercase;
  letter-spacing: 0.4px;
}

.mirror-result-score {
  color: var(--text-muted);
}

.mirror-result-url {
  font-family: 'JetBrains Mono', 'Fira Code', monospace;
  font-size: 11px;
  line-height: 1.6;
  color: var(--text-primary);
  word-break: break-all;
}

.mirror-result-meta {
  color: var(--text-muted);
}

.mirrors-empty {
  padding: 18px 12px;
  font-size: 12px;
  color: var(--text-muted);
}

@media (max-width: 980px) {
  .mirrors-stats,
  .mirrors-grid {
    grid-template-columns: 1fr 1fr;
  }
}

@media (max-width: 720px) {
  .mirrors-modal {
    width: 100%;
    max-height: calc(100vh - 24px);
    padding: 14px;
  }

  .mirrors-stats,
  .mirrors-grid {
    grid-template-columns: 1fr;
  }

  .mirrors-modal-header,
  .mirrors-progress-copy,
  .mirrors-log-head,
  .mirrors-results-head,
  .mirror-result-top,
  .mirror-result-meta {
    align-items: flex-start;
    flex-direction: column;
  }
}

.actions-row {
  display: flex;
  justify-content: space-between;
  gap: 12px;
}

.btn-clear,
.btn-add {
  border: 1px solid var(--border-color);
  border-radius: 10px;
  padding: 11px 16px;
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
}

.btn-clear {
  background: transparent;
  color: var(--text-muted);
  border-color: var(--border-color);
}

.btn-add {
  background: linear-gradient(90deg, #5b7cff, #6f63ff);
  border-color: transparent;
  color: #fff;
  flex: 1;
  box-shadow: 0 2px 12px rgba(99, 102, 241, 0.35);
}

.btn-add:not(:disabled):hover {
  box-shadow: 0 4px 20px rgba(99, 102, 241, 0.5);
  transform: translateY(-1px);
  transition: all 0.15s ease;
}

.btn-clear:disabled,
.btn-add:disabled {
  opacity: 0.55;
  cursor: not-allowed;
}
</style>
