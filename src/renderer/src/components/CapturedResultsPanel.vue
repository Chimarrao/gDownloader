<template>
  <div class="captured-panel">
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

    <div class="captured-list">
      <div
        v-for="row in filteredRows"
        :key="row.url"
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
            class="row-icon"
            :class="getFileIcon(row.info?.name ?? row.displayName, row.info?.mimeType, row.info?.isFolder).className"
            :aria-label="getFileIcon(row.info?.name ?? row.displayName, row.info?.mimeType, row.info?.isFolder).alt"
            role="img"
          ></span>

          <div class="row-copy">
            <div class="row-title-line">
              <div class="row-title">{{ row.info?.name ?? row.displayName }}</div>
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
              <span v-else-if="row.info">{{ fmtBytes(row.info.size) }}</span>
              <template v-if="row.expectedHash">
                <span>·</span>
                <span class="hash-chip">{{ row.expectedHash.algorithm.toUpperCase() }} {{ row.expectedHash.value }}</span>
              </template>
            </div>
          </div>

          <div class="row-actions">
            <button
              v-if="canSearchMirrors(row)"
              class="row-action-btn is-mirror"
              :class="{ 'is-active': activeMirrorRowUrl === row.url }"
              :title="t('linkGrabberOpenMirrorsTitle')"
              @click="emit('open-mirrors', row)"
            >
              <i class="pi pi-sitemap"></i>
            </button>
            <button
              v-if="(row.info?.isFolder && (row.info.children?.length ?? 0) > 0) || row.sourceUrls.length > 1"
              class="row-action-btn expand-btn"
              :title="row.expanded ? t('closeDetails') : t('openDetails')"
              @click="emit('toggle-expanded', row)"
            >
              <i class="pi" :class="row.expanded ? 'pi-chevron-up' : 'pi-chevron-down'"></i>
            </button>
          </div>
        </div>

        <div v-if="row.expanded && ((row.info?.isFolder && row.info.children?.length) || row.sourceUrls.length > 1)" class="child-panel">
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

          <div v-if="row.info?.isFolder && row.info.children?.length" class="children-list">
            <div v-if="supportsChildSelection(row)" class="children-toolbar">
              <label class="tree-master-check">
                  <input
                    type="checkbox"
                    :checked="isRowChecked(row)"
                    :indeterminate.prop="isRowIndeterminate(row)"
                    @change="onToggleRow(row, $event)"
                  />
                <span>{{ t('linkGrabberSelectAllFolder') }}</span>
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
    </div>
  </div>
</template>

<script setup lang="ts">
import type { PropType } from 'vue'
import { ref, computed, watch } from 'vue'

import { useI18n } from '../i18n'
import { getFileIcon } from '../assets/file-icons'
import type { DerivedChildNode } from '../utils/child-tree'
import VirtualRows from './VirtualRows.vue'
import type { CapturedRow, SelectableChild } from './link-grabber-model'

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
  (e: 'toggle-expanded', row: CapturedRow): void
  (e: 'open-mirrors', row: CapturedRow): void
  (e: 'filtered-change', urls: string[]): void
}>()

const { t } = useI18n()

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

watch(
  filteredRows,
  (rows) => emit('filtered-change', rows.map((row) => row.url)),
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
</style>
