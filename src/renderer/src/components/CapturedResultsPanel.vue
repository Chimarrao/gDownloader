<template>
  <div class="captured-panel" data-tour="captured-results">
    <div class="status-chips">
      <span class="status-chip">
        {{ t('linkGrabberStatusAll') }}
        <strong>{{ rows.length }}</strong>
      </span>
      <span class="status-chip is-online">
        {{ t('linkGrabberStatusOnline') }}
        <strong>{{ onlineCount }}</strong>
      </span>
      <span class="status-chip is-error">
        {{ t('linkGrabberStatusOffline') }}
        <strong>{{ offlineCount }}</strong>
      </span>
      <span class="status-chip is-folder">
        {{ t('linkGrabberStatusFolders') }}
        <strong>{{ folderCount }}</strong>
      </span>
      <span class="status-chip is-file">
        {{ t('linkGrabberStatusFiles') }}
        <strong>{{ fileCount }}</strong>
      </span>
      <span class="status-chip is-duplicate">
        {{ t('linkGrabberStatusDuplicates') }}
        <strong>{{ duplicateCount }}</strong>
      </span>
      <span class="status-chip is-error">
        {{ t('linkGrabberStatusUnavailable') }}
        <strong>{{ errorCount }}</strong>
      </span>
    </div>

    <div class="captured-toolbar">
      <label class="master-check">
        <input
          type="checkbox"
          :checked="allSelectableChecked"
          :indeterminate.prop="someSelectableChecked && !allSelectableChecked"
          :title="t('linkGrabberSelectAllTitle')"
          @change="onToggleAll"
        />
        <span>{{ t('linkGrabberSelectAll') }}</span>
      </label>
      <span class="captured-meta">
        {{ selectedCount }} {{ t('linkGrabberSelectedSuffix') }} · {{ availableCount }} {{ t('linkGrabberAvailableSuffix') }}
      </span>
    </div>

    <div class="filter-bar">
      <input
        v-model="searchQuery"
        class="filter-search"
        type="text"
        placeholder="Buscar por nome..."
      />
      <div class="filter-chips">
        <label class="filter-chip" :class="{ active: filterOnlineOnly }">
          <input type="checkbox" v-model="filterOnlineOnly" />
          <span>Só online</span>
        </label>
        <button
          v-if="availableHosts.length > 0"
          class="filter-chip filter-hosts-btn"
          :class="{ active: filterHosts.length > 0 }"
          @click="showHostFilter = !showHostFilter"
        >
          <i class="pi pi-server"></i>
          <span>{{ filterHosts.length > 0 ? filterHosts.length + ' host(s)' : 'Hosts' }}</span>
        </button>
        <button
          v-if="hasActiveFilters"
          class="filter-clear-btn"
          @click="clearFilters"
        >
          <i class="pi pi-times"></i>
        </button>
      </div>
      <div class="filter-size">
        <input v-model.number="filterSizeMin" type="number" min="0" placeholder="Min MB" class="filter-size-input" />
        <span class="filter-size-sep">–</span>
        <input v-model.number="filterSizeMax" type="number" min="0" placeholder="Max MB" class="filter-size-input" />
        <span class="filter-size-label">MB</span>
      </div>
      <input
        v-model="excludeRegex"
        class="filter-search filter-regex"
        type="text"
        placeholder="Excluir regex..."
        title="Remove da seleção/adicionar itens cujo nome ou URL case com a regex"
      />
      <div v-if="showHostFilter && availableHosts.length > 0" class="host-filter-dropdown">
        <label
          v-for="host in availableHosts"
          :key="host"
          class="host-filter-item"
          :class="{ active: filterHosts.includes(host) }"
        >
          <input
            type="checkbox"
            :checked="filterHosts.includes(host)"
            @change="toggleHostFilter(host)"
          />
          <span>{{ host }}</span>
        </label>
      </div>
    </div>

    <VirtualRows
      class="captured-list"
      :items="filteredRows"
      key-field="url"
      :item-height="112"
      :overscan="8"
      max-height="100%"
    >
      <template #default="{ item: row }">
      <div
        class="captured-row"
        :class="{ unavailable: !row.module || row.error }"
      >
        <div class="row-main">
          <label class="row-check">
              <input
                type="checkbox"
                :checked="isRowChecked(row)"
                :indeterminate.prop="isRowIndeterminate(row)"
                :disabled="rowSelectableUnitCount(row) === 0"
                @change="onToggleRow(row, $event)"
              />
          </label>

          <span
            v-if="row.module?.id === 'youtube' && row.info?.thumbnailUrl"
            class="row-icon row-thumb"
            aria-label="YouTube thumbnail"
            role="img"
          >
            <img :src="row.info.thumbnailUrl" class="row-thumb-img" />
          </span>
          <span
            v-else-if="row.module?.id === 'youtube'"
            class="row-icon provider-row-icon"
            :style="{ color: getProviderIcon(row.module.id).color }"
            aria-label="YouTube"
            role="img"
            v-html="getProviderIcon(row.module.id).svg"
          ></span>
          <span
            v-else-if="row.loading"
            class="row-icon row-thumb-loading"
            aria-hidden="true"
          ></span>
          <span
            v-else
            class="row-icon"
            :class="getFileIcon(effectiveName(row), row.info?.mimeType, row.info?.isFolder).className"
            :aria-label="getFileIcon(effectiveName(row), row.info?.mimeType, row.info?.isFolder).alt"
            role="img"
          ></span>

          <div class="row-copy">
            <div class="row-title-line">
              <template v-if="renamingUrl === row.url">
                <input
                  class="row-rename-input"
                  type="text"
                  v-model="renameDraft"
                  @keyup.enter="commitRename(row)"
                  @keyup.esc="cancelRename"
                  @blur="commitRename(row)"
                />
              </template>
              <template v-else>
                <div class="row-title" :title="effectiveName(row)">{{ effectiveName(row) }}</div>
                <button
                  class="row-name-btn"
                  type="button"
                  title="Renomear arquivo antes de baixar"
                  @click.stop="startRename(row)"
                >
                  <i class="pi pi-pencil"></i>
                </button>
                <button
                  class="row-name-btn"
                  type="button"
                  :title="copiedUrl === row.url ? 'Copiado!' : 'Copiar nome do arquivo'"
                  @click.stop="copyName(row)"
                >
                  <i class="pi" :class="copiedUrl === row.url ? 'pi-check' : 'pi-copy'"></i>
                </button>
              </template>
              <span
                class="row-badge"
                :class="{
                  'is-loading': row.loading,
                  'is-online': !row.loading && !row.error && !!row.module,
                  'is-error': !!row.error,
                  'is-folder': row.info?.isFolder,
                }"
              >
                {{ rowBadgeLabel(row) }}
              </span>
            </div>
            <div class="row-sub">
              <span>{{ row.module?.name ?? t('linkGrabberUnsupported') }}</span>
              <template v-if="row.info?.channelName">
                <span>·</span>
                <span class="row-channel">
                  <img v-if="row.info.channelThumbnailUrl" :src="row.info.channelThumbnailUrl" class="row-channel-avatar" />
                  <span>{{ row.info.channelName }}</span>
                </span>
              </template>
              <span>·</span>
              <span>{{ availabilityLabel(row) }}</span>
              <template v-if="row.sourceLabels.length > 1">
                <span>·</span>
                <span>{{ row.sourceLabels.length }} {{ t('linkGrabberSourcesLabel') }}: {{ row.sourceLabels.join(', ') }}</span>
              </template>
              <span>·</span>
              <span v-if="row.loading && row.cachedInfo">{{ t('linkGrabberCachedChecking') }}</span>
              <span v-else-if="row.loading">{{ t('linkGrabberReadingMetadata') }}</span>
              <span v-else-if="row.error">{{ row.error }}</span>
              <span v-else-if="row.info">{{ fmtBytes(effectiveSize(row.info.size, row.info.children, row.info.isFolder)) }}</span>
              <template v-if="row.info?.durationSecs">
                <span>·</span>
                <span>{{ formatMediaDuration(row.info.durationSecs) }}</span>
              </template>
              <template v-if="selectedQualityLabel(row)">
                <span>·</span>
                <span class="quality-chip">{{ selectedQualityLabel(row) }}</span>
              </template>
              <template v-if="row.expectedHash">
                <span>·</span>
                <span class="hash-chip">{{ row.expectedHash.algorithm.toUpperCase() }} {{ row.expectedHash.value }}</span>
              </template>
            </div>
            <div class="row-destination">
              <i class="pi pi-folder"></i>
              <span class="destination-path" :title="row.destDir || 'Pasta padrão'">
                {{ row.destDir || 'Pasta padrão' }}
              </span>
              <button
                class="destination-btn"
                type="button"
                title="Escolher pasta deste download"
                @click="emit('choose-destination', row)"
              >
                Alterar
              </button>
            </div>
          </div>

          <div class="row-actions">
            <button
              v-if="canSearchMirrors(row)"
              class="row-action-btn is-mirror"
              :class="{ 'is-active': activeMirrorRowUrl === row.url }"
              :title="t('linkGrabberOpenMirrorsTitle')"
              data-tour="mirrors"
              @click="emit('open-mirrors', row)"
            >
              <i class="pi pi-sitemap"></i>
            </button>
            <button
              v-if="(supportsChildSelection(row) && (row.info?.children?.length ?? 0) > 0) || row.sourceUrls.length > 1"
              class="row-action-btn expand-btn"
              :title="row.expanded ? t('closeDetails') : t('openDetails')"
              @click="emit('toggle-expanded', row)"
            >
              <i class="pi" :class="row.expanded ? 'pi-chevron-up' : 'pi-chevron-down'"></i>
            </button>
          </div>
        </div>

        <div v-if="row.expanded && ((supportsChildSelection(row) && row.info?.children?.length) || row.sourceUrls.length > 1)" class="child-panel">
          <div v-if="row.sourceUrls.length > 1" class="source-panel">
            <div class="source-heading">{{ t('linkGrabberAvailableSources') }}</div>
            <div
              v-for="(sourceUrl, idx) in row.sourceUrls"
              :key="`${row.url}:source:${sourceUrl}`"
              class="source-row"
            >
              <span class="source-provider">{{ row.sourceLabels[idx] ?? t('linkGrabberServer') }}</span>
              <span class="source-url" :title="sourceUrl">{{ truncateUrl(sourceUrl) }}</span>
            </div>
          </div>

          <div v-if="isYouTubeFormatRow(row) && row.info?.children?.length" class="youtube-options">
            <div class="youtube-option-grid">
              <label class="youtube-field">
                <span>Vídeo</span>
                <select
                  class="youtube-select"
                  :value="selectedYouTubeSourceUrl(row)"
                  :title="selectedYouTubeFormatDetail(row)"
                  @change="onSelectYouTubeFormat(row, $event)"
                >
                  <option
                    v-for="child in youtubeSelectableChildren(row)"
                    :key="child.sourceUrl"
                    :value="child.sourceUrl"
                    :title="youtubeFormatDetail(child)"
                  >
                    {{ youtubeFormatLabel(child) }}
                  </option>
                </select>
              </label>

              <label class="youtube-field">
                <span>Container</span>
                <select
                  class="youtube-select"
                  :value="row.youtubeOutputFormat ?? 'mp4'"
                  @change="onUpdateYouTubeString(row, 'youtubeOutputFormat', $event)"
                >
                  <option v-for="format in youtubeOutputFormats" :key="format.value" :value="format.value">
                    {{ format.label }}
                  </option>
                </select>
              </label>
            </div>

            <div v-if="selectedYouTubeFormatDetail(row)" class="youtube-detail" :title="selectedYouTubeFormatDetail(row)">
              {{ selectedYouTubeFormatDetail(row) }}
            </div>

            <div class="youtube-extra-groups">
              <div class="youtube-extra-group">
                <span>Áudio</span>
                <label class="youtube-check-option" title="Inclui múltiplas faixas de áudio quando o YouTube oferece mais de um idioma.">
                  <input
                    type="checkbox"
                    :checked="!!row.youtubeMultiAudio"
                    @change="onUpdateYouTubeBoolean(row, 'youtubeMultiAudio', $event)"
                  />
                  <span>Múltiplos idiomas</span>
                </label>
              </div>

              <div class="youtube-extra-group">
                <span>Descrição</span>
                <label class="youtube-check-option">
                  <input
                    type="checkbox"
                    :checked="!!row.youtubeDownloadThumbnail"
                    @change="onUpdateYouTubeBoolean(row, 'youtubeDownloadThumbnail', $event)"
                  />
                  <span>Thumbnail</span>
                </label>
                <label class="youtube-check-option">
                  <input
                    type="checkbox"
                    :checked="!!row.youtubeDownloadSubtitles"
                    @change="onUpdateYouTubeBoolean(row, 'youtubeDownloadSubtitles', $event)"
                  />
                  <span>Legendas</span>
                </label>
              </div>
            </div>
          </div>

          <div v-else-if="supportsChildSelection(row) && row.info?.children?.length" class="children-list">
            <div v-if="supportsChildSelection(row)" class="children-toolbar">
              <label class="tree-master-check">
                  <input
                    type="checkbox"
                    :checked="isRowChecked(row)"
                    :indeterminate.prop="isRowIndeterminate(row)"
                    @change="onToggleRow(row, $event)"
                  />
                <span>{{ childSelectionLabel(row) }}</span>
              </label>
              <button class="tree-action-btn" type="button" @click="emit('set-row-selection', { row, checked: false })">
                {{ t('clear') }}
              </button>
            </div>
            <VirtualRows
              :items="childNodes(row)"
              key-field="key"
              :item-height="36"
              max-height="420px"
            >
              <template #default="{ item: node }">
                <div
                  class="child-row"
                  :class="{ 'is-folder-row': node.isFolder }"
                >
                  <div class="child-name" :style="{ paddingInlineStart: `${node.depth * 18}px` }">
                    <label
                      v-if="node.isFolder && folderNodeSelectableCount(node) > 0"
                      class="child-check"
                      :title="`${t('linkGrabberSelectInside')} ${node.name}`"
                    >
                      <input
                        type="checkbox"
                        :checked="isFolderNodeChecked(node)"
                        :indeterminate.prop="isFolderNodeIndeterminate(node)"
                        @change="onToggleFolderNode(row, node, $event)"
                      />
                    </label>
                    <label
                      v-else-if="supportsChildSelection(row) && node.original?.sourceUrl && !node.isFolder"
                      class="child-check"
                      :title="`${t('linkGrabberSelectItem')} ${node.name}`"
                    >
                      <input
                        type="checkbox"
                        :checked="node.original.selected !== false"
                        :disabled="!node.original.sourceUrl"
                        @change="onToggleChild(row, node.original, $event)"
                      />
                    </label>
                    <span v-else class="child-check child-check-placeholder"></span>
                    <span
                      class="child-icon"
                      :class="getFileIcon(node.name, node.mimeType, node.isFolder).className"
                      :aria-label="getFileIcon(node.name, node.mimeType, node.isFolder).alt"
                      role="img"
                    ></span>
                    <span>{{ node.name }}</span>
                    <span v-if="node.isFolder" class="child-folder-badge">{{ node.fileCount }} {{ t('itemsCount') }}</span>
                  </div>
                  <div class="child-side">
                    <span v-if="node.isFolder && folderNodeSelectableCount(node) > 0" class="child-selection-meta">
                      {{ folderNodeSelectedCount(node) }}/{{ folderNodeSelectableCount(node) }}
                    </span>
                    <span>{{ fmtBytes(node.size) }}</span>
                  </div>
                </div>
              </template>
            </VirtualRows>
          </div>
        </div>
      </div>
      </template>
    </VirtualRows>
  </div>
</template>

<script setup lang="ts">
import type { PropType } from 'vue'
import { ref, computed, watch, nextTick } from 'vue'

import { useI18n } from '../i18n'
import { getFileIcon } from '../assets/file-icons'
import { getProviderIcon } from '../assets/provider-icons'
import type { DerivedChildNode } from '../utils/child-tree'
import VirtualRows from './VirtualRows.vue'
import type { CapturedRow, SelectableChild } from './link-grabber-model'
import { effectiveSize } from '../utils/display-size'
import { formatMediaDuration } from '../utils/format'

const props = defineProps({
  rows: {
    type: Array as PropType<CapturedRow[]>,
    required: true,
  },
  allSelectableChecked: {
    type: Boolean,
    required: true,
  },
  someSelectableChecked: {
    type: Boolean,
    required: true,
  },
  selectedCount: {
    type: Number,
    required: true,
  },
  availableCount: {
    type: Number,
    required: true,
  },
  onlineCount: {
    type: Number,
    required: true,
  },
  offlineCount: {
    type: Number,
    required: true,
  },
  folderCount: {
    type: Number,
    required: true,
  },
  fileCount: {
    type: Number,
    required: true,
  },
  duplicateCount: {
    type: Number,
    required: true,
  },
  errorCount: {
    type: Number,
    required: true,
  },
  activeMirrorRowUrl: {
    type: String,
    default: '',
  },
  isRowChecked: {
    type: Function as PropType<(row: CapturedRow) => boolean>,
    required: true,
  },
  isRowIndeterminate: {
    type: Function as PropType<(row: CapturedRow) => boolean>,
    required: true,
  },
  rowSelectableUnitCount: {
    type: Function as PropType<(row: CapturedRow) => number>,
    required: true,
  },
  supportsChildSelection: {
    type: Function as PropType<(row: CapturedRow) => boolean>,
    required: true,
  },
  childNodes: {
    type: Function as PropType<(row: CapturedRow) => DerivedChildNode<SelectableChild>[]>,
    required: true,
  },
  folderNodeSelectableCount: {
    type: Function as PropType<(node: DerivedChildNode<SelectableChild>) => number>,
    required: true,
  },
  folderNodeSelectedCount: {
    type: Function as PropType<(node: DerivedChildNode<SelectableChild>) => number>,
    required: true,
  },
  isFolderNodeChecked: {
    type: Function as PropType<(node: DerivedChildNode<SelectableChild>) => boolean>,
    required: true,
  },
  isFolderNodeIndeterminate: {
    type: Function as PropType<(node: DerivedChildNode<SelectableChild>) => boolean>,
    required: true,
  },
  canSearchMirrors: {
    type: Function as PropType<(row: CapturedRow) => boolean>,
    required: true,
  },
  fmtBytes: {
    type: Function as PropType<(value: number) => string>,
    required: true,
  },
  truncateUrl: {
    type: Function as PropType<(url: string) => string>,
    required: true,
  },
})

const emit = defineEmits<{
  (e: 'toggle-all', checked: boolean): void
  (e: 'toggle-row', payload: { row: CapturedRow; checked: boolean }): void
  (e: 'toggle-child', payload: { row: CapturedRow; child: SelectableChild; checked: boolean }): void
  (e: 'toggle-folder-node', payload: { row: CapturedRow; node: DerivedChildNode<SelectableChild>; checked: boolean }): void
  (e: 'set-row-selection', payload: { row: CapturedRow; checked: boolean }): void
  (e: 'select-youtube-format', payload: { row: CapturedRow; sourceUrl: string }): void
  (e: 'update-youtube-option', payload: { row: CapturedRow; key: 'youtubeOutputFormat' | 'youtubeDownloadThumbnail' | 'youtubeDownloadSubtitles' | 'youtubeMultiAudio'; value: string | boolean }): void
  (e: 'toggle-expanded', row: CapturedRow): void
  (e: 'open-mirrors', row: CapturedRow): void
  (e: 'choose-destination', row: CapturedRow): void
  (e: 'rename-row', payload: { row: CapturedRow; name: string }): void
  (e: 'filtered-change', urls: string[]): void
}>()

const { t } = useI18n()

// ── Renomear (A3) e copiar nome (A4) ──────────────────────────────────────────
const renamingUrl = ref<string | null>(null)
const renameDraft = ref('')
const copiedUrl = ref<string | null>(null)

function effectiveName(row: CapturedRow): string {
  return row.customName || row.info?.name || row.displayName
}

function startRename(row: CapturedRow): void {
  renamingUrl.value = row.url
  renameDraft.value = effectiveName(row)
  void nextTick(() => {
    const input = document.querySelector<HTMLInputElement>('.row-rename-input')
    input?.focus()
    input?.select()
  })
}

function commitRename(row: CapturedRow): void {
  if (renamingUrl.value !== row.url) return
  emit('rename-row', { row, name: renameDraft.value })
  renamingUrl.value = null
}

function cancelRename(): void {
  renamingUrl.value = null
}

async function copyName(row: CapturedRow): Promise<void> {
  try {
    await navigator.clipboard.writeText(effectiveName(row))
    copiedUrl.value = row.url
    window.setTimeout(() => {
      if (copiedUrl.value === row.url) copiedUrl.value = null
    }, 1200)
  } catch {
    // clipboard indisponível — ignora silenciosamente
  }
}

const youtubeOutputFormats = [
  { value: 'mp4', label: 'MP4' },
  { value: 'mkv', label: 'MKV' },
  { value: 'webm', label: 'WebM' },
]

// ── Filter state ──────────────────────────────────────────────────────────────
const searchQuery = ref('')
const filterHosts = ref<string[]>([])
const filterSizeMin = ref<number>(0)
const filterSizeMax = ref<number>(0)
const filterOnlineOnly = ref(false)
const excludeRegex = ref('')
const showHostFilter = ref(false)

const availableHosts = computed(() => {
  const hosts = new Set<string>()
  for (const row of props.rows) {
    const host = row.module?.name ?? ''
    if (host) hosts.add(host)
  }
  return [...hosts].sort()
})

const filteredRows = computed(() => {
  const q = searchQuery.value.toLowerCase().trim()
  return props.rows.filter(row => {
    // Search by name
    if (q) {
      const name = (row.info?.name ?? row.displayName ?? '').toLowerCase()
      if (!name.includes(q)) return false
    }
    // Host filter
    if (filterHosts.value.length > 0) {
      const rowHost = row.module?.name ?? ''
      if (!filterHosts.value.includes(rowHost)) return false
    }
    // Size range (info.size is in bytes)
    const size = row.info?.size ?? 0
    if (filterSizeMin.value > 0 && size > 0 && size < filterSizeMin.value * 1_048_576) return false
    if (filterSizeMax.value > 0 && size > filterSizeMax.value * 1_048_576) return false
    // Online-only toggle
    if (filterOnlineOnly.value && row.availability === 'offline') return false
    if (excludeRegex.value.trim()) {
      try {
        const rule = new RegExp(excludeRegex.value.trim(), 'i')
        const haystack = `${row.info?.name ?? row.displayName ?? ''}\n${row.url}`
        if (rule.test(haystack)) return false
      } catch {
        // Regex incompleta durante a digitação: ignora até ficar válida.
      }
    }
    return true
  })
})

// Debounce do 'filtered-change': ao processar muitos links de uma vez (ex.: 60), o
// filteredRows muda a cada linha e emitir toda hora fazia o capturador piscar.
let filteredChangeTimer: ReturnType<typeof setTimeout> | null = null
watch(
  filteredRows,
  (rows) => {
    if (filteredChangeTimer) clearTimeout(filteredChangeTimer)
    const urls = rows.map((row) => row.url)
    filteredChangeTimer = setTimeout(() => emit('filtered-change', urls), 250)
  },
  { immediate: true }
)

function toggleHostFilter(host: string): void {
  const idx = filterHosts.value.indexOf(host)
  if (idx === -1) {
    filterHosts.value = [...filterHosts.value, host]
  } else {
    filterHosts.value = filterHosts.value.filter(h => h !== host)
  }
}

function clearFilters(): void {
  searchQuery.value = ''
  filterHosts.value = []
  filterSizeMin.value = 0
  filterSizeMax.value = 0
  filterOnlineOnly.value = false
  excludeRegex.value = ''
}

const hasActiveFilters = computed(() =>
  searchQuery.value !== '' ||
  filterHosts.value.length > 0 ||
  filterSizeMin.value > 0 ||
  filterSizeMax.value > 0 ||
  filterOnlineOnly.value ||
  excludeRegex.value.trim() !== ''
)

function rowBadgeLabel(row: CapturedRow): string {
  if (row.loading) {
    return row.cachedInfo ? t('linkGrabberStatusCache') : t('linkGrabberStatusReading')
  }
  if (row.availability === 'offline') {
    return t('linkGrabberStatusOffline')
  }
  if (row.error) {
    return t('error')
  }
  if (row.info?.isFolder) {
    return t('linkGrabberStatusFolder')
  }
  return t('linkGrabberStatusFile')
}

function childSelectionLabel(row: CapturedRow): string {
  return row.info?.isFolder ? t('linkGrabberSelectAllFolder') : 'Formato do video'
}

function selectedQualityLabel(row: CapturedRow): string {
  if (row.module?.id !== 'youtube') return ''
  const child = selectedYouTubeChild(row)
  if (!child) return ''
  return youtubeFormatLabel(child)
}

function availabilityLabel(row: CapturedRow): string {
  if (row.availability === 'online') return t('linkGrabberAvailabilityOnline')
  if (row.availability === 'offline') return t('linkGrabberAvailabilityOffline')
  if (row.loading) return t('linkGrabberAvailabilityChecking')
  return t('linkGrabberAvailabilityUnknown')
}

function checkboxValue(event: Event): boolean {
  return (event.target as HTMLInputElement).checked
}

function onToggleAll(event: Event): void {
  emit('toggle-all', checkboxValue(event))
}

function onToggleRow(row: CapturedRow, event: Event): void {
  emit('toggle-row', { row, checked: checkboxValue(event) })
}

function onToggleChild(row: CapturedRow, child: SelectableChild, event: Event): void {
  emit('toggle-child', { row, child, checked: checkboxValue(event) })
}

function onToggleFolderNode(row: CapturedRow, node: DerivedChildNode<SelectableChild>, event: Event): void {
  emit('toggle-folder-node', { row, node, checked: checkboxValue(event) })
}

function isYouTubeFormatRow(row: CapturedRow): boolean {
  return row.module?.id === 'youtube' && !row.info?.isFolder
}

function youtubeSelectableChildren(row: CapturedRow): SelectableChild[] {
  return row.info?.children?.filter((child) => !!child.sourceUrl && !child.isFolder) ?? []
}

function selectedYouTubeChild(row: CapturedRow): SelectableChild | undefined {
  return youtubeSelectableChildren(row).find((child) => child.selected !== false)
    ?? youtubeSelectableChildren(row)[0]
}

function selectedYouTubeSourceUrl(row: CapturedRow): string {
  return selectedYouTubeChild(row)?.sourceUrl ?? ''
}

function selectedYouTubeFormatDetail(row: CapturedRow): string {
  const child = selectedYouTubeChild(row)
  return child ? youtubeFormatDetail(child) : ''
}

function onSelectYouTubeFormat(row: CapturedRow, event: Event): void {
  emit('select-youtube-format', {
    row,
    sourceUrl: (event.target as HTMLSelectElement).value,
  })
}

function onUpdateYouTubeBoolean(
  row: CapturedRow,
  key: 'youtubeDownloadThumbnail' | 'youtubeDownloadSubtitles' | 'youtubeMultiAudio',
  event: Event,
): void {
  emit('update-youtube-option', {
    row,
    key,
    value: checkboxValue(event),
  })
}

function onUpdateYouTubeString(row: CapturedRow, key: 'youtubeOutputFormat', event: Event): void {
  emit('update-youtube-option', {
    row,
    key,
    value: (event.target as HTMLSelectElement).value,
  })
}

function youtubeFormatLabel(child: SelectableChild): string {
  const cleaned = child.filename
    .replace(/\s+#\S+$/i, '')
    .replace(/\s+/g, ' ')
    .trim()

  if (/^melhor qualidade/i.test(cleaned)) {
    const raw = cleaned.replace(/^Melhor qualidade\s*-\s*/i, '')
    return `Melhor qualidade: ${friendlyResolution(raw) || 'melhor disponível'}`
  }

  const parts = cleaned.split(' · ').map((part) => part.trim()).filter(Boolean)
  const rawResolution = parts.find((part) => /(\d{3,5}x\d{3,5}|\d{3,4}p|audio)/i.test(part)) ?? parts[0] ?? ''
  const resolution = friendlyResolution(rawResolution)
  const fps = parts.find((part) => /\d+\s*fps/i.test(part)) ?? ''
  const ext = parts.find((part) => /^(mp4|mkv|webm|m4a|opus|aac)$/i.test(part)) ?? ''
  const mediaKind = parts.some((part) => /^audio$/i.test(part) || /audio only/i.test(part))
    ? 'Áudio'
    : 'Vídeo'
  const displayResolution = mediaKind === 'Áudio' && resolution === 'somente áudio' ? '' : resolution
  return [mediaKind, displayResolution, fps, ext.toUpperCase()]
    .filter((part) => part.trim().length > 0)
    .join(' · ')
}

function youtubeFormatDetail(child: SelectableChild): string {
  return child.filename
    .replace(/\s+#\S+$/i, '')
    .replace(/\bvideo\+audio\b/i, 'vídeo + áudio')
    .replace(/\bvideo\b/i, 'vídeo')
    .replace(/\baudio\b/i, 'áudio')
    .trim()
}

function friendlyResolution(value: string): string {
  const trimmed = value.trim()
  const dimensions = trimmed.match(/(\d{3,5})x(\d{3,5})/)
  if (dimensions) {
    const width = Number(dimensions[1])
    const height = Number(dimensions[2])
    if (height >= 2160 || width >= 3840) return suffixFps(trimmed, '4K')
    if (height > 0) return suffixFps(trimmed, `${height}p`)
  }

  const progressive = trimmed.match(/\b(\d{3,4})p\b/i)
  if (progressive) {
    const height = Number(progressive[1])
    return suffixFps(trimmed, height >= 2160 ? '4K' : `${height}p`)
  }

  if (/audio/i.test(trimmed)) return 'somente áudio'
  return trimmed
}

function suffixFps(source: string, label: string): string {
  const fps = source.match(/\b(\d+)\s*fps\b/i)?.[1]
  return fps ? `${label} ${fps}fps` : label
}
</script>

<style scoped>
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

.captured-list :deep(.virtual-row-shell + .virtual-row-shell .captured-row) {
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

.provider-row-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
}

.provider-row-icon :deep(svg) {
  width: 24px;
  height: 24px;
  display: block;
}

.row-thumb {
  width: 40px;
  height: 28px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  overflow: hidden;
  border-radius: 3px;
}

.row-thumb-img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
}

.row-thumb-loading {
  width: 26px;
  height: 26px;
  border-radius: 3px;
  background: linear-gradient(90deg, var(--color-bg-3) 25%, var(--color-bg-2) 50%, var(--color-bg-3) 75%);
  background-size: 200% 100%;
  animation: shimmer 1.2s infinite;
}

.row-channel {
  display: inline-flex;
  align-items: center;
  gap: 4px;
}

.row-channel-avatar {
  width: 14px;
  height: 14px;
  border-radius: 50%;
  object-fit: cover;
  display: block;
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

.row-rename-input {
  flex: 1;
  min-width: 0;
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
  background: var(--bg-primary, var(--surface-section));
  border: 1px solid var(--accent-color);
  border-radius: 6px;
  padding: 3px 8px;
}

.row-name-btn {
  flex-shrink: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  border: 1px solid transparent;
  border-radius: 6px;
  background: transparent;
  color: var(--text-muted);
  cursor: pointer;
  font-size: 12px;
  transition: all 0.15s ease;
}

.row-name-btn:hover {
  color: var(--accent-color);
  background: color-mix(in srgb, var(--accent-color) 12%, transparent);
}

.row-name-btn .pi-check {
  color: #16a34a;
}

.quality-chip {
  color: var(--text-primary);
  font-weight: 700;
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

.row-destination {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
  margin-top: 5px;
  font-size: 11px;
  color: var(--text-muted);
}

.row-destination i {
  color: #64748b;
  font-size: 12px;
}

.destination-path {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.destination-btn {
  flex-shrink: 0;
  border: 1px solid rgba(126, 139, 164, 0.22);
  border-radius: 7px;
  padding: 3px 8px;
  background: color-mix(in srgb, var(--bg-card) 92%, var(--accent-color));
  color: var(--text-primary);
  font-size: 10px;
  font-weight: 700;
  cursor: pointer;
}

.destination-btn:hover {
  border-color: color-mix(in srgb, var(--accent-color) 36%, var(--border-color));
  color: var(--accent-color);
}

.hash-chip {
  color: var(--text-primary);
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
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

.youtube-options {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 12px;
  border: 1px solid color-mix(in srgb, #ff0000 18%, var(--border-color));
  border-radius: 12px;
  background: color-mix(in srgb, #ff0000 4%, var(--bg-primary));
}

.youtube-option-grid {
  display: grid;
  grid-template-columns: minmax(220px, 1fr) minmax(120px, 180px);
  gap: 10px;
}

.youtube-field {
  display: flex;
  flex-direction: column;
  gap: 5px;
  min-width: 0;
}

.youtube-field > span,
.youtube-extra-group > span {
  font-size: 11px;
  font-weight: 800;
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: 0.4px;
}

.youtube-select {
  width: 100%;
  height: 32px;
  border: 1px solid var(--border-color);
  border-radius: 8px;
  background: var(--bg-card);
  color: var(--text-primary);
  font-size: 12px;
  padding: 0 9px;
  outline: none;
}

.youtube-select:focus {
  border-color: var(--accent-color);
  box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent-color) 16%, transparent);
}

.youtube-detail {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--text-muted);
  font-size: 11px;
}

.youtube-extra-groups {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
}

.youtube-extra-group {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}

.youtube-check-option {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  min-height: 28px;
  padding: 0 9px;
  border: 1px solid color-mix(in srgb, var(--border-color) 80%, transparent);
  border-radius: 8px;
  background: var(--bg-card);
  color: var(--text-secondary);
  font-size: 12px;
  font-weight: 600;
}

.youtube-check-option input {
  accent-color: var(--accent-color);
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

/* ── Filter bar ──────────────────────────────────────────────────────────── */
.filter-bar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
  padding: 10px 14px;
  border-bottom: 1px solid rgba(126, 139, 164, 0.16);
  position: relative;
  background: var(--bg-card);
}

.filter-search {
  flex: 1;
  min-width: 160px;
  background: color-mix(in srgb, var(--bg-primary) 80%, var(--bg-card));
  border: 1px solid var(--border-color);
  border-radius: 8px;
  color: var(--text-primary);
  font-size: 12px;
  padding: 6px 10px;
  outline: none;
  transition: border-color 0.15s;
}

.filter-search::placeholder {
  color: var(--text-muted);
}

.filter-search:focus {
  border-color: var(--accent-color);
}

.filter-chips {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
}

.filter-chip {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 5px 10px;
  border-radius: 999px;
  font-size: 11px;
  font-weight: 600;
  color: var(--text-muted);
  background: rgba(126, 139, 164, 0.1);
  border: 1px solid transparent;
  cursor: pointer;
  transition: background 0.15s, border-color 0.15s, color 0.15s;
  user-select: none;
}

.filter-chip input[type="checkbox"] {
  display: none;
}

.filter-chip:hover {
  background: rgba(126, 139, 164, 0.18);
  color: var(--text-primary);
}

.filter-chip.active {
  background: color-mix(in srgb, var(--accent-color) 14%, transparent);
  border-color: color-mix(in srgb, var(--accent-color) 40%, transparent);
  color: var(--accent-color);
}

.filter-hosts-btn {
  border: 1px solid rgba(126, 139, 164, 0.22);
  background: var(--bg-card);
}

.filter-hosts-btn.active {
  background: color-mix(in srgb, var(--accent-color) 14%, transparent);
  border-color: color-mix(in srgb, var(--accent-color) 40%, transparent);
  color: var(--accent-color);
}

.filter-clear-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  border-radius: 999px;
  border: 1px solid rgba(239, 83, 80, 0.3);
  background: rgba(239, 83, 80, 0.08);
  color: #d64541;
  cursor: pointer;
  font-size: 10px;
  transition: background 0.15s;
  flex-shrink: 0;
}

.filter-clear-btn:hover {
  background: rgba(239, 83, 80, 0.16);
}

.filter-size {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  flex-shrink: 0;
}

.filter-size-input {
  width: 64px;
  background: color-mix(in srgb, var(--bg-primary) 80%, var(--bg-card));
  border: 1px solid var(--border-color);
  border-radius: 6px;
  color: var(--text-primary);
  font-size: 11px;
  padding: 5px 6px;
  outline: none;
  transition: border-color 0.15s;
  text-align: center;
}

.filter-size-input::placeholder {
  color: var(--text-muted);
  font-size: 10px;
}

.filter-size-input:focus {
  border-color: var(--accent-color);
}

.filter-size-sep {
  color: var(--text-muted);
  font-size: 12px;
}

.filter-size-label {
  font-size: 11px;
  color: var(--text-muted);
  font-weight: 600;
}

.host-filter-dropdown {
  position: absolute;
  top: calc(100% + 4px);
  left: 14px;
  z-index: 100;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: 10px;
  padding: 6px;
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 180px;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.18);
}

.host-filter-item {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding: 7px 10px;
  border-radius: 7px;
  font-size: 12px;
  color: var(--text-secondary);
  cursor: pointer;
  transition: background 0.1s;
  user-select: none;
}

.host-filter-item:hover {
  background: rgba(126, 139, 164, 0.1);
}

.host-filter-item.active {
  background: color-mix(in srgb, var(--accent-color) 12%, transparent);
  color: var(--accent-color);
}

.host-filter-item input[type="checkbox"] {
  accent-color: var(--accent-color);
}

@media (max-width: 760px) {
  .youtube-option-grid {
    grid-template-columns: 1fr;
  }
}
</style>
