<template>
  <div class="mirrors-modal-backdrop" @click.self="$emit('close')">
    <div
      ref="modalRef"
      class="mirrors-modal"
      role="dialog"
      aria-modal="true"
      :aria-labelledby="titleId"
      tabindex="-1"
        @keydown="onDialogKeydown"
    >
      <div class="mirrors-modal-header">
        <div class="mirrors-modal-copy">
          <div class="mirrors-title-row">
            <i class="pi pi-sitemap"></i>
            <span :id="titleId" class="mirrors-title">{{ t('mirrorsTitle') }}</span>
          </div>
          <p class="mirrors-modal-subtitle">
            {{ t('mirrorsSubtitle') }}
          </p>
        </div>

        <div class="mirrors-modal-actions">
          <button
            ref="searchButtonRef"
            class="btn-find-mirrors"
            :class="{ loading: searching }"
            :disabled="searching"
            @click="$emit('search')"
          >
            <i :class="searching ? 'pi pi-spin pi-spinner' : 'pi pi-search'"></i>
            {{ searching ? t('mirrorsSearching') : t('mirrorsSearchNow') }}
          </button>
          <button class="row-action-btn" type="button" :title="t('close')" @click="$emit('close')">
            <i class="pi pi-times"></i>
          </button>
        </div>
      </div>

      <div class="mirrors-file-card" :title="filename">
        {{ filename }}
      </div>

      <div class="mirrors-stats">
        <div class="mirrors-stat-card">
          <span class="mirrors-stat-label">{{ t('mirrorsCurrentSearcher') }}</span>
          <strong>{{ currentSearcher }}</strong>
          <small>{{ phaseLabel }}</small>
        </div>
        <div class="mirrors-stat-card">
          <span class="mirrors-stat-label">{{ t('progress') }}</span>
          <strong>{{ progressText }}</strong>
          <small>{{ progressPercent }}%</small>
        </div>
        <div class="mirrors-stat-card">
          <span class="mirrors-stat-label">{{ t('mirrorsResults') }}</span>
          <strong>{{ results.length }}</strong>
          <small>{{ hosterText }}</small>
        </div>
        <div class="mirrors-stat-card">
          <span class="mirrors-stat-label">{{ t('mirrorsTime') }}</span>
          <strong>{{ elapsedLabel }}</strong>
          <small>{{ timingLabel }}</small>
        </div>
      </div>

      <div class="mirrors-body">
        <div class="mirrors-progress-wrap">
          <div class="mirrors-progress-copy">
            <span>{{ progressHeadline }}</span>
            <span>{{ progressText }}</span>
          </div>
          <div class="mirrors-progress-track">
            <div class="mirrors-progress-fill" :style="{ width: `${progressPercent}%` }"></div>
          </div>
        </div>

        <div class="mirrors-grid">
          <div class="mirrors-results-wrap">
            <div class="mirrors-results-head">
              <div class="mirrors-results-label">
                <i class="pi pi-check-circle"></i>
                <span>{{ results.length }} {{ t('mirrorsRealtimeLinks') }}</span>
              </div>
              <button
                v-if="results.length > 0"
                class="mirrors-copy-btn"
                type="button"
                @click="$emit('copy-all')"
              >
                {{ t('mirrorsCopyAll') }}
              </button>
            </div>

            <div v-if="results.length > 0" class="mirrors-results-list">
              <button
                v-for="result in results"
                :key="result.url"
                type="button"
                class="mirror-result-card"
                :class="{ 'mirror-result-best': isBestMirror(result.url) }"
                @click="$emit('copy-one', result.url)"
              >
                <div class="mirror-result-top">
                  <span class="mirror-result-source">{{ result.source }}</span>
                  <div class="mirror-result-badges">
                    <span v-if="isBestMirror(result.url)" class="mirror-badge-best">
                      <i class="pi pi-star-fill"></i>
                      Recomendado
                    </span>
                    <span v-else-if="estimatingUrls.has(result.url)" class="mirror-badge-estimating">
                      <i class="pi pi-spin pi-spinner"></i>
                    </span>
                    <span class="mirror-result-score">score {{ result.score }}</span>
                  </div>
                </div>
                <div class="mirror-result-url">{{ result.url }}</div>
                <div class="mirror-result-meta">
                  <span>{{ result.hoster ?? t('mirrorsDirectLink') }}</span>
                  <span>{{ t('mirrorsClickToCopy') }}</span>
                </div>
              </button>
            </div>

            <div v-else class="mirrors-empty">
              {{ searching ? t('mirrorsWaitingFirstLinks') : t('mirrorsEmptyState') }}
            </div>
          </div>

          <div class="mirrors-log-wrap">
            <div class="mirrors-log-head">
              <span>{{ t('mirrorsLog') }}</span>
              <small>{{ log.length }} {{ t('mirrorsEvents') }}</small>
            </div>
            <pre ref="logRef" class="mirrors-log">{{ log.join('\n') }}</pre>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { nextTick, onMounted, ref, watch } from 'vue'

import { useI18n } from '../i18n'
import { focusFirstDialogElement, trapDialogTab } from '../utils/dialog-focus'

interface MirrorViewResult {
  url: string
  source: string
  hoster?: string | null
  score: number
}

const props = defineProps<{
  filename: string
  searching: boolean
  currentSearcher: string
  phaseLabel: string
  progressText: string
  progressPercent: number
  hosterText: string
  elapsedLabel: string
  timingLabel: string
  progressHeadline: string
  results: MirrorViewResult[]
  log: string[]
}>()

// ── Mirror speed estimation ───────────────────────────────────────────────
// Map from URL to estimated quality score (higher = faster)
const mirrorSpeeds = ref<Map<string, number>>(new Map())
// Set of URLs currently being estimated
const estimatingUrls = ref<Set<string>>(new Set())

async function estimateMirrorSpeed(url: string): Promise<void> {
  if (estimatingUrls.value.has(url) || mirrorSpeeds.value.has(url)) return
  estimatingUrls.value = new Set([...estimatingUrls.value, url])
  try {
    const start = Date.now()
    const resp = await fetch(url, { method: 'HEAD', signal: AbortSignal.timeout(3000) })
    const elapsed = Date.now() - start
    const contentLength = parseInt(resp.headers.get('content-length') || '0')
    const score = contentLength > 0 ? contentLength / elapsed * 1000 : 1000 / elapsed
    mirrorSpeeds.value = new Map([...mirrorSpeeds.value, [url, score]])
  } catch {
    // Mark as 0 so we don't retry
    mirrorSpeeds.value = new Map([...mirrorSpeeds.value, [url, 0]])
  } finally {
    const next = new Set(estimatingUrls.value)
    next.delete(url)
    estimatingUrls.value = next
  }
}

function getBestMirrorUrl(): string | null {
  if (props.results.length === 0) return null
  let best: string | null = null
  let bestScore = -1
  for (const result of props.results) {
    const speed = mirrorSpeeds.value.get(result.url) ?? -1
    if (speed > bestScore) {
      bestScore = speed
      best = result.url
    }
  }
  return bestScore > 0 ? best : null
}

function isBestMirror(url: string): boolean {
  return getBestMirrorUrl() === url
}

// Kick off estimation for each new result (up to 10 to avoid too many requests)
watch(() => props.results, (newResults) => {
  const toEstimate = newResults.filter((r) => !mirrorSpeeds.value.has(r.url)).slice(0, 10)
  for (const r of toEstimate) {
    void estimateMirrorSpeed(r.url)
  }
}, { deep: true })

const emit = defineEmits<{
  (e: 'close'): void
  (e: 'search'): void
  (e: 'copy-all'): void
  (e: 'copy-one', url: string): void
}>()

const { t } = useI18n()
const modalRef = ref<HTMLElement | null>(null)
const searchButtonRef = ref<HTMLElement | null>(null)
const logRef = ref<HTMLElement | null>(null)
const titleId = `mirrors-title-${Math.random().toString(36).slice(2, 10)}`

function focusDialog(): void {
  nextTick(() => {
    searchButtonRef.value?.focus()
    focusFirstDialogElement(modalRef.value)
  })
}

function onDialogKeydown(event: KeyboardEvent): void {
  if (event.key === 'Escape') {
    event.preventDefault()
    event.stopPropagation()
    emit('close')
    return
  }
  trapDialogTab(event, modalRef.value)
}

watch(() => props.log.length, () => {
  nextTick(() => {
    if (logRef.value) {
      logRef.value.scrollTop = logRef.value.scrollHeight
    }
  })
})

onMounted(() => {
  focusDialog()
})
</script>

<style scoped>
.mirrors-modal-backdrop {
  position: fixed;
  inset: 0;
  background: color-mix(in srgb, #0f172a 34%, transparent);
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 28px;
  z-index: 80;
}

.mirrors-modal {
  width: min(1180px, 100%);
  max-height: min(860px, calc(100vh - 48px));
  overflow: auto;
  border-radius: 22px;
  border: 1px solid var(--border-color);
  background: var(--bg-card);
  box-shadow: var(--shadow-lg);
  padding: 22px;
  display: flex;
  flex-direction: column;
  gap: 18px;
  outline: none;
}

.mirrors-modal-header,
.mirrors-modal-copy,
.mirrors-stats,
.mirrors-body,
.mirrors-grid,
.mirrors-results-head,
.mirrors-log-head,
.mirrors-progress-copy,
.mirrors-title-row,
.mirrors-modal-actions {
  display: flex;
}

.mirrors-modal-header,
.mirrors-results-head,
.mirrors-log-head,
.mirrors-progress-copy {
  align-items: center;
  justify-content: space-between;
}

.mirrors-modal-copy,
.mirrors-body {
  flex-direction: column;
}

.mirrors-stats,
.mirrors-grid,
.mirrors-modal-actions,
.mirrors-title-row {
  gap: 12px;
}

.mirrors-stats {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
}

.mirrors-stat-card,
.mirrors-results-wrap,
.mirrors-log-wrap,
.mirrors-progress-wrap {
  border: 1px solid var(--border-color);
  border-radius: 16px;
  background: var(--bg-panel);
}

.mirrors-stat-card {
  padding: 14px;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.mirrors-stat-label,
.mirrors-modal-subtitle,
.mirrors-log-head small,
.mirrors-stat-card small {
  color: var(--text-muted);
}

.mirrors-title {
  font-weight: 700;
  font-size: 18px;
  color: var(--text-primary);
}

.mirrors-file-card {
  padding: 14px 16px;
  border-radius: 16px;
  border: 1px solid var(--border-color);
  background: var(--bg-panel);
  color: var(--text-primary);
  font-weight: 600;
}

.mirrors-progress-wrap,
.mirrors-results-wrap,
.mirrors-log-wrap {
  padding: 14px;
}

.mirrors-progress-track {
  height: 10px;
  border-radius: 999px;
  background: color-mix(in srgb, var(--accent-color) 10%, transparent);
  overflow: hidden;
}

.mirrors-progress-fill {
  height: 100%;
  background: linear-gradient(90deg, var(--accent-color), color-mix(in srgb, var(--accent-color) 70%, white));
}

.mirrors-grid {
  display: grid;
  grid-template-columns: minmax(0, 1.2fr) minmax(360px, 0.8fr);
}

.mirrors-results-list {
  display: flex;
  flex-direction: column;
  gap: 10px;
  max-height: 420px;
  overflow: auto;
}

.mirror-result-card,
.btn-find-mirrors,
.mirrors-copy-btn {
  border-radius: 14px;
  border: 1px solid var(--border-color);
}

.mirror-result-card {
  padding: 12px;
  text-align: left;
  background: var(--bg-card);
  color: inherit;
}

.mirror-result-top,
.mirror-result-meta {
  display: flex;
  justify-content: space-between;
  gap: 12px;
}

.mirror-result-badges {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
}

.mirror-badge-best {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 2px 8px;
  border-radius: 999px;
  font-size: 10px;
  font-weight: 700;
  background: rgba(251, 191, 36, 0.15);
  color: #fbbf24;
  border: 1px solid rgba(251, 191, 36, 0.3);
}

.mirror-badge-estimating {
  display: inline-flex;
  align-items: center;
  font-size: 10px;
  color: var(--text-muted);
  opacity: 0.7;
}

.mirror-result-best {
  border-color: color-mix(in srgb, #fbbf24 35%, var(--border-color)) !important;
  background: color-mix(in srgb, var(--bg-card) 92%, rgba(251, 191, 36, 0.08)) !important;
}

.mirror-result-url {
  margin: 8px 0;
  word-break: break-all;
}

.mirrors-log {
  margin: 0;
  max-height: 420px;
  overflow: auto;
  white-space: pre-wrap;
  color: var(--text-secondary);
}

.btn-find-mirrors,
.mirrors-copy-btn {
  background: var(--bg-card);
  color: var(--text-primary);
  padding: 10px 14px;
}

.mirrors-empty {
  padding: 18px 0 6px;
  color: var(--text-muted);
}

@media (max-width: 980px) {
  .mirrors-stats,
  .mirrors-grid {
    grid-template-columns: 1fr;
  }
}
</style>
