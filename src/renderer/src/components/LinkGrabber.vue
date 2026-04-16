<template>
  <div class="link-grabber">
    <div class="grabber-header">
      <h2 class="grabber-title">Capturador de Links</h2>
      <p class="grabber-sub">Cole links, aguarde a leitura e selecione o que vai para a fila.</p>
    </div>

    <div class="url-field">
      <label class="field-label">Links para capturar</label>
      <div class="textarea-wrapper">
        <i class="pi pi-link textarea-icon"></i>
        <textarea
          v-model="urlsInput"
          class="url-textarea"
          placeholder="https://mega.nz/file/...&#10;https://www.mediafire.com/folder/...&#10;https://pixeldrain.com/l/..."
          rows="5"
          @input="autoResize"
        ></textarea>
      </div>
    </div>

    <div v-if="rows.length > 0" class="captured-panel">
      <div class="status-chips">
        <span class="status-chip">
          Todos
          <strong>{{ rows.length }}</strong>
        </span>
        <span class="status-chip is-online">
          Online
          <strong>{{ onlineCount }}</strong>
        </span>
        <span class="status-chip is-folder">
          Pastas
          <strong>{{ folderCount }}</strong>
        </span>
        <span class="status-chip is-file">
          Arquivos
          <strong>{{ fileCount }}</strong>
        </span>
        <span class="status-chip is-duplicate">
          Duplicados
          <strong>{{ duplicateCount }}</strong>
        </span>
        <span class="status-chip is-error">
          Indisponível
          <strong>{{ errorCount }}</strong>
        </span>
      </div>

      <div class="captured-toolbar">
        <label class="master-check">
          <input
            type="checkbox"
            :checked="allSelectableChecked"
            :indeterminate="someSelectableChecked && !allSelectableChecked"
            @change="toggleAll"
          />
          <span>Selecionar tudo</span>
        </label>
        <span class="captured-meta">
          {{ selectedCount }} selecionado(s) · {{ availableCount }} disponível(is)
        </span>
      </div>

      <div class="captured-list">
        <div
          v-for="row in rows"
          :key="row.url"
          class="captured-row"
          :class="{ unavailable: !row.module || row.error }"
        >
          <div class="row-main">
            <label class="row-check">
              <input
                type="checkbox"
                v-model="row.selected"
                :disabled="!row.module || !!row.error || row.loading"
              />
            </label>

            <img
              class="row-icon"
              :src="getFileIcon(row.info?.name ?? row.displayName, row.info?.mimeType, row.info?.isFolder).src"
              :alt="getFileIcon(row.info?.name ?? row.displayName, row.info?.mimeType, row.info?.isFolder).alt"
              draggable="false"
            />

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
                  {{
                    row.loading
                      ? 'Lendo'
                      : row.error
                        ? 'Erro'
                        : row.info?.isFolder
                          ? 'Pasta'
                          : 'Arquivo'
                  }}
                </span>
              </div>
              <div class="row-sub">
                <span>{{ row.module?.name ?? 'Não suportado' }}</span>
                <template v-if="row.sourceLabels.length > 1">
                  <span>·</span>
                  <span>{{ row.sourceLabels.length }} fontes: {{ row.sourceLabels.join(', ') }}</span>
                </template>
                <span>·</span>
                <span v-if="row.loading">Lendo metadados...</span>
                <span v-else-if="row.error">{{ row.error }}</span>
                <span v-else-if="row.info">{{ fmtBytes(row.info.size) }}</span>
              </div>
            </div>

            <button
              v-if="(row.info?.isFolder && (row.info.children?.length ?? 0) > 0) || row.sourceUrls.length > 1"
              class="expand-btn"
              @click="row.expanded = !row.expanded"
            >
              <i class="pi" :class="row.expanded ? 'pi-chevron-up' : 'pi-chevron-down'"></i>
            </button>
          </div>

          <div v-if="row.expanded && ((row.info?.isFolder && row.info.children?.length) || row.sourceUrls.length > 1)" class="child-panel">
            <div v-if="row.sourceUrls.length > 1" class="source-panel">
              <div class="source-heading">Fontes disponíveis</div>
              <div
                v-for="(sourceUrl, idx) in row.sourceUrls"
                :key="`${row.url}:source:${sourceUrl}`"
                class="source-row"
              >
                <span class="source-provider">{{ row.sourceLabels[idx] ?? 'Servidor' }}</span>
                <span class="source-url" :title="sourceUrl">{{ truncateUrl(sourceUrl) }}</span>
              </div>
            </div>

            <div v-if="row.info?.isFolder && row.info.children?.length" class="children-list">
              <div v-for="child in row.info.children" :key="`${row.url}:${child.filename}`" class="child-row">
                <div class="child-name">
                  <img
                    class="child-icon"
                    :src="getFileIcon(child.filename, child.mimeType, child.isFolder).src"
                    :alt="getFileIcon(child.filename, child.mimeType, child.isFolder).alt"
                    draggable="false"
                  />
                  <span>{{ child.filename }}</span>
                </div>
                <span>{{ fmtBytes(child.size) }}</span>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>

    <p v-if="lastError" class="error-msg">
      <i class="pi pi-exclamation-triangle"></i>
      {{ lastError }}
    </p>

    <div class="actions-row" ref="actionsRef">
      <button class="btn-clear" :disabled="!urlsInput.trim() && rows.length === 0" @click="clear">
        <i class="pi pi-trash"></i>
        Limpar
      </button>
      <button
        class="btn-add"
        :disabled="selectedCount === 0 || adding"
        :class="{ loading: adding }"
        @click="addAll"
      >
        <i :class="adding ? 'pi pi-spin pi-spinner' : 'pi pi-plus'"></i>
        {{ adding ? 'Adicionando...' : `Adicionar ${selectedCount} selecionado(s)` }}
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from 'vue'
import { getFileIcon } from '../assets/file-icons'
import type { FileInfo } from '../../../shared/types'

interface ModuleSummary {
  id: string
  name: string
  icon: string
  color: string
}

interface CapturedRow {
  url: string
  displayName: string
  module: ModuleSummary | null
  info: FileInfo | null
  loading: boolean
  error: string
  selected: boolean
  expanded: boolean
  sourceUrls: string[]
  sourceLabels: string[]
}

const emit = defineEmits<{ (e: 'added'): void }>()

const urlsInput = ref('')
const rows = ref<CapturedRow[]>([])
const adding = ref(false)
const lastError = ref('')
const detectToken = ref(0)
const actionsRef = ref<HTMLElement | null>(null)

const selectableRows = computed(() => rows.value.filter((row) => row.module && !row.loading && !row.error))
const selectedRows = computed(() => selectableRows.value.filter((row) => row.selected))
const selectedCount = computed(() => selectedRows.value.length)
const availableCount = computed(() => selectableRows.value.length)
const allSelectableChecked = computed(() => selectableRows.value.length > 0 && selectedRows.value.length === selectableRows.value.length)
const someSelectableChecked = computed(() => selectedRows.value.length > 0)
const onlineCount = computed(() => rows.value.filter((row) => row.module && !row.loading && !row.error).length)
const errorCount = computed(() => rows.value.filter((row) => !!row.error).length)
const folderCount = computed(() => rows.value.filter((row) => row.info?.isFolder).length)
const fileCount = computed(() => rows.value.filter((row) => row.info && !row.info.isFolder).length)
const duplicateCount = computed(() => rows.value.filter((row) => row.sourceUrls.length > 1).length)

onMounted(async () => {
  await window.api.settings.load().catch(() => null)
})

watch(urlsInput, () => void detectProviders())

watch(
  () => rows.value.every((r) => !r.loading),
  (allDone) => {
    if (allDone && rows.value.length > 0) {
      nextTick(() => actionsRef.value?.scrollIntoView({ behavior: 'smooth', block: 'nearest' }))
    }
  }
)

function parseUrls(text: string): string[] {
  const seen = new Set<string>()
  for (const line of text.split('\n')) {
    const url = normalizeUrlCandidate(line)
    if (url) seen.add(url)
  }
  return [...seen]
}

function normalizeUrlCandidate(line: string): string {
  const trimmed = line.trim()
  if (!trimmed) return ''
  const match = trimmed.match(/https?:\/\/\S+/i)
  if (!match) return ''
  return match[0].replace(/[),.;]+$/, '')
}

async function detectProviders(): Promise<void> {
  const token = ++detectToken.value
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
    selected: true,
    expanded: false,
    sourceUrls: [url],
    sourceLabels: [],
  }))

  await Promise.all(
    rows.value.map(async (row, index) => {
      try {
        const module = await window.api.modules.detect(row.url)
        if (token !== detectToken.value) return
        rows.value[index].module = module

        if (!module) {
          rows.value[index].loading = false
          rows.value[index].error = 'Servidor não suportado'
          rows.value[index].selected = false
          return
        }

        const info = await window.api.modules.fileInfo(module.id, row.url)
        if (token !== detectToken.value) return
        rows.value[index].info = info
        rows.value[index].displayName = info.name
        rows.value[index].loading = false
      } catch (error) {
        if (token !== detectToken.value) return
        rows.value[index].loading = false
        rows.value[index].selected = false
        rows.value[index].error = error instanceof Error ? error.message : String(error)
      }
    })
  )

  if (token === detectToken.value) {
    rows.value = groupDuplicateRows(rows.value)
  }
}

async function addAll(): Promise<void> {
  if (selectedRows.value.length === 0 || adding.value) return
  adding.value = true
  lastError.value = ''
  let addedCount = 0
  const current = await window.api.settings.load().catch(() => null)
  const outputDir = current?.outputDir ?? '~/Downloads'

  for (const row of selectedRows.value) {
    if (!row.module || !row.info) continue
    try {
      await window.api.downloads.add(row.url, row.module.id, row.info.name, row.info.size, outputDir)
      addedCount += 1
    } catch (err) {
      lastError.value = `Erro ao adicionar: ${truncateUrl(row.url)} — ${err instanceof Error ? err.message : String(err)}`
    }
  }

  adding.value = false
  if (addedCount > 0) {
    urlsInput.value = ''
    rows.value = []
    emit('added')
  }
}

function toggleAll(event: Event): void {
  const checked = (event.target as HTMLInputElement).checked
  for (const row of selectableRows.value) {
    row.selected = checked
  }
}

function clear(): void {
  urlsInput.value = ''
  rows.value = []
  lastError.value = ''
}

function autoResize(event: Event): void {
  const el = event.target as HTMLTextAreaElement
  el.style.height = 'auto'
  el.style.height = Math.min(el.scrollHeight, 200) + 'px'
}

function truncateUrl(url: string): string {
  try {
    const u = new URL(url)
    const parts = u.pathname.split('/').filter(Boolean)
    const last = parts[parts.length - 1] || u.hostname
    return last.length > 44 ? last.slice(0, 41) + '...' : last
  } catch {
    return url.length > 50 ? url.slice(0, 47) + '...' : url
  }
}

function groupDuplicateRows(inputRows: CapturedRow[]): CapturedRow[] {
  const grouped = new Map<string, CapturedRow>()

  for (const row of inputRows) {
    const info = row.info
    const key = info
      ? `${(info.name ?? row.displayName).toLowerCase()}::${info.size}::${info.isFolder ? 'folder' : 'file'}`
      : `url::${row.url}`

    const existing = grouped.get(key)
    const sourceLabel = row.module?.name ?? 'Não suportado'

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

    if (!existing.module && row.module) existing.module = row.module
    if ((!existing.info || !existing.info.children?.length) && row.info) existing.info = row.info
    if (!existing.error && row.error) existing.error = row.error
  }

  for (const row of grouped.values()) {
    row.url = row.sourceUrls[0]
  }

  return [...grouped.values()]
}

function fmtBytes(n: number): string {
  if (!n || n <= 0) return '0 B'
  if (n < 1024) return `${n} B`
  if (n < 1048576) return `${(n / 1024).toFixed(1)} KB`
  if (n < 1073741824) return `${(n / 1048576).toFixed(1)} MB`
  return `${(n / 1073741824).toFixed(2)} GB`
}
</script>

<style scoped>
.link-grabber {
  padding: 24px 28px;
  display: flex;
  flex-direction: column;
  gap: 20px;
  max-width: 980px;
  width: 100%;
  min-width: 0;
  flex: 1;
  min-height: 0;
  overflow: hidden;
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
}

.row-copy {
  min-width: 0;
  flex: 1;
}

.row-title-line {
  display: flex;
  align-items: center;
  gap: 8px;
}

.row-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
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
  gap: 6px;
  margin-top: 3px;
  font-size: 11px;
  color: var(--text-muted);
}

.expand-btn {
  width: 28px;
  height: 28px;
  border: 1px solid rgba(126, 139, 164, 0.22);
  border-radius: 8px;
  background: #fff;
  color: #66758a;
  cursor: pointer;
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
}

.child-row {
  min-height: 30px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.child-row + .child-row {
  border-top: 1px solid rgba(126, 139, 164, 0.1);
}

.child-name {
  display: flex;
  align-items: center;
  gap: 8px;
}

.child-icon {
  width: 17px;
  height: 17px;
}

.error-msg {
  margin: 0;
  color: #ef5350;
  font-size: 12px;
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
