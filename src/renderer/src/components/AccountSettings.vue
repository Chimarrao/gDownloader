<template>
  <div class="account-panel">
    <div class="account-header">
      <h2 class="account-title">{{ t('accountTitle') }}</h2>
      <p class="account-sub">{{ t('accountSub') }}</p>
    </div>

    <section
      v-for="card in cards"
      :key="card.id"
      class="account-card"
    >
      <div class="provider-row">
        <div class="provider-meta">
          <div v-html="card.icon.svg" class="provider-icon"></div>
          <div class="provider-copy">
            <strong>{{ card.name }}</strong>
            <span>{{ card.description }}</span>
          </div>
        </div>
        <span class="provider-status" :class="{ connected: card.connected }">
          {{ card.connected ? t('accountConnected') : t('accountDisconnected') }}
        </span>
      </div>

      <p
        v-for="note in card.notes"
        :key="`${card.id}:${note}`"
        class="account-note"
      >
        {{ note }}
      </p>
      <p v-if="card.feedback" class="account-feedback" :class="{ success: card.connected, error: !card.connected && !!card.feedback }">
        {{ card.feedback }}
      </p>

      <div class="actions">
        <button v-if="!card.connected" class="primary-btn" :disabled="card.loading" @click="card.connect">
          {{ card.loading ? t('accountOpeningLogin') : card.connectLabel }}
        </button>
        <button v-if="card.connected" class="ghost-btn" @click="card.disconnect">{{ t('accountDisconnect') }}</button>
      </div>
    </section>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useI18n } from '../i18n'
import { getProviderIcon } from '../assets/provider-icons'

const { t, locale } = useI18n()

interface ProviderCardSummary {
  id: string
  name: string
  description?: string
  capabilities?: {
    supportsManualAuth?: boolean
    supportsFolder?: boolean
    requiresBrowserHelper?: boolean
    requiresAccountForLargeFiles?: boolean
  }
  accountState?: {
    connected: boolean
    verifiedAt?: string | null
  } | null
}

const providerCards = ref<ProviderCardSummary[]>([])
const loadingById = ref<Record<string, boolean>>({})
const feedbackById = ref<Record<string, string>>({})

function isKnownAuthProvider(id: string): boolean {
  return id === 'terabox'
}

function providerNotes(card: ProviderCardSummary): string[] {
  const notes = [t('accountLocalSqliteNote')]

  if (card.id === 'terabox') {
    notes.push(t('accountTeraboxTempCopyNote'))
  }
  if (card.capabilities?.requiresBrowserHelper) {
    notes.push(t('accountBrowserHelperNote'))
  }
  if (card.capabilities?.requiresAccountForLargeFiles) {
    notes.push(t('accountFreeLimitNote'))
  }
  if (card.capabilities?.supportsFolder) {
    notes.push(t('accountSharedSessionNote'))
  }

  return notes
}

function providerDescription(card: ProviderCardSummary): string {
  if (card.description?.trim()) {
    return card.description
  }
  if (card.id === 'terabox') {
    return t('accountTeraboxDesc')
  }
  return card.name
}

const cards = computed(() => providerCards.value
  .filter((card) => isKnownAuthProvider(card.id) && card.capabilities?.supportsManualAuth)
  .map((card) => ({
    ...card,
    description: providerDescription(card),
    notes: providerNotes(card),
    feedback: feedbackById.value[card.id] ?? '',
    connected: card.accountState?.connected ?? false,
    loading: !!loadingById.value[card.id],
    icon: getProviderIcon(card.id),
    connectLabel: `${t('accountConnect')} ${card.name}`,
    connect: () => connectProvider(card.id),
    disconnect: () => disconnectProvider(card.id),
  })))

async function refreshProviders(): Promise<void> {
  const modules = await window.api.modules.list().catch(() => [])
  providerCards.value = Array.isArray(modules) ? modules : []

  for (const card of providerCards.value.filter((entry) => isKnownAuthProvider(entry.id))) {
    const account = await window.api.auth.accountInfo(card.id).catch(() => null) as { verifiedAt?: string } | null
    if (account?.verifiedAt) {
      feedbackById.value = {
        ...feedbackById.value,
        [card.id]: `${t('accountConnectedAt')} ${new Date(String(account.verifiedAt)).toLocaleString(locale.value)}`,
      }
    }
  }
}

onMounted(async () => {
  await refreshProviders()
})

async function connectProvider(moduleId: string): Promise<void> {
  feedbackById.value = {
    ...feedbackById.value,
    [moduleId]: '',
  }
  loadingById.value = {
    ...loadingById.value,
    [moduleId]: true,
  }
  try {
    await window.api.auth.login(moduleId, {})
    await refreshProviders()
    feedbackById.value = {
      ...feedbackById.value,
      [moduleId]: t('accountConnectedSuccess'),
    }
  } catch (error) {
    feedbackById.value = {
      ...feedbackById.value,
      [moduleId]: error instanceof Error ? error.message : String(error),
    }
  } finally {
    loadingById.value = {
      ...loadingById.value,
      [moduleId]: false,
    }
  }
}

async function disconnectProvider(moduleId: string): Promise<void> {
  await window.api.auth.logout(moduleId)
  feedbackById.value = {
    ...feedbackById.value,
    [moduleId]: '',
  }
  await refreshProviders()
}
</script>

<style scoped>
.account-panel {
  width: 100%;
  max-width: 760px;
  padding: 24px 28px;
  display: flex;
  flex-direction: column;
  gap: 20px;
  overflow-y: auto;
}

.account-header {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.account-title {
  margin: 0;
  font-size: 18px;
  font-weight: 700;
  color: var(--text-primary);
}

.account-sub {
  margin: 0;
  font-size: 12px;
  color: var(--text-muted);
}

.account-card {
  display: flex;
  flex-direction: column;
  gap: 16px;
  padding: 18px;
  border: 1px solid var(--border-color);
  border-radius: 16px;
  background: var(--bg-card);
}

.provider-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
}

.provider-meta {
  display: flex;
  align-items: center;
  gap: 12px;
}

.provider-icon {
  width: 18px;
  height: 18px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
}

.provider-icon :deep(svg) {
  width: 18px;
  height: 18px;
  display: block;
}

.provider-copy {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.provider-copy span {
  font-size: 12px;
  color: var(--text-muted);
}

.provider-status {
  padding: 6px 10px;
  border-radius: 999px;
  font-size: 11px;
  font-weight: 700;
  background: color-mix(in srgb, #ef4444 14%, transparent);
  color: #ef4444;
}

.provider-status.connected {
  background: color-mix(in srgb, #22c55e 14%, transparent);
  color: #22c55e;
}

.field-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 14px;
}

.field {
  display: flex;
  flex-direction: column;
  gap: 6px;
  font-size: 12px;
  color: var(--text-muted);
}

.field-input {
  border: 1px solid var(--border-color);
  background: var(--bg-secondary);
  color: var(--text-primary);
  border-radius: 10px;
  padding: 10px 12px;
  outline: none;
}

.field-input:focus {
  border-color: var(--accent-color);
}

.account-note {
  margin: 0;
  font-size: 12px;
  color: var(--text-muted);
}

.account-feedback {
  margin: 0;
  font-size: 12px;
}

.account-feedback.success {
  color: #22c55e;
}

.account-feedback.error {
  color: #ef4444;
}

.actions {
  display: flex;
  gap: 10px;
}

.primary-btn,
.ghost-btn {
  border-radius: 10px;
  padding: 10px 14px;
  font-size: 12px;
  font-weight: 700;
  cursor: pointer;
}

.primary-btn {
  border: none;
  background: var(--accent-color);
  color: white;
}

.ghost-btn {
  border: 1px solid var(--border-color);
  background: transparent;
  color: var(--text-primary);
}

.ghost-btn:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

@media (max-width: 720px) {
  .field-grid {
    grid-template-columns: 1fr;
  }
}
</style>
