<template>
  <div class="download-list">
    <!-- Empty state -->
    <div v-if="items.length === 0 && (skeletonCount ?? 0) === 0" class="empty-state">
      <div class="empty-icon">
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48" fill="none" width="48" height="48">
          <circle cx="24" cy="24" r="22" stroke="currentColor" stroke-width="1.5" opacity="0.3"/>
          <path d="M24 14 L24 30 M18 25 L24 32 L30 25" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
          <path d="M16 36 H32" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
        </svg>
      </div>
      <p class="empty-title">Nenhum download ativo</p>
      <p class="empty-sub">Cole um link, arraste um arquivo ou importe um container para começar</p>
      <div class="empty-actions">
        <button class="empty-primary" @click="$emit('open-grabber')">
          <i class="pi pi-link"></i>
          Capturar links
        </button>
        <button class="empty-secondary" @click="chooseOutputDir">
          <i class="pi pi-folder"></i>
          Pasta destino
        </button>
        <button class="empty-secondary" @click="hydrate">
          <i class="pi pi-refresh"></i>
          Atualizar
        </button>
      </div>
    </div>

    <aside v-if="items.length > 0" class="package-sidebar" aria-label="Pacotes">
      <button
        class="package-node"
        :class="{ active: selectedPackageId === 'all' }"
        @click="selectedPackageId = 'all'"
        @dragover.prevent
        @drop="assignDraggedToPackage('')"
      >
        <span class="package-status-dot ok"></span>
        <span class="package-node-label">Todos</span>
        <span class="package-node-count">{{ items.length }}</span>
      </button>
      <button
        class="package-node"
        :class="{ active: selectedPackageId === 'unassigned' }"
        @click="selectedPackageId = 'unassigned'"
        @dragover.prevent
        @drop="assignDraggedToPackage('')"
      >
        <span class="package-status-dot" :class="packageAggregateClass('')"></span>
        <span class="package-node-label">Sem pacote</span>
        <span class="package-node-count">{{ packageStats('').total }}</span>
      </button>
      <div class="package-tree">
        <button
          v-for="pkg in packages"
          :key="pkg.id"
          class="package-node"
          :class="{ active: selectedPackageId === pkg.id }"
          @click="selectedPackageId = pkg.id"
          @dragover.prevent
          @drop="assignDraggedToPackage(pkg.id)"
        >
          <span
            class="package-color-dot"
            :style="{ backgroundColor: pkg.color }"
          ></span>
          <span class="package-node-label">{{ pkg.name }}</span>
          <span class="package-status-dot" :class="packageAggregateClass(pkg.id)"></span>
          <span class="package-node-count">{{ packageStats(pkg.id).total }}</span>
        </button>
      </div>
    </aside>

    <!-- Download items -->
      <div
        v-if="items.length > 0 || (skeletonCount ?? 0) > 0"
        ref="itemsContainerRef"
        class="items-container"
        @scroll="onListScroll"
      >
      <div
        v-if="items.length === 0"
        v-for="i in (skeletonCount ?? 0)"
        :key="`skeleton-${i}`"
        class="download-card skeleton-card"
      >
        <div class="skeleton-line skeleton-title"></div>
        <div class="skeleton-line skeleton-progress"></div>
        <div class="skeleton-line skeleton-meta"></div>
      </div>

      <div v-if="items.length > 0" class="list-toolbar">
        <span class="list-count">{{ orderedItems.length }} item(ns) na sessão</span>
        <div class="toolbar-actions">
          <button
            class="toolbar-btn"
            title="Criar pacote"
            @click="createPackage"
          >
            Novo pacote
          </button>
          <div class="columns-menu-wrap">
            <button class="toolbar-btn" title="Mostrar/esconder colunas" @click="showColumnsMenu = !showColumnsMenu">
              Colunas
            </button>
            <div v-if="showColumnsMenu" class="columns-menu">
              <label
                v-for="column in columnOrder"
                :key="column"
                class="column-option"
                draggable="true"
                @dragstart="draggedColumn = column"
                @dragover.prevent
                @drop="dropColumn(column)"
              >
                <input
                  type="checkbox"
                  :checked="visibleColumns.includes(column)"
                  @change="toggleColumn(column)"
                />
                <span>{{ columnLabel(column) }}</span>
              </label>
            </div>
          </div>
          <label class="toolbar-sort">
            <span>Status</span>
            <select v-model="filterStatuses" class="toolbar-select" multiple size="1" @change="persistFilters">
              <option v-for="status in statusFilterOptions" :key="status" :value="status">{{ status }}</option>
            </select>
          </label>
          <label class="toolbar-sort">
            <span>Host</span>
            <select v-model="filterHosts" class="toolbar-select" multiple size="1" @change="persistFilters">
              <option v-for="host in hostFilterOptions" :key="host" :value="host">{{ host }}</option>
            </select>
          </label>
          <label class="toolbar-sort">
            <span>Pacote</span>
            <select v-model="filterPackages" class="toolbar-select" multiple size="1" @change="persistFilters">
              <option value="">Sem pacote</option>
              <option v-for="pkg in packages" :key="pkg.id" :value="pkg.id">{{ pkg.name }}</option>
            </select>
          </label>
          <button
            v-if="hasActiveListFilters"
            class="toolbar-btn"
            title="Limpar filtros"
            @click="clearListFilters"
          >
            Limpar filtros
          </button>
          <label class="toolbar-sort">
            <span>Ordenar</span>
            <select v-model="sortMode" class="toolbar-select">
              <option
                v-for="option in sortOptions"
                :key="option.value"
                :value="option.value"
              >
                {{ option.label }}
              </option>
            </select>
          </label>
          <button
            class="toolbar-btn"
            :disabled="finishedCount === 0"
            title="Remover downloads encerrados da lista"
            @click="clearFinished"
          >
            Limpar concluídos
          </button>
        </div>
      </div>
      <div class="items-stack">
        <div v-if="virtualizationEnabled && topSpacerHeight > 0" :style="{ height: `${topSpacerHeight}px` }"></div>
        <div
          v-for="item in visibleItems"
          :key="item.id"
          class="download-card"
          :class="[`status-bg-${item.status}`, { 'status-flash': flashingIds.has(item.id), 'card-pinned': item.pinned }]"
          draggable="true"
          @dragstart="draggedDownloadId = item.id"
          @dragend="draggedDownloadId = null"
        >
          <!-- Left: provider icon -->
          <div
            v-if="hasColumn('host')"
            class="provider-icon"
            v-html="getIcon(item.moduleId).svg"
            :title="moduleLabel(item.moduleId)"
          ></div>

          <!-- Center: info -->
          <div class="item-body">
            <!-- Row 1: filename + status + actions -->
            <div class="item-header">
              <div class="item-title-wrap">
                <template v-if="hasColumn('name')">
                <span
                  class="type-icon"
                  :class="getFileIcon(item.title || item.url, undefined, item.isFolder).className"
                  :aria-label="getFileIcon(item.title || item.url, undefined, item.isFolder).alt"
                  role="img"
                ></span>
                <span class="item-title" :title="item.title">{{ item.title || item.url }}</span>
                </template>
              </div>
              <div class="item-actions">
                <span v-if="hasColumn('status')" class="status-badge" :class="`badge-${item.status}`">
                  <span class="badge-dot" :class="`dot-${item.status}`"></span>
                  {{ statusTextValue(item) }}
                </span>
                <button
                  class="action-btn pin-btn"
                  :class="{ 'pin-btn-active': item.pinned }"
                  :title="item.pinned ? 'Desafixar' : 'Fixar no topo'"
                  @click="togglePin(item.id)"
                >
                  <i class="pi pi-star" :class="{ 'pi-star-fill': item.pinned }"></i>
                </button>
                <button
                  class="action-btn"
                  :title="item.isFolder ? 'Copiar URLs' : 'Copiar URL'"
                  @click="copyUrl(item)"
                >
                  <i class="pi pi-copy"></i>
                </button>
                <button
                  v-if="item.isFolder && (item.children?.length ?? 0) > 0"
                  class="action-btn"
                  :title="isExpanded(item.id) ? 'Ocultar itens' : 'Mostrar itens'"
                  @click="toggleFolder(item.id)"
                >
                  <i class="pi" :class="isExpanded(item.id) ? 'pi-chevron-up' : 'pi-chevron-down'"></i>
                </button>
                <button
                  v-if="actionsFor(item).canPause"
                  class="action-btn"
                  title="Pausar"
                  @click="pause(item.id)"
                >
                  <i class="pi pi-pause"></i>
                </button>
                <button
                  v-if="actionsFor(item).canResume"
                  class="action-btn"
                  title="Retomar"
                  @click="resume(item.id)"
                >
                  <i class="pi pi-play"></i>
                </button>
                <button
                  v-if="actionsFor(item).canOpenCaptcha"
                  class="action-btn"
                  title="Resolver captcha"
                  @click="openCaptcha(item.id)"
                >
                  <i class="pi pi-shield"></i>
                </button>
                <button
                  v-if="actionsFor(item).canCancel"
                  class="cancel-btn"
                  title="Cancelar"
                  @click="cancel(item.id)"
                >
                  <i class="pi pi-times"></i>
                </button>
                <button
                  v-if="actionsFor(item).canRetry"
                  class="action-btn"
                  title="Tentar novamente"
                  @click="retry(item.id)"
                >
                  <i class="pi pi-refresh"></i>
                </button>
                <button
                  v-if="actionsFor(item).canForce"
                  class="action-btn"
                  title="Forçar download agora"
                  @click="force(item.id)"
                >
                  <i class="pi pi-bolt"></i>
                </button>
                <button
                  v-if="actionsFor(item).canRestart"
                  class="action-btn"
                  :title="item.status === 'corrupted' ? 'Re-baixar' : 'Reiniciar'"
                  @click="restart(item.id)"
                >
                  <i class="pi pi-replay"></i>
                </button>
                <button
                  v-if="item.status === 'complete' && item.outputPath && isExtractableArchive(item.outputPath)"
                  class="action-btn"
                  title="Extrair"
                  @click="extract(item.outputPath!)"
                >
                  <i class="pi pi-folder-plus"></i>
                </button>
                <button
                  v-if="actionsFor(item).canOpenFolder"
                  class="open-btn"
                  title="Mostrar na pasta"
                  @click="openFolder(item.outputPath!)"
                >
                  <i class="pi pi-folder-open"></i>
                </button>
                <button
                  v-if="isTerminal(item.status)"
                  class="action-btn"
                  title="Remover da lista"
                  @click="remove(item.id)"
                >
                  <i class="pi pi-trash"></i>
                </button>
                <button
                  v-if="actionsFor(item).canRemoveWithFiles"
                  class="action-btn"
                  title="Remover da lista e apagar arquivos físicos"
                  @click="removeWithFiles(item.id)"
                >
                  <i class="pi pi-trash"></i>
                </button>
              </div>
            </div>

            <!-- Row 2: progress bar -->
            <div v-if="hasColumn('progress')" class="progress-track">
              <div
                class="progress-fill"
                :class="{ 'progress-shimmer': item.status === 'downloading' || item.status === 'verifying' }"
                :style="{
                  width: item.percent + '%',
                  background: getProgressColor(item)
                }"
              ></div>
            </div>

            <!-- Row 3: meta info -->
            <div class="item-meta">
              <span class="meta-percent">{{ item.percent }}%</span>

              <template v-if="item.status === 'downloading' && hasColumn('speed')">
                <span class="meta-sep">·</span>
                <span class="meta-speed">{{ formatSpeed(effectiveSpeedValue(item)) }}</span>
              </template>
              <template v-if="item.status === 'downloading' && hasColumn('eta')">
                <span class="meta-sep">·</span>
                <span class="meta-eta">{{ formatEta(effectiveEtaValue(item)) }} restante</span>
              </template>

              <template v-else-if="item.status === 'verifying'">
                <span class="meta-sep">·</span>
                <span class="meta-verifying">
                  <i class="pi pi-shield"></i>
                  Verificando {{ item.expectedHash?.algorithm?.toUpperCase() ?? 'hash' }}
                </span>
              </template>

              <template v-else-if="item.status === 'rate_limited'">
                <span class="meta-sep">·</span>
                <span class="meta-wait">
                  <i class="pi pi-clock"></i>
                  {{ item.retryAt && item.retryAt > nowTick ? formatEta(Math.ceil((item.retryAt - nowTick) / 1000)) + ' para desbloquear' : 'Bloqueado por limite de taxa' }}
                </span>
              </template>

              <template v-else-if="item.status === 'waiting_captcha'">
                <span class="meta-sep">·</span>
                <span class="meta-captcha-wait">
                  <i class="pi pi-shield"></i>
                  Aguardando resolução do captcha
                </span>
              </template>

              <template v-else-if="isWaitingRetryNow(item)">
                <span class="meta-sep">·</span>
                <span class="meta-wait">
                  <i class="pi pi-clock"></i>
                  {{ formatEta(retryCountdownNow(item)) }} para tentar novamente
                </span>
              </template>

              <template v-if="item.size > 0 && hasColumn('size')">
                <span class="meta-sep">·</span>
                <span class="meta-size">
                  {{ formatBytes(Math.floor((item.percent / 100) * item.size)) }}
                  / {{ formatBytes(item.size) }}
                </span>
              </template>

              <template v-if="item.isFolder && (item.children?.length ?? 0) > 0">
                <span class="meta-sep">·</span>
                <span class="meta-size">{{ item.children?.length }} item(ns)</span>
              </template>

              <template v-if="isWaitingRetryNow(item) && item.error">
                <span class="meta-sep">·</span>
                <span class="meta-wait-reason" :title="item.error">{{ item.error }}</span>
              </template>

              <template v-else-if="item.status === 'disk_full'">
                <span class="meta-sep">·</span>
                <span class="meta-disk-full">
                  <i class="pi pi-database"></i>
                  Espaço em disco insuficiente
                </span>
              </template>

              <template v-else-if="(item.status === 'error' || item.status === 'corrupted') && item.error">
                <span class="meta-sep">·</span>
                <span class="meta-error" :title="item.error">{{ item.error }}</span>
              </template>

              <template v-if="(item.maxRetries ?? 0) > 0">
                <span class="meta-sep">·</span>
                <span class="meta-retries">
                  tentativa {{ (item.retryCount ?? 0) + 1 }}/{{ (item.maxRetries ?? 0) + 1 }}
                </span>
              </template>

              <template v-if="packages.length > 0 && hasColumn('package')">
                <span class="meta-sep">·</span>
                <label class="package-picker">
                  <span
                    class="package-dot"
                    :style="{ backgroundColor: packageColor(item.packageId) }"
                  ></span>
                  <select
                    :value="item.packageId ?? ''"
                    title="Mover para pacote"
                    @change="assignPackage(item, ($event.target as HTMLSelectElement).value)"
                  >
                    <option value="">Sem pacote</option>
                    <option
                      v-for="pkg in packages"
                      :key="pkg.id"
                      :value="pkg.id"
                    >
                      {{ pkg.name }}
                    </option>
                  </select>
                </label>
              </template>
              <template v-if="hasColumn('added')">
                <span class="meta-sep">·</span>
                <span class="meta-size">Adicionado {{ formatDateTime(item.addedAt) }}</span>
              </template>
              <template v-if="item.completedAt && hasColumn('completed')">
                <span class="meta-sep">·</span>
                <span class="meta-size">Concluído {{ formatDateTime(item.completedAt) }}</span>
              </template>
              <template v-if="item.expectedHash && hasColumn('hash')">
                <span class="meta-sep">·</span>
                <span class="meta-size">{{ item.expectedHash.algorithm.toUpperCase() }}</span>
              </template>
            </div>

            <!-- ErrorState for specific error types -->
            <ErrorState
              v-if="item.status === 'disk_full'"
              title="Disco cheio"
              :description="item.error || 'Não há espaço suficiente para salvar o arquivo.'"
              icon="pi pi-database"
              :actions="[
                { label: 'Trocar pasta', icon: 'pi pi-folder', variant: 'primary', handler: () => chooseOutputDir() },
                { label: 'Tentar novamente', icon: 'pi pi-refresh', variant: 'secondary', handler: () => retry(item.id) },
              ]"
            />
            <ErrorState
              v-else-if="item.status === 'error' && item.error"
              :title="item.error"
              icon="pi pi-exclamation-circle"
              :actions="[
                { label: 'Tentar novamente', icon: 'pi pi-refresh', variant: 'secondary', handler: () => retry(item.id) },
              ]"
            />

            <!-- Row 5: output path (clickable) -->
            <div
              v-if="item.outputPath"
              class="item-path"
              :title="item.outputPath"
              @click="openFolder(item.outputPath!)"
            >
              <i class="pi pi-folder" style="font-size: 10px;"></i>
              {{ item.outputPath }}
            </div>

            <div
              v-show="item.isFolder && isExpanded(item.id) && (item.children?.length ?? 0) > 0"
              class="folder-children"
            >
              <div class="folder-tree-header">
                <span>Árvore de arquivos</span>
                <span>{{ item.children?.length ?? 0 }} item(ns)</span>
              </div>
              <div
                v-for="node in childNodes(item.children)"
                :key="`${item.id}:${node.key}`"
                class="child-row"
                :class="{ 'is-folder-row': node.isFolder }"
              >
                <div class="child-main">
                  <div class="child-name" :style="{ paddingInlineStart: `${node.depth * 18}px` }">
                    <span
                      class="child-icon"
                      :class="getFileIcon(node.name, node.mimeType, node.isFolder).className"
                      :aria-label="getFileIcon(node.name, node.mimeType, node.isFolder).alt"
                      role="img"
                    ></span>
                    <span class="child-name-label">{{ node.name }}</span>
                    <span v-if="node.isFolder" class="child-folder-badge">{{ node.fileCount }} item(ns)</span>
                  </div>
                  <div class="child-meta">
                    <span class="child-status">{{ childStatusText(node.status) }}</span>
                    <span class="meta-sep">·</span>
                    <span>{{ childPercent(node) }}%</span>
                    <template v-if="(node.speedBps ?? 0) > 0">
                      <span class="meta-sep">·</span>
                      <span class="meta-speed">{{ formatSpeed(node.speedBps ?? 0) }}</span>
                    </template>
                    <template v-if="(node.etaSec ?? 0) > 0">
                      <span class="meta-sep">·</span>
                      <span>{{ formatEta(node.etaSec ?? 0) }}</span>
                    </template>
                    <template v-if="node.isFolder">
                      <span class="meta-sep">·</span>
                      <span>{{ node.fileCount }} arquivo(s)</span>
                    </template>
                  </div>
                  <div class="child-track">
                    <div
                      class="child-fill"
                      :style="{ width: `${childPercent(node)}%` }"
                    ></div>
                  </div>
                </div>
                <span class="child-size">{{ formatBytes(node.size) }}</span>
              </div>
            </div>
          </div>
        </div>
        <div v-if="virtualizationEnabled && bottomSpacerHeight > 0" :style="{ height: `${bottomSpacerHeight}px` }"></div>
      </div>
    </div>

    <div v-if="activeCaptchaItem" class="captcha-modal-backdrop" @click.self="closeCaptchaModal">
      <div
        ref="captchaModalRef"
        class="captcha-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="captcha-dialog-title"
        tabindex="-1"
        @keydown="onCaptchaDialogKeydown"
      >
        <div class="captcha-modal-header">
          <div>
            <strong id="captcha-dialog-title">Captcha necessário</strong>
            <p>Se houver NoPecha configurado, o app tenta antes. Se não, o captcha abre na própria página do host.</p>
          </div>
          <button class="action-btn" title="Fechar" @click="closeCaptchaModal">
            <i class="pi pi-times"></i>
          </button>
        </div>

        <div class="captcha-modal-file" :title="activeCaptchaItem.title">
          {{ activeCaptchaItem.title || activeCaptchaItem.url }}
        </div>
        <div class="captcha-modal-body">
          <p>
            O app abre uma janela modal do próprio host para evitar bloqueios de domínio do captcha.
            Resolva lá e a fila continua sozinha.
          </p>
          <button
            ref="captchaPrimaryButtonRef"
            class="captcha-modal-open-btn"
            :disabled="captchaWindowBusy"
            @click="reopenCaptchaWindow"
          >
            <i class="pi" :class="captchaWindowBusy ? 'pi-spin pi-spinner' : 'pi-external-link'"></i>
            {{ captchaWindowBusy ? 'Aguardando resolução...' : 'Abrir janela do host' }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { DownloadStatus as DownloadStatusEnum } from '../../../shared/constants'
import type { DownloadChild, DownloadItem, DownloadPackage } from '../../../shared/types'
import { getFileIcon } from '../assets/file-icons'
import { getProviderIcon, getProviderColor } from '../assets/provider-icons'
import { buildChildTree, flattenChildTree, type DerivedChildNode } from '../utils/child-tree'
import {
  childStatusText,
  compareDownloads,
  DOWNLOAD_SORT_OPTIONS,
  effectiveEta,
  effectiveSpeed,
  getDownloadActions,
  isTerminal,
  isWaitingRetry,
  retryCountdown,
  statusText,
  type DownloadSortMode,
} from '../utils/download-display'
import { formatBytes, formatEta, formatSpeed } from '../utils/format'
import { focusFirstDialogElement, trapDialogTab } from '../utils/dialog-focus'
import ErrorState from './ErrorState.vue'

interface ModuleSummary {
  id: string
  name: string
  color: string
}

// ── Props ──────────────────────────────────────────────────
const props = withDefaults(defineProps<{ skeletonCount?: number }>(), {
  skeletonCount: 0
})
const skeletonCount = computed(() => props.skeletonCount)

// ── Emits ──────────────────────────────────────────────────
const emit = defineEmits<{
  (e: 'count-change', count: number): void
  (e: 'download-complete', payload: { id: string; outputPath: string; url?: string; title?: string; sha256Hash?: string }): void
  (e: 'global-speed', bps: number): void
  (e: 'skeleton-done'): void
  (e: 'open-grabber'): void
}>()

// ── State ──────────────────────────────────────────────────
const items = ref<DownloadItem[]>([])
const packages = ref<DownloadPackage[]>([])
const selectedPackageId = ref<'all' | 'unassigned' | string>('all')
const draggedDownloadId = ref<string | null>(null)
const defaultColumns = ['status', 'name', 'size', 'progress', 'speed', 'eta', 'host', 'package', 'added', 'completed', 'hash']
const columnOrder = ref<string[]>([...defaultColumns])
const visibleColumns = ref<string[]>([...defaultColumns])
const showColumnsMenu = ref(false)
const draggedColumn = ref<string | null>(null)
const filterStatuses = ref<string[]>([])
const filterHosts = ref<string[]>([])
const filterPackages = ref<string[]>([])
const modulesById = ref<Record<string, ModuleSummary>>({})
const expandedFolders = ref<Record<string, boolean>>({})
const activeCaptchaId = ref<string | null>(null)
const captchaWindowBusy = ref(false)
const captchaModalRef = ref<HTMLElement | null>(null)
const captchaPrimaryButtonRef = ref<HTMLElement | null>(null)
const sortMode = ref<DownloadSortMode>('newest')
const itemsContainerRef = ref<HTMLElement | null>(null)
const listScrollTop = ref(0)
const unsubs: Array<() => void> = []
const captchaAttemptedIds = new Set<string>()
const itemIndexById = ref<Record<string, number>>({})
// Mutex: at most one hydrate() runs at a time; hydrateQueued ensures one follow-up
// run executes after the in-flight one finishes.
let hydrateQueued = false
let hydrateInFlight = false
let isMounted = false
let lastSpeedEmit = 0
const nowTick = ref(Date.now())
let retryTimer: number | null = null
let hydrateTimer: number | null = null
const sortOptions = DOWNLOAD_SORT_OPTIONS
// Set of download IDs that recently changed status (for flash animation)
const flashingIds = ref<Set<string>>(new Set())

// ── Computed ───────────────────────────────────────────────
const packageFilteredItems = computed(() => {
  let base = items.value
  if (selectedPackageId.value === 'unassigned') {
    base = base.filter((item) => !item.packageId)
  } else if (selectedPackageId.value !== 'all') {
    base = base.filter((item) => item.packageId === selectedPackageId.value)
  }
  if (filterStatuses.value.length > 0) {
    base = base.filter((item) => filterStatuses.value.includes(item.status))
  }
  if (filterHosts.value.length > 0) {
    base = base.filter((item) => filterHosts.value.includes(moduleLabel(item.moduleId)))
  }
  if (filterPackages.value.length > 0) {
    base = base.filter((item) => filterPackages.value.includes(item.packageId ?? ''))
  }
  return base
})

const statusFilterOptions = ['pending', 'downloading', 'paused', 'complete', 'error', 'waiting_captcha', 'rate_limited']
const hostFilterOptions = computed(() => {
  const hosts = new Set<string>()
  for (const item of items.value) hosts.add(moduleLabel(item.moduleId))
  return [...hosts].sort((a, b) => a.localeCompare(b))
})
const hasActiveListFilters = computed(() =>
  filterStatuses.value.length > 0 || filterHosts.value.length > 0 || filterPackages.value.length > 0
)

const orderedItems = computed(() =>
  [...packageFilteredItems.value].sort((left, right) => {
    // Pinned items float to the top
    const pinnedDiff = (right.pinned ? 1 : 0) - (left.pinned ? 1 : 0)
    if (pinnedDiff !== 0) return pinnedDiff
    return compareDownloads(left, right, sortMode.value, nowTick.value)
  })
)
const packageStatsMap = computed(() => {
  const stats = new Map<string, { total: number; active: number; failed: number; complete: number }>()
  const ensure = (id: string) => {
    if (!stats.has(id)) stats.set(id, { total: 0, active: 0, failed: 0, complete: 0 })
    return stats.get(id)!
  }
  for (const item of items.value) {
    const id = item.packageId ?? ''
    const stat = ensure(id)
    stat.total += 1
    if (item.status === 'error' || item.status === 'corrupted' || item.status === 'disk_full') stat.failed += 1
    else if (item.status === 'complete') stat.complete += 1
    else if (!isTerminal(item.status)) stat.active += 1
  }
  return stats
})
const finishedCount = computed(() =>
  items.value.filter((item) => isTerminal(item.status)).length
)
const activeCaptchaItem = computed(() =>
  items.value.find((item) => item.id === activeCaptchaId.value && item.status === DownloadStatusEnum.WaitingCaptcha) ?? null
)

function packageStats(packageId: string): { total: number; active: number; failed: number; complete: number } {
  return packageStatsMap.value.get(packageId) ?? { total: 0, active: 0, failed: 0, complete: 0 }
}

function packageAggregateClass(packageId: string): string {
  const stats = packageStats(packageId)
  if (stats.failed > 0) return 'failed'
  if (stats.active > 0) return 'active'
  if (stats.total > 0 && stats.complete === stats.total) return 'ok'
  return 'idle'
}

async function assignDraggedToPackage(packageId: string): Promise<void> {
  if (!draggedDownloadId.value) return
  const item = items.value.find((entry) => entry.id === draggedDownloadId.value)
  if (!item) return
  await assignPackage(item, packageId)
}

function hasColumn(column: string): boolean {
  return visibleColumns.value.includes(column)
}

function columnLabel(column: string): string {
  return {
    status: 'Status',
    name: 'Nome',
    size: 'Tamanho',
    progress: 'Progresso',
    speed: 'Speed',
    eta: 'ETA',
    host: 'Host',
    package: 'Pacote',
    added: 'Adicionado',
    completed: 'Concluído',
    hash: 'Hash',
  }[column] ?? column
}

async function persistVisibleColumns(): Promise<void> {
  const settings = await window.api.settings.load().catch(() => null)
  if (!settings) return
  await window.api.settings.save({
    ...settings,
    visibleColumns: columnOrder.value.filter((column) => visibleColumns.value.includes(column)),
  }).catch(() => null)
}

function toggleColumn(column: string): void {
  if (visibleColumns.value.includes(column)) {
    visibleColumns.value = visibleColumns.value.filter((entry) => entry !== column)
  } else {
    visibleColumns.value = [...visibleColumns.value, column]
  }
  void persistVisibleColumns()
}

function dropColumn(target: string): void {
  if (!draggedColumn.value || draggedColumn.value === target) return
  const next = columnOrder.value.filter((column) => column !== draggedColumn.value)
  const targetIndex = next.indexOf(target)
  next.splice(targetIndex, 0, draggedColumn.value)
  columnOrder.value = next
  draggedColumn.value = null
  void persistVisibleColumns()
}

function formatDateTime(timestamp: number): string {
  return new Date(timestamp).toLocaleString('pt-BR', {
    day: '2-digit',
    month: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  })
}

async function persistFilters(): Promise<void> {
  const settings = await window.api.settings.load().catch(() => null)
  if (!settings) return
  await window.api.settings.save({
    ...settings,
    lastFilters: {
      statuses: filterStatuses.value,
      hosts: filterHosts.value,
      packages: filterPackages.value,
    },
  }).catch(() => null)
}

function clearListFilters(): void {
  filterStatuses.value = []
  filterHosts.value = []
  filterPackages.value = []
  void persistFilters()
}

const virtualizationEnabled = computed(() =>
  orderedItems.value.length > 40 && !Object.values(expandedFolders.value).some(Boolean)
)
const estimatedRowHeight = 148
const overscan = 6
const visibleRange = computed(() => {
  if (!virtualizationEnabled.value) {
    return { start: 0, end: orderedItems.value.length }
  }
  const viewportHeight = itemsContainerRef.value?.clientHeight ?? 900
  const start = Math.max(0, Math.floor(listScrollTop.value / estimatedRowHeight) - overscan)
  const visibleCount = Math.ceil(viewportHeight / estimatedRowHeight) + overscan * 2
  return {
    start,
    end: Math.min(orderedItems.value.length, start + visibleCount),
  }
})
const visibleItems = computed(() =>
  virtualizationEnabled.value
    ? orderedItems.value.slice(visibleRange.value.start, visibleRange.value.end)
    : orderedItems.value
)
const topSpacerHeight = computed(() =>
  virtualizationEnabled.value ? visibleRange.value.start * estimatedRowHeight : 0
)
const bottomSpacerHeight = computed(() =>
  virtualizationEnabled.value
    ? Math.max(0, (orderedItems.value.length - visibleRange.value.end) * estimatedRowHeight)
    : 0
)

// ── Lifecycle ──────────────────────────────────────────────
onMounted(async () => {
  isMounted = true
  retryTimer = window.setInterval(() => {
    nowTick.value = Date.now()
    const totalSpeed = items.value
      .filter((item) => item.status === DownloadStatusEnum.Downloading)
      .reduce((sum, item) => sum + effectiveSpeedValue(item), 0)
    emit('global-speed', totalSpeed)
  }, 1000)
  hydrateTimer = window.setInterval(() => {
    if (!isMounted) return
    const hasLiveQueue = items.value.some((item) => !isTerminal(item.status))
    if (!hasLiveQueue && items.value.length > 0) return
    void hydrate()
  }, 4000)

  // Load module metadata for labels
  const modules = await window.api.modules.list().catch(() => [])
  modulesById.value = modules.reduce<Record<string, ModuleSummary>>((acc, mod) => {
    acc[mod.id] = { id: mod.id, name: mod.name, color: mod.color }
    return acc
  }, {})

  packages.value = await window.api.packages.list().catch(() => [])
  const settings = await window.api.settings.load().catch(() => null)
  if (Array.isArray(settings?.visibleColumns) && settings.visibleColumns.length > 0) {
    const known = new Set(defaultColumns)
    visibleColumns.value = settings.visibleColumns.filter((column) => known.has(column))
    columnOrder.value = [
      ...settings.visibleColumns.filter((column) => known.has(column)),
      ...defaultColumns.filter((column) => !settings.visibleColumns?.includes(column)),
    ]
  }
  filterStatuses.value = settings?.lastFilters?.statuses ?? []
  filterHosts.value = settings?.lastFilters?.hosts ?? []
  filterPackages.value = settings?.lastFilters?.packages ?? []

  // Load existing downloads from backend
  await hydrate()

  // Real-time progress events
  unsubs.push(
    window.api.downloads.on('download:progress', (event: unknown) => {
      const ev = event as {
        type?: string
        id: string
        bytes?: number
        total?: number
        speed?: number
        eta?: number
        child_path?: string
        status?: string
        child_filename?: string
        child_bytes?: number
        child_total?: number
        child_speed?: number
        child_eta?: number
      }
      if (!ev?.id) return
      const idx = itemIndexById.value[ev.id] ?? -1
      if (idx >= 0) {
        const total = ev.total ?? items.value[idx].size
        const bytes = ev.bytes ?? 0
        let nextChildren = items.value[idx].children
        if (ev.child_filename && nextChildren?.length) {
          nextChildren = nextChildren.map((child) => {
            const matches = ev.child_path
              ? child.path === ev.child_path
              : child.filename === ev.child_filename

            if (!matches) {
              return child.status === DownloadStatusEnum.Downloading
                ? { ...child, speedBps: 0, etaSec: 0 }
                : child
            }

            const childTotal = ev.child_total ?? child.size ?? 0
            const childBytes = ev.child_bytes ?? child.bytesDownloaded ?? 0
            const childStatus =
              childTotal > 0 && childBytes >= childTotal
                ? DownloadStatusEnum.Complete
                : DownloadStatusEnum.Downloading

            return {
              ...child,
              bytesDownloaded: childBytes,
              speedBps: ev.child_speed ?? child.speedBps ?? 0,
              etaSec: ev.child_eta ?? child.etaSec ?? 0,
              status: childStatus
            }
          })
        }

        const isFolder = items.value[idx].isFolder && (nextChildren?.length ?? 0) > 0
        const aggregatedChildBytes = isFolder
          ? nextChildren!.reduce((sum, child) => sum + (child.bytesDownloaded ?? 0), 0)
          : bytes
        const aggregatedChildSpeed = isFolder
          ? nextChildren!.reduce((sum, child) => sum + (child.speedBps ?? 0), 0)
          : (ev.speed ?? 0)
        const aggregatedPercent = total > 0
          ? Math.min(100, Math.floor((aggregatedChildBytes / total) * 100))
          : items.value[idx].percent
        const aggregatedEta = aggregatedChildSpeed > 0 && total > aggregatedChildBytes
          ? Math.floor((total - aggregatedChildBytes) / aggregatedChildSpeed)
          : 0

        items.value[idx] = {
          ...items.value[idx],
          percent: aggregatedPercent,
          speedBps: aggregatedChildSpeed,
          etaSec: isFolder ? aggregatedEta : (ev.eta ?? 0),
          lastProgressAt: Date.now(),
          status: (ev.status as DownloadItem['status']) ?? items.value[idx].status,
          size: total > 0 ? total : items.value[idx].size,
          // Keep parent bytes implicit in percent/size, but base folder progress on the sum of children.
          children: nextChildren
        }
        // Somar speed de todos os itens ativos (throttled a 200ms para não sobrecarregar)
        const now = Date.now()
        if (now - lastSpeedEmit >= 120) {
          lastSpeedEmit = now
          const totalSpeed = items.value
            .filter((i) => i.status === DownloadStatusEnum.Downloading)
            .reduce((sum, i) => sum + effectiveSpeedValue(i), 0)
          emit('global-speed', totalSpeed)
        }
      } else {
        // Unknown item — refresh list
        void hydrate()
      }
    })
  )

  unsubs.push(
    window.api.downloads.on('download:verifying', (event: unknown) => {
      const ev = event as {
        id: string
        bytes_done?: number
        bytes_total?: number
      }
      if (!ev?.id) return
      const idx = itemIndexById.value[ev.id] ?? -1
      if (idx < 0) {
        void hydrate()
        return
      }

      const total = ev.bytes_total ?? items.value[idx].size
      const bytesDone = ev.bytes_done ?? 0
      const percent = total > 0
        ? Math.min(100, Math.floor((bytesDone / total) * 100))
        : items.value[idx].percent

      items.value[idx] = {
        ...items.value[idx],
        status: DownloadStatusEnum.Verifying,
        percent,
        size: total > 0 ? total : items.value[idx].size,
        speedBps: 0,
        etaSec: 0,
        lastProgressAt: Date.now(),
      }
    })
  )

  unsubs.push(
    window.api.downloads.on('download:complete', (event: unknown) => {
      const ev = event as { id: string; path?: string; outputPath?: string }
      if (!ev?.id) return
      const idx = itemIndexById.value[ev.id] ?? -1
      const outputPath = ev.path ?? ev.outputPath ?? ''
      if (idx >= 0) {
        items.value[idx] = {
          ...items.value[idx],
          status: DownloadStatusEnum.Complete,
          percent: 100,
          speedBps: 0,
          etaSec: 0,
          outputPath
        }
        emit('download-complete', {
          id: ev.id,
          outputPath,
          url: items.value[idx].url,
          title: items.value[idx].title,
          sha256Hash: items.value[idx].expectedHash?.algorithm === 'sha256'
            ? items.value[idx].expectedHash?.value
            : undefined,
        })

        // Auto-extract if enabled
        if (outputPath) {
          void window.api.settings.load().then((settings) => {
            if (!settings.autoExtract) return
            const passwords = (settings as any).passwordList ?? []
            return window.api.archive.autoExtract(outputPath, passwords).then((result) => {
              if (result.success && result.outputDir) {
                void window.api.system.notify('Extração concluída', result.outputDir).catch(() => null)
              } else if (result.error === 'WRONG_PASSWORD') {
                void window.api.system.notify('Extração falhou', 'Nenhuma senha funcionou').catch(() => null)
              }
            })
          }).catch(() => null)
        }
      }
      void hydrate()
    })
  )

  unsubs.push(
    window.api.downloads.on('download:error', (event: unknown) => {
      const ev = event as { id: string; message?: string; error?: string }
      if (!ev?.id) return
      const idx = itemIndexById.value[ev.id] ?? -1
      if (idx >= 0) {
        items.value[idx] = {
          ...items.value[idx],
          status: DownloadStatusEnum.Error,
          speedBps: 0,
          etaSec: 0,
          error: ev.message ?? ev.error ?? 'Erro desconhecido'
        }
      }
      void hydrate()
    })
  )

  // Status change events (includes rate_limited + waiting_captcha metadata)
  unsubs.push(
    window.api.downloads.on('download:status', (event: unknown) => {
      const ev = event as {
        type?: string
        id: string
        status?: DownloadItem['status']
        retry_at?: number
        captcha_type?: string
        captcha_sitekey?: string
        captcha_page_url?: string
        error?: string
      }
      if (!ev?.id) return
      if (ev.type === 'status_changed') {
        upsertById(ev.id, {
          status: ev.status,
          retryAt: ev.retry_at ? ev.retry_at * 1000 : undefined,
          captchaType: ev.captcha_type ?? undefined,
          captchaSitekey: ev.captcha_sitekey ?? undefined,
          captchaPageUrl: ev.captcha_page_url ?? undefined,
          error: ev.error ?? '',
          speedBps: 0,
          etaSec: 0,
        })
        // Flash animation for status change
        flashingIds.value = new Set([...flashingIds.value, ev.id])
        setTimeout(() => {
          flashingIds.value = new Set([...flashingIds.value].filter((id) => id !== ev.id))
        }, 400)
        void maybeResolveCaptchaById(ev.id)
      } else {
        void hydrate()
      }
    })
  )

  unsubs.push(
    window.api.downloads.on('download:cancelled', (event: unknown) => {
      const ev = event as { id: string }
      if (!ev?.id) return
      upsertById(ev.id, { status: DownloadStatusEnum.Cancelled })
      void hydrate()
    })
  )
})

onUnmounted(() => {
  isMounted = false
  if (retryTimer !== null) {
    window.clearInterval(retryTimer)
    retryTimer = null
  }
  if (hydrateTimer !== null) {
    window.clearInterval(hydrateTimer)
    hydrateTimer = null
  }
  for (const unsub of unsubs) unsub()
})

watch(activeCaptchaItem, (item) => {
  if (!item) {
    return
  }
  nextTick(() => {
    captchaPrimaryButtonRef.value?.focus()
    focusFirstDialogElement(captchaModalRef.value)
  })
})

// ── Data methods ───────────────────────────────────────────
async function hydrate(): Promise<void> {
  if (hydrateInFlight) {
    hydrateQueued = true
    return
  }
  hydrateInFlight = true
  try {
    if (!isMounted) return
    const fresh: DownloadItem[] = await window.api.downloads.list().catch(() => [])
    const freshById = new Map<string, DownloadItem>(fresh.map((item) => [item.id, item]))

    for (let i = items.value.length - 1; i >= 0; i--) {
      if (!freshById.has(items.value[i].id)) {
        items.value.splice(i, 1)
      }
    }

    for (const freshItem of fresh) {
      const idx = items.value.findIndex((i) => i.id === freshItem.id)
      if (idx >= 0) {
        Object.assign(items.value[idx], freshItem)
      } else {
        items.value.push(freshItem)
      }
    }

    rebuildItemIndex()

    for (const item of items.value) {
      if (item.status !== DownloadStatusEnum.WaitingCaptcha) {
        captchaAttemptedIds.delete(item.id)
        if (activeCaptchaId.value === item.id) {
          activeCaptchaId.value = null
        }
      }
    }

    for (const item of items.value) {
      if (item.status === DownloadStatusEnum.WaitingCaptcha) {
        void maybeResolveCaptcha(item)
      }
    }

    emit('count-change', items.value.length)
    emit('skeleton-done')
  } finally {
    hydrateInFlight = false
    if (hydrateQueued) {
      hydrateQueued = false
      if (isMounted) void hydrate()
    }
  }
}

async function createPackage(): Promise<void> {
  const name = window.prompt('Nome do pacote')
  if (!name?.trim()) return
  const palette = ['#2563eb', '#16a34a', '#f59e0b', '#dc2626', '#7c3aed', '#0891b2']
  const color = palette[packages.value.length % palette.length]
  const created = await window.api.packages.create({ name: name.trim(), color }).catch(() => null)
  if (created) packages.value = [created, ...packages.value]
}

async function assignPackage(item: DownloadItem, packageId: string): Promise<void> {
  if (packageId) {
    await window.api.packages.assign(packageId, item.id).catch(() => null)
  } else {
    await window.api.packages.unassign(item.id).catch(() => null)
  }
  const idx = itemIndexById.value[item.id] ?? -1
  if (idx >= 0) {
    items.value[idx] = { ...items.value[idx], packageId: packageId || undefined }
  }
}

function packageColor(packageId: string | undefined): string {
  if (!packageId) return '#9ca3af'
  return packages.value.find((pkg) => pkg.id === packageId)?.color ?? '#9ca3af'
}

function upsertById(id: string, patch: Partial<DownloadItem>): void {
  const idx = itemIndexById.value[id] ?? -1
  if (idx === -1) {
    void hydrate()
    return
  }
  items.value[idx] = { ...items.value[idx], ...patch }
}

function rebuildItemIndex(): void {
  itemIndexById.value = Object.fromEntries(items.value.map((item, index) => [item.id, index]))
}

function onListScroll(): void {
  listScrollTop.value = itemsContainerRef.value?.scrollTop ?? 0
}

// ── Actions ────────────────────────────────────────────────
async function cancel(id: string): Promise<void> {
  await window.api.downloads.cancel(id).catch(() => null)
  await hydrate()
}

async function pause(id: string): Promise<void> {
  await window.api.downloads.pause(id).catch(() => null)
  await hydrate()
}

async function resume(id: string): Promise<void> {
  await window.api.downloads.resume(id).catch(() => null)
  await hydrate()
}

async function retry(id: string): Promise<void> {
  await window.api.downloads.retry(id).catch(() => null)
  await hydrate()
}

async function force(id: string): Promise<void> {
  await window.api.downloads.force(id).catch(() => null)
  await hydrate()
}

async function restart(id: string): Promise<void> {
  await window.api.downloads.restart(id).catch(() => null)
  await hydrate()
}

async function remove(id: string): Promise<void> {
  await window.api.downloads.remove(id).catch(() => null)
  await hydrate()
}

async function removeWithFiles(id: string): Promise<void> {
  if (!window.confirm('Remover da lista e apagar os arquivos físicos deste download?')) {
    return
  }
  await window.api.downloads.removeWithFiles(id).catch(() => null)
  await hydrate()
}

async function clearFinished(): Promise<void> {
  await window.api.downloads.clearFinished().catch(() => null)
  await hydrate()
}

async function togglePin(id: string): Promise<void> {
  const idx = itemIndexById.value[id] ?? -1
  if (idx >= 0) {
    // Optimistic update
    items.value[idx] = { ...items.value[idx], pinned: !items.value[idx].pinned }
  }
  await window.api.downloads.togglePin(id).catch(() => null)
}

function chooseOutputDir(): void {
  window.api.settings.chooseDirectory().then((dir) => {
    if (dir) {
      window.api.settings.load().then((settings) => {
        window.api.settings.save({ ...settings, outputDir: dir }).catch(() => null)
      }).catch(() => null)
    }
  }).catch(() => null)
}

async function copyUrl(item: DownloadItem): Promise<void> {
  const folderUrls = (item.children ?? [])
    .map((child: DownloadChild) => child.sourceUrl?.trim() ?? '')
    .filter((url) => url.length > 0)

  const payload = item.isFolder && folderUrls.length > 0
    ? folderUrls.join('\n')
    : item.url

  await window.api.clipboard.writeText(payload).catch(() => null)
}

async function extract(filePath: string): Promise<void> {
  try {
    const extractedPath = await window.api.archive.extract(filePath)
    await window.api.system.notify('Extração concluída', extractedPath).catch(() => null)
    await window.api.openPath(extractedPath).catch(() => null)
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error)
    await window.api.system.notify('Falha na extração', message).catch(() => null)
  }
}

function toggleFolder(id: string): void {
  expandedFolders.value = {
    ...expandedFolders.value,
    [id]: !expandedFolders.value[id]
  }
}

function effectiveSpeedValue(item: DownloadItem): number {
  return effectiveSpeed(item, nowTick.value)
}

function effectiveEtaValue(item: DownloadItem): number {
  return effectiveEta(item, nowTick.value)
}

function isWaitingRetryNow(item: DownloadItem): boolean {
  return isWaitingRetry(item, nowTick.value)
}

function retryCountdownNow(item: DownloadItem): number {
  return retryCountdown(item, nowTick.value)
}

function statusTextValue(item: DownloadItem): string {
  return statusText(item, nowTick.value)
}

function actionsFor(item: DownloadItem): Record<string, boolean> {
  return getDownloadActions(item)
}

function openCaptcha(id: string): void {
  activeCaptchaId.value = id
  const item = items.value.find((entry) => entry.id === id)
  if (item) {
    void openCaptchaWindow(item)
  }
}

function closeCaptchaModal(): void {
  activeCaptchaId.value = null
}

function onCaptchaDialogKeydown(event: KeyboardEvent): void {
  if (event.key === 'Escape') {
    event.preventDefault()
    event.stopPropagation()
    closeCaptchaModal()
    return
  }

  trapDialogTab(event, captchaModalRef.value)
}

function reopenCaptchaWindow(): void {
  if (!activeCaptchaItem.value) {
    return
  }
  void openCaptchaWindow(activeCaptchaItem.value)
}

function isExpanded(id: string): boolean {
  return !!expandedFolders.value[id]
}

function openFolder(filePath: string): void {
  window.api.showInFolder(filePath)
}

function childNodes(children?: DownloadChild[]): DerivedChildNode[] {
  if (!children?.length) {
    return []
  }
  return flattenChildTree(buildChildTree(children))
}

function isExtractableArchive(filePath: string): boolean {
  const lower = filePath.toLowerCase()
  return (
    lower.endsWith('.zip') ||
    lower.endsWith('.rar') ||
    lower.endsWith('.7z') ||
    lower.endsWith('.tar') ||
    lower.endsWith('.tar.gz') ||
    lower.endsWith('.tgz') ||
    lower.endsWith('.tar.bz2') ||
    lower.endsWith('.tbz2') ||
    lower.endsWith('.tar.xz') ||
    lower.endsWith('.txz') ||
    lower.endsWith('.tar.zst')
  )
}

// ── Display helpers ────────────────────────────────────────
function moduleLabel(moduleId: string): string {
  return modulesById.value[moduleId]?.name ?? moduleId
}

function getIcon(moduleId: string) {
  return getProviderIcon(moduleId)
}

function getProgressColor(item: DownloadItem): string {
  if (item.status === 'error' || item.status === 'corrupted') return '#ef4444'
  if (item.status === 'disk_full') return '#ef4444'
  if (item.status === 'verifying') return 'linear-gradient(90deg, #38bdf8, #60a5fa)'
  if (item.status === 'complete') return 'linear-gradient(90deg, #22c55e, #4ade80)'
  if (item.status === 'cancelled') return '#666'
  if (isWaitingRetryNow(item)) return 'linear-gradient(90deg, #f59e0b, #fbbf24)'
  // Use provider color for active downloads
  const color = modulesById.value[item.moduleId]?.color ?? getProviderColor(item.moduleId)
  return `linear-gradient(90deg, ${color}, ${color}cc)`
}

function childPercent(child: Pick<DownloadChild, 'size' | 'bytesDownloaded'>): number {
  if (!child.size || child.size <= 0) return 0
  const bytes = child.bytesDownloaded ?? 0
  return Math.max(0, Math.min(100, Math.floor((bytes / child.size) * 100)))
}

async function openCaptchaWindow(item: DownloadItem): Promise<void> {
  if (captchaWindowBusy.value) {
    return
  }

  captchaWindowBusy.value = true
  try {
    const token = await window.api.captcha.openWindow({
      provider: item.moduleId,
      pageUrl: item.captchaPageUrl ?? item.url,
      sourceUrl: item.url,
    }).catch(() => null)

    if (!token) {
      return
    }

    await window.api.captcha.submit(item.id, token).catch(() => null)
    if (activeCaptchaId.value === item.id) {
      activeCaptchaId.value = null
    }
  } finally {
    captchaWindowBusy.value = false
  }
}

async function maybeResolveCaptcha(item: DownloadItem): Promise<void> {
  if (item.status !== DownloadStatusEnum.WaitingCaptcha || !item.captchaSitekey || captchaAttemptedIds.has(item.id)) {
    return
  }

  captchaAttemptedIds.add(item.id)
  const token = await window.api.captcha.nopechaSolve({
    type: item.captchaType ?? 'recaptcha2',
    sitekey: item.captchaSitekey,
    pageurl: item.captchaPageUrl ?? item.url,
  }).catch(() => null)

  if (token) {
    await window.api.captcha.submit(item.id, token).catch(() => null)
    return
  }

  if (!activeCaptchaId.value) {
    activeCaptchaId.value = item.id
    void openCaptchaWindow(item)
  }
}

async function maybeResolveCaptchaById(id: string): Promise<void> {
  const item = items.value.find((entry) => entry.id === id)
  if (!item) {
    return
  }
  await maybeResolveCaptcha(item)
}
</script>

<style scoped>
/* ── Container ──────────────────────────────────────────────── */
.download-list {
  display: flex;
  flex-direction: row;
  flex: 1;
  width: 100%;
  min-width: 0;
  min-height: 0;
  gap: 0;
  align-self: stretch;
  overflow: hidden;
}

.package-sidebar {
  width: 220px;
  min-width: 190px;
  max-width: 260px;
  flex: 0 0 220px;
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 2px 12px 2px 0;
  border-right: 1px solid var(--border-color);
  margin-right: 12px;
  overflow-y: auto;
}

.package-tree {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.package-node {
  display: grid;
  grid-template-columns: 10px minmax(0, 1fr) auto auto;
  align-items: center;
  gap: 8px;
  width: 100%;
  min-height: 34px;
  padding: 0 8px;
  border: 1px solid transparent;
  border-radius: 7px;
  background: transparent;
  color: var(--text-muted);
  cursor: pointer;
  text-align: left;
}

.package-node:hover,
.package-node.active {
  border-color: color-mix(in srgb, var(--accent-color) 22%, transparent);
  background: color-mix(in srgb, var(--accent-color) 8%, transparent);
  color: var(--text-primary);
}

.package-node-label {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 12px;
}

.package-node-count {
  min-width: 22px;
  padding: 2px 6px;
  border-radius: 999px;
  background: var(--bg-card);
  color: var(--text-muted);
  font-size: 11px;
  text-align: center;
}

.package-color-dot,
.package-status-dot {
  width: 8px;
  height: 8px;
  border-radius: 999px;
}

.package-status-dot.ok { background: #22c55e; }
.package-status-dot.active { background: #3b82f6; }
.package-status-dot.failed { background: #ef4444; }
.package-status-dot.idle { background: #9ca3af; }

.columns-menu-wrap {
  position: relative;
}

.columns-menu {
  position: absolute;
  right: 0;
  top: calc(100% + 6px);
  z-index: 20;
  width: 190px;
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 8px;
  border: 1px solid var(--border-color);
  border-radius: 8px;
  background: var(--bg-card);
  box-shadow: 0 14px 28px rgba(0, 0, 0, 0.18);
}

.column-option {
  display: flex;
  align-items: center;
  gap: 8px;
  height: 30px;
  padding: 0 7px;
  border-radius: 6px;
  color: var(--text-primary);
  font-size: 12px;
  cursor: grab;
}

.column-option:hover {
  background: color-mix(in srgb, var(--accent-color) 9%, transparent);
}

@media (max-width: 760px) {
  .download-list {
    flex-direction: column;
  }

  .package-sidebar {
    width: 100%;
    max-width: none;
    flex: 0 0 auto;
    flex-direction: row;
    overflow-x: auto;
    overflow-y: hidden;
    border-right: none;
    border-bottom: 1px solid var(--border-color);
    margin-right: 0;
    margin-bottom: 10px;
    padding: 0 0 8px;
  }

  .package-tree {
    flex-direction: row;
  }

  .package-node {
    min-width: 150px;
  }
}

/* ── Empty state ────────────────────────────────────────────── */
.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  flex: 1;
  min-height: 0;
  gap: 10px;
  color: var(--text-muted);
  text-align: center;
}

.empty-icon {
  opacity: 0.4;
  margin-bottom: 8px;
}

.empty-title {
  font-size: 15px;
  font-weight: 600;
  margin: 0;
  color: var(--text-muted);
}

.empty-sub {
  font-size: 13px;
  margin: 0;
  color: var(--text-muted);
  opacity: 0.7;
}

.empty-actions {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  flex-wrap: wrap;
  margin-top: 8px;
}

.empty-primary,
.empty-secondary {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  height: 34px;
  padding: 0 12px;
  border-radius: 7px;
  font-size: 12px;
  cursor: pointer;
}

.empty-primary {
  border: 1px solid var(--accent-color);
  background: var(--accent-color);
  color: white;
}

.empty-secondary {
  border: 1px solid var(--border-color);
  background: var(--bg-card);
  color: var(--text-primary);
}

/* ── Items list ─────────────────────────────────────────────── */
.items-container {
  display: flex;
  flex-direction: column;
  width: 100%;
  min-width: 0;
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  gap: 10px;
  padding-right: 2px;
}

.list-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  width: 100%;
  min-width: 0;
}

.list-count {
  font-size: 12px;
  color: var(--text-muted);
}

.toolbar-actions {
  display: inline-flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
}

.toolbar-sort {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
  color: var(--text-muted);
}

.toolbar-select {
  border: 1px solid var(--border-color);
  background: var(--bg-card);
  color: var(--text-primary);
  border-radius: 10px;
  padding: 8px 10px;
  font-size: 12px;
  font-weight: 600;
  outline: none;
}

.toolbar-btn {
  border: 1px solid var(--border-color);
  background: var(--bg-card);
  color: var(--text-primary);
  border-radius: 10px;
  padding: 8px 12px;
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
}

.toolbar-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.items-stack {
  display: flex;
  flex-direction: column;
  gap: 10px;
  width: 100%;
  min-width: 0;
  align-self: stretch;
}

/* ── Download card ──────────────────────────────────────────── */
.download-card {
  display: flex;
  align-items: flex-start;
  gap: 12px;
  padding: 14px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: 12px;
  transition: border-color 0.2s ease, box-shadow 0.2s ease;
  position: relative;
  overflow: hidden;
  width: 100%;
  box-sizing: border-box;
  align-self: stretch;
  content-visibility: auto;
  contain: layout paint style;
}

.download-card::before {
  content: '';
  position: absolute;
  left: 0;
  top: 0;
  bottom: 0;
  width: 3px;
  border-radius: 12px 0 0 12px;
  background: var(--status-indicator, var(--border-color));
}

.download-card.status-bg-downloading::before {
  background: var(--accent-gradient);
  animation: pulse-glow 2s ease-in-out infinite;
}

.download-card.status-bg-verifying::before {
  background: linear-gradient(180deg, #38bdf8, #60a5fa);
  animation: pulse-glow 2s ease-in-out infinite;
}

.download-card.status-bg-complete::before {
  background: linear-gradient(180deg, #22c55e, #4ade80);
}

.download-card.status-bg-error::before,
.download-card.status-bg-corrupted::before {
  background: #ef4444;
}

.download-card.status-bg-cancelled::before {
  background: #555;
}

.download-card:hover {
  border-color: color-mix(in srgb, var(--accent-color) 40%, var(--border-color));
  box-shadow: var(--shadow-card);
}

/* ── Provider icon ──────────────────────────────────────────── */
.provider-icon {
  width: 36px;
  height: 36px;
  flex-shrink: 0;
  border-radius: 8px;
  overflow: hidden;
  display: flex;
  align-items: center;
  justify-content: center;
  margin-top: 2px;
}

.provider-icon :deep(svg) {
  width: 36px;
  height: 36px;
}

/* ── Item body ──────────────────────────────────────────────── */
.item-body {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 7px;
}

/* ── Header row ─────────────────────────────────────────────── */
.item-header {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.item-title-wrap {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 8px;
}

.item-title {
  font-size: 13px;
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--text-primary);
}

.type-icon {
  width: 18px;
  height: 18px;
  flex-shrink: 0;
  display: inline-block;
  background-size: contain;
  background-position: center;
  background-repeat: no-repeat;
}

.item-actions {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
}

/* ── Status badge ───────────────────────────────────────────── */
.status-badge {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 3px 8px;
  border-radius: 999px;
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.3px;
  text-transform: uppercase;
}

.badge-pending {
  background: rgba(136, 136, 170, 0.15);
  color: var(--status-pending);
}

.badge-downloading {
  background: rgba(124, 111, 255, 0.15);
  color: var(--status-downloading);
}

.badge-verifying {
  background: rgba(56, 189, 248, 0.15);
  color: #38bdf8;
}

.badge-complete {
  background: rgba(34, 197, 94, 0.15);
  color: var(--status-complete);
}

.badge-error,
.badge-corrupted {
  background: rgba(239, 68, 68, 0.15);
  color: var(--status-error);
}

.badge-cancelled {
  background: rgba(100, 100, 120, 0.15);
  color: var(--status-cancelled);
}

.badge-paused {
  background: rgba(250, 204, 21, 0.15);
  color: var(--status-paused);
}

.badge-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  flex-shrink: 0;
}

.dot-pending { background: var(--status-pending); }
.dot-downloading {
  background: var(--status-downloading);
  animation: pulse-glow 1.2s ease-in-out infinite;
}
.dot-verifying {
  background: #38bdf8;
  animation: pulse-glow 1.2s ease-in-out infinite;
}
.dot-complete { background: var(--status-complete); }
.dot-error,
.dot-corrupted { background: var(--status-error); }
.dot-cancelled { background: var(--status-cancelled); }
.dot-paused { background: var(--status-paused); }
.dot-rate_limited { background: #f59e0b; animation: pulse-glow 1.5s ease-in-out infinite; }
.dot-waiting_captcha { background: #8b5cf6; animation: pulse-glow 1.5s ease-in-out infinite; }

.badge-rate_limited {
  background: rgba(245, 158, 11, 0.15);
  color: #f59e0b;
}

.badge-waiting_captcha {
  background: rgba(139, 92, 246, 0.15);
  color: #8b5cf6;
}

.download-card.status-bg-rate_limited::before {
  background: linear-gradient(180deg, #f59e0b, #fbbf24);
  animation: pulse-glow 2s ease-in-out infinite;
}

.download-card.status-bg-waiting_captcha::before {
  background: linear-gradient(180deg, #8b5cf6, #a78bfa);
  animation: pulse-glow 2s ease-in-out infinite;
}

/* ── Action buttons ─────────────────────────────────────────── */
.cancel-btn,
.open-btn,
.action-btn {
  width: 26px;
  height: 26px;
  border-radius: 6px;
  border: 1px solid var(--border-color);
  background: transparent;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  font-size: 11px;
  transition: all 0.15s ease;
}

.action-btn {
  color: var(--text-muted);
}

.action-btn:hover {
  background: rgba(99, 102, 241, 0.15);
  border-color: #6366f1;
  color: #6366f1;
}

.cancel-btn {
  color: var(--text-muted);
}

.cancel-btn:hover {
  background: rgba(239, 68, 68, 0.15);
  border-color: #ef4444;
  color: #ef4444;
}

.open-btn {
  color: var(--text-muted);
}

.open-btn:hover {
  background: rgba(34, 197, 94, 0.15);
  border-color: #22c55e;
  color: #22c55e;
}

/* ── Progress track ─────────────────────────────────────────── */
.progress-track {
  height: 6px;
  background: var(--surface-section);
  border-radius: 999px;
  overflow: hidden;
}

.progress-fill {
  height: 100%;
  border-radius: 999px;
  transition: width 0.3s ease;
  min-width: 2px;
}

.progress-shimmer {
  background-size: 200% 100% !important;
  animation: shimmer 1.8s ease-in-out infinite;
}

@keyframes shimmer {
  0% { background-position: 200% 0; }
  100% { background-position: -200% 0; }
}

/* ── Meta info ──────────────────────────────────────────────── */
.item-meta {
  display: flex;
  align-items: center;
  gap: 4px;
  flex-wrap: wrap;
  font-size: 11px;
  color: var(--text-muted);
}

.meta-percent {
  font-weight: 700;
  color: var(--text-primary);
  min-width: 32px;
}

.meta-sep {
  opacity: 0.4;
  padding: 0 2px;
}

.meta-speed {
  color: var(--accent-color);
  font-weight: 600;
}

.meta-eta {
  color: var(--text-muted);
}

.meta-wait,
.meta-wait-reason {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  color: #f59e0b;
}

.meta-wait {
  font-weight: 600;
}

.meta-verifying {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  color: #38bdf8;
  font-weight: 600;
}

.meta-size {
  color: var(--text-muted);
  font-family: 'Courier New', monospace;
  font-size: 10.5px;
}

.meta-error {
  color: #fca5a5;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 200px;
}

.meta-retries {
  color: var(--text-muted);
  font-size: 10.5px;
}

/* ── Output path ────────────────────────────────────────────── */
.item-path {
  display: flex;
  align-items: center;
  gap: 5px;
  font-size: 10.5px;
  color: var(--text-muted);
  font-family: 'Courier New', monospace;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  cursor: pointer;
  transition: color 0.15s;
  padding: 2px 0;
}

.item-path:hover {
  color: var(--accent-color);
}

.folder-children {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-top: 4px;
  padding: 8px 10px;
  border: 1px solid color-mix(in srgb, var(--border-color) 70%, transparent);
  border-radius: 10px;
  background: color-mix(in srgb, var(--bg-card) 70%, transparent);
  overflow: hidden;
  transition: opacity 0.2s ease;
}

.folder-tree-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding-bottom: 8px;
  border-bottom: 1px solid color-mix(in srgb, var(--border-color) 82%, transparent);
  font-size: 11px;
  font-weight: 700;
  color: var(--text-secondary);
}

.child-row {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
  font-size: 12px;
  color: var(--text-muted);
}

.child-main {
  min-width: 0;
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.child-name {
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}

.child-name-label {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
}

.child-meta {
  display: flex;
  align-items: center;
  gap: 4px;
  flex-wrap: wrap;
  font-size: 10.5px;
  color: var(--text-muted);
}

.child-status {
  font-weight: 600;
  color: var(--text-primary);
}

.child-track {
  height: 4px;
  background: var(--surface-section);
  border-radius: 999px;
  overflow: hidden;
}

.child-fill {
  height: 100%;
  min-width: 2px;
  border-radius: 999px;
  background: linear-gradient(90deg, var(--accent-color), color-mix(in srgb, var(--accent-color) 60%, #ffffff));
}

.child-icon {
  width: 16px;
  height: 16px;
  flex-shrink: 0;
  display: inline-block;
  background-size: contain;
  background-position: center;
  background-repeat: no-repeat;
}

.child-size {
  flex-shrink: 0;
  color: var(--text-muted);
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

.child-row.is-folder-row {
  background: color-mix(in srgb, var(--bg-card) 82%, rgba(255, 193, 7, 0.07));
  border-radius: 8px;
  padding: 4px 6px;
}

/* ── Captcha modal ──────────────────────────────────────────── */
.captcha-modal-backdrop {
  position: fixed;
  inset: 0;
  z-index: 40;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 24px;
  background: rgba(15, 23, 42, 0.42);
  backdrop-filter: blur(6px);
}

.captcha-modal {
  width: min(560px, 100%);
  display: flex;
  flex-direction: column;
  gap: 14px;
  padding: 18px;
  border-radius: 18px;
  border: 1px solid color-mix(in srgb, #8b5cf6 24%, var(--border-color));
  background: var(--bg-card);
  box-shadow: 0 22px 50px rgba(15, 23, 42, 0.24);
}

.captcha-modal-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
}

.captcha-modal-header strong {
  display: block;
  margin-bottom: 4px;
  font-size: 14px;
  color: var(--text-primary);
}

.captcha-modal-header p {
  margin: 0;
  font-size: 12px;
  color: var(--text-muted);
}

.captcha-modal-file {
  padding: 10px 12px;
  border-radius: 12px;
  background: color-mix(in srgb, var(--bg-primary) 86%, var(--bg-secondary));
  color: var(--text-secondary);
  font-size: 12px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.captcha-modal-body {
  display: flex;
  flex-direction: column;
  gap: 14px;
  padding: 14px;
  border-radius: 14px;
  background: color-mix(in srgb, var(--bg-primary) 88%, var(--bg-card));
  border: 1px solid color-mix(in srgb, var(--accent-primary, #5b6cff) 20%, var(--border-color));
}

.captcha-modal-body p {
  margin: 0;
  font-size: 13px;
  line-height: 1.55;
  color: var(--text-secondary);
}

.captcha-modal-open-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  min-height: 42px;
  padding: 0 14px;
  border: none;
  border-radius: 12px;
  background: linear-gradient(135deg, var(--accent-primary, #5b6cff), color-mix(in srgb, var(--accent-primary, #5b6cff) 72%, #ffffff));
  color: white;
  font-size: 13px;
  font-weight: 700;
  cursor: pointer;
}

.captcha-modal-open-btn:disabled {
  opacity: 0.7;
  cursor: wait;
}

.captcha-row {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 10px;
  border: 1px solid color-mix(in srgb, #8b5cf6 30%, var(--border-color));
  border-radius: 10px;
  background: color-mix(in srgb, #8b5cf6 5%, var(--bg-card));
}

.captcha-header {
  display: flex;
  align-items: center;
  gap: 7px;
  font-size: 12px;
  font-weight: 600;
  color: #8b5cf6;
}

.captcha-frame {
  width: 100%;
  height: 82px;
  border: none;
  border-radius: 6px;
  background: transparent;
}

.meta-captcha-wait {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  color: #8b5cf6;
  font-weight: 600;
}

.package-picker {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}

.package-picker select {
  max-width: 150px;
  height: 24px;
  border: 1px solid var(--border-color);
  border-radius: 6px;
  background: var(--bg-card);
  color: var(--text-primary);
  font-size: 12px;
}

.package-dot {
  width: 8px;
  height: 8px;
  flex: 0 0 auto;
  border-radius: 999px;
}

@keyframes pulse-glow {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.5; }
}

/* ── Skeleton cards ─────────────────────────────────────────── */
@keyframes shimmer-skeleton {
  0%   { background-position: -200% 0; }
  100% { background-position:  200% 0; }
}

.skeleton-card {
  pointer-events: none;
  flex-direction: column;
  gap: 8px;
}

.skeleton-line {
  border-radius: 6px;
  background: linear-gradient(
    90deg,
    var(--bg-card) 25%,
    color-mix(in srgb, var(--bg-card) 70%, var(--text-muted)) 50%,
    var(--bg-card) 75%
  );
  background-size: 200% 100%;
  animation: shimmer-skeleton 1.4s infinite;
}

.skeleton-title    { height: 14px; width: 55%; }
.skeleton-progress { height: 8px;  width: 100%; }
.skeleton-meta     { height: 10px; width: 35%; }

/* ── disk_full meta ─────────────────────────────────────────── */
.meta-disk-full {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  color: #ef4444;
  font-weight: 600;
}

/* ── Status flash animation ─────────────────────────────────── */
@keyframes statusFlash {
  0% { background-color: color-mix(in srgb, var(--accent-color) 15%, transparent); }
  100% { background-color: transparent; }
}

.status-flash {
  animation: statusFlash 400ms ease-out;
}

/* ── Progress bar smooth transition ────────────────────────── */
.progress-fill {
  transition: width 0.4s ease-out;
}

/* ── Pin button ─────────────────────────────────────────────── */
.pin-btn {
  color: var(--text-muted);
}

.pin-btn-active {
  color: #fbbf24 !important;
  border-color: rgba(251, 191, 36, 0.4) !important;
  background: rgba(251, 191, 36, 0.1) !important;
}

.pin-btn:hover {
  color: #fbbf24 !important;
  border-color: rgba(251, 191, 36, 0.4) !important;
  background: rgba(251, 191, 36, 0.1) !important;
}

/* ── Pinned card indicator ──────────────────────────────────── */
.card-pinned {
  border-color: color-mix(in srgb, #fbbf24 30%, var(--border-color)) !important;
}

.card-pinned::after {
  content: '';
  position: absolute;
  top: 6px;
  right: 6px;
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: #fbbf24;
  opacity: 0.7;
}

/* ── disk_full card indicator ──────────────────────────────── */
.download-card.status-bg-disk_full::before {
  background: #ef4444;
}

.badge-disk_full {
  background: rgba(239, 68, 68, 0.15);
  color: #ef4444;
}

.dot-disk_full {
  background: #ef4444;
}
</style>
