<template>
  <div class="account-panel">
    <div class="account-header">
      <h2 class="account-title">{{ t('accountTitle') }}</h2>
      <p class="account-sub">{{ t('accountSub') }}</p>
    </div>

    <section class="account-card">
      <div class="provider-row">
        <div class="provider-meta">
          <img
            src="../assets/provider-icons/terabox.svg"
            alt="Terabox"
            width="18"
            height="18"
          />
          <div class="provider-copy">
            <strong>Terabox</strong>
            <span>{{ t('accountTeraboxDesc') }}</span>
          </div>
        </div>
        <span class="provider-status" :class="{ connected: loggedIn }">
          {{ loggedIn ? t('accountConnected') : t('accountDisconnected') }}
        </span>
      </div>

      <p class="account-note">
        Conecte-se via browser integrado — não armazenamos sua senha.
      </p>
      <p v-if="feedback" class="account-feedback" :class="{ success: loggedIn, error: !loggedIn && !!feedback }">
        {{ feedback }}
      </p>

      <div class="actions">
        <button v-if="!loggedIn" class="primary-btn" :disabled="loading" @click="saveAccount">
          {{ loading ? 'Abrindo login...' : 'Conectar Terabox' }}
        </button>
        <button v-if="loggedIn" class="ghost-btn" @click="disconnect">{{ t('accountDisconnect') }}</button>
      </div>
    </section>
  </div>
</template>

<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useI18n } from '../i18n'

const { t } = useI18n()

const loggedIn = ref(false)
const loading = ref(false)
const feedback = ref('')

onMounted(async () => {
  const isLogged = await window.api.auth.isLoggedIn('terabox').catch(() => false)
  loggedIn.value = isLogged
  if (isLogged) {
    const account = await window.api.auth.accountInfo('terabox').catch(() => null)
    if (account && 'verifiedAt' in account && account.verifiedAt) {
      feedback.value = `Conectado em ${new Date(String(account.verifiedAt)).toLocaleString()}`
    }
  }
})

async function saveAccount(): Promise<void> {
  feedback.value = ''
  loading.value = true
  try {
    await window.api.auth.login('terabox', {})
    loggedIn.value = true
    feedback.value = 'Conta conectada com sucesso!'
  } catch (error) {
    loggedIn.value = false
    feedback.value = error instanceof Error ? error.message : String(error)
  } finally {
    loading.value = false
  }
}

async function disconnect(): Promise<void> {
  await window.api.auth.logout('terabox')
  loggedIn.value = false
  feedback.value = ''
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
