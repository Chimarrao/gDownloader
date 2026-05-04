<template>
  <div class="settings-panel">
    <div class="settings-header">
      <h2 class="settings-title">{{ t('settingsTitle') }}</h2>
      <p class="settings-sub">{{ t('settingsSub') }}</p>
      <p v-if="saveFeedback" class="settings-feedback" :class="{ error: saveFeedbackError }">
        {{ saveFeedback }}
      </p>
    </div>

    <!-- Download section -->
    <div class="settings-section">
      <h3 class="section-title">{{ t('downloadsSection') }}</h3>

      <div class="setting-row" data-tour="download-folder">
        <div class="setting-info">
          <span class="setting-label">{{ t('outputFolder') }}</span>
          <span class="setting-desc">{{ t('outputFolderDesc') }}</span>
        </div>
        <div class="output-folder-actions">
          <input
            v-model="settings.outputDir"
            class="setting-input setting-input-wide"
            placeholder="~/Downloads"
            @change="save"
          />
          <button class="browse-btn" @click="chooseDirectory">{{ t('choose') }}</button>
        </div>
      </div>

      <div class="setting-row">
        <div class="setting-info">
          <span class="setting-label">{{ t('concurrentDownloads') }}</span>
          <span class="setting-desc">{{ t('concurrentDownloadsDesc') }}</span>
        </div>
        <select v-model="settings.maxConcurrentDownloads" class="setting-select" @change="save">
          <option v-for="n in [1,2,3,4,5,8,10]" :key="n" :value="n">{{ n }}</option>
        </select>
      </div>

      <div class="setting-row">
        <div class="setting-info">
          <span class="setting-label">{{ t('maxRetries') }}</span>
          <span class="setting-desc">{{ t('maxRetriesDesc') }}</span>
        </div>
        <select v-model="settings.maxRetriesPerDownload" class="setting-select" @change="save">
          <option v-for="n in [0,1,2,3,5,8,10]" :key="n" :value="n">{{ n }}</option>
        </select>
      </div>

      <div class="setting-row">
        <div class="setting-info">
          <span class="setting-label">{{ t('parallelParts') }}</span>
          <span class="setting-desc">{{ t('parallelPartsDesc') }}</span>
        </div>
        <select v-model="settings.parallelPartsPerDownload" class="setting-select" @change="save">
          <option v-for="n in [1,2,4,6,8]" :key="n" :value="n">{{ n }}</option>
        </select>
      </div>

      <div class="setting-row">
        <div class="setting-info">
          <span class="setting-label">{{ t('speedLimitSetting') }}</span>
          <span class="setting-desc">{{ t('speedLimitDesc') }}</span>
        </div>
        <input
          v-model.number="settings.speedLimitKib"
          type="number"
          min="0"
          step="100"
          class="setting-input"
          @change="save"
        />
      </div>

      <div class="setting-row">
        <div class="setting-info">
          <span class="setting-label">Duplicatas</span>
          <span class="setting-desc">Ação padrão quando o arquivo já existe na fila ou no histórico</span>
        </div>
        <select v-model="settings.duplicateAction" class="setting-select" @change="save">
          <option value="ask">Perguntar</option>
          <option value="skip">Ignorar existente</option>
          <option value="rename">Salvar com sufixo _2</option>
          <option value="always_download">Baixar mesmo assim</option>
        </select>
      </div>

      <div class="setting-row setting-row-stack" data-tour="browser-capture">
        <div class="setting-info">
          <span class="setting-label">Interceptação local</span>
          <span class="setting-desc">Proxy em 127.0.0.1:9667 para capturar downloads de apps no mesmo computador</span>
        </div>
        <div class="remote-actions">
          <select v-model="settings.interceptMode" class="setting-select" @change="save">
            <option value="off">Desativado</option>
            <option value="proxy_only">Proxy local</option>
          </select>
          <input
            v-model.number="settings.interceptMinSizeMb"
            class="setting-input"
            type="number"
            min="0"
            title="Tamanho mínimo em MB"
            @change="save"
          />
          <button class="browse-btn" @click="installInterceptCa">Instalar CA</button>
          <button class="browse-btn" @click="openProxySettings">Configurar proxy do sistema</button>
        </div>
        <div class="setting-grid">
          <label>
            <span>Mimes permitidos</span>
            <textarea
              v-model="interceptMimeText"
              class="setting-textarea"
              placeholder="application/zip&#10;video/"
              @change="saveInterceptLists"
            ></textarea>
          </label>
          <label>
            <span>Domínios ignorados</span>
            <textarea
              v-model="interceptBlockText"
              class="setting-textarea"
              placeholder="bank.com&#10;accounts.google.com"
              @change="saveInterceptLists"
            ></textarea>
          </label>
        </div>
        <p v-if="interceptInfo" class="settings-feedback">
          Proxy: {{ interceptInfo.proxyAddr }} · CA: {{ interceptInfo.caCertPath || 'gerando no backend' }}
        </p>
      </div>

      <div class="setting-row">
        <div class="setting-info">
          <span class="setting-label">{{ t('language') }}</span>
          <span class="setting-desc">{{ t('languageDesc') }}</span>
        </div>
        <select v-model="settings.locale" class="setting-select" @change="onLocaleChange">
          <option value="pt-BR">{{ t('langPtBr') }}</option>
          <option value="en-US">{{ t('langEnUs') }}</option>
        </select>
      </div>
    </div>

    <!-- Appearance section -->
    <div class="settings-section">
      <h3 class="section-title">{{ t('appearance') }}</h3>

      <div class="setting-row">
        <div class="setting-info">
          <span class="setting-label">{{ t('theme') }}</span>
          <span class="setting-desc">{{ t('themeDesc') }}</span>
        </div>
        <select v-model="settings.theme" class="setting-select" @change="onThemeChange">
          <option value="light">{{ t('themeLight') }}</option>
          <option value="system">{{ t('themeSystem') }}</option>
          <option value="dark-purple">{{ t('themeDarkPurple') }}</option>
          <option value="dark-monokai">{{ t('themeDarkMonokai') }}</option>
          <option value="dark-default">{{ t('themeDarkDefault') }}</option>
        </select>
      </div>

      <div class="setting-row">
        <div class="setting-info">
          <span class="setting-label">{{ t('accentColorLabel') }}</span>
          <span class="setting-desc">{{ t('accentColorDesc') }}</span>
        </div>
        <div style="display:flex;gap:8px;align-items:center;">
          <input
            type="color"
            :value="settings.accentColor || '#a855f7'"
            class="color-picker"
            @change="onAccentColorChange"
          />
          <button
            v-if="settings.accentColor"
            class="browse-btn"
            style="padding:6px 10px;font-size:12px;"
            @click="resetAccentColor"
          >{{ t('reset') }}</button>
        </div>
      </div>

      <div class="setting-row">
        <div class="setting-info">
          <span class="setting-label">{{ t('fontFamilyLabel') }}</span>
          <span class="setting-desc">{{ t('fontFamilyDesc') }}</span>
        </div>
        <select v-model="settings.fontFamily" class="setting-select" @change="onAppearanceChange">
          <option value="Inter">Inter</option>
          <option value="IBM Plex Sans">IBM Plex Sans</option>
          <option value="Segoe UI">Segoe UI</option>
          <option value="SF Pro Display">SF Pro Display</option>
        </select>
      </div>

      <div class="setting-row">
        <div class="setting-info">
          <span class="setting-label">{{ t('fontSizeLabel') }}</span>
          <span class="setting-desc">{{ t('fontSizeDesc') }}</span>
        </div>
        <select v-model.number="settings.fontSize" class="setting-select" @change="onAppearanceChange">
          <option v-for="size in [12,13,14,15,16,18]" :key="size" :value="size">{{ size }} px</option>
        </select>
      </div>

      <div class="setting-row">
        <div class="setting-info">
          <span class="setting-label">{{ t('uiZoomLabel') }}</span>
          <span class="setting-desc">{{ t('uiZoomDesc') }}</span>
        </div>
        <select v-model.number="settings.uiZoom" class="setting-select" @change="onAppearanceChange">
          <option v-for="zoom in [0.9,1,1.1,1.2,1.3]" :key="zoom" :value="zoom">{{ zoom.toFixed(1) }}x</option>
        </select>
      </div>
    </div>

    <!-- Integrations section -->
    <div class="settings-section">
      <h3 class="section-title">{{ t('integrationsSection') }}</h3>

      <div class="setting-row" data-tour="captcha-solver">
        <div class="setting-info">
          <span class="setting-label">{{ t('nopechaApiKeyLabel') }}</span>
          <span class="setting-desc">{{ t('nopechaApiKeyDesc') }}</span>
        </div>
        <input
          v-model="settings.nopechaApiKey"
          type="password"
          class="setting-input setting-input-wide"
          placeholder="nopecha_xxxxxxxxx"
          @change="save"
        />
      </div>

      <div class="setting-row">
        <div class="setting-info">
          <span class="setting-label">{{ t('clipboardMonitorLabel') }}</span>
          <span class="setting-desc">{{ t('clipboardMonitorDesc') }}</span>
        </div>
        <label class="toggle">
          <input
            v-model="settings.clipboardMonitorEnabled"
            type="checkbox"
            @change="save"
          />
          <span class="toggle-track">
            <span class="toggle-thumb"></span>
          </span>
        </label>
      </div>
    </div>

    <!-- Notifications section -->
    <div class="settings-section">
      <h3 class="section-title">{{ t('notificationsSection') }}</h3>

      <div class="setting-row">
        <div class="setting-info">
          <span class="setting-label">{{ t('notifyOnComplete') }}</span>
          <span class="setting-desc">{{ t('notifyOnCompleteDesc') }}</span>
        </div>
        <label class="toggle">
          <input
            type="checkbox"
            v-model="settings.nativeNotification"
            @change="save"
          />
          <span class="toggle-track">
            <span class="toggle-thumb"></span>
          </span>
        </label>
      </div>
    </div>

    <!-- Network section -->
    <div class="settings-section">
      <h3 class="section-title">{{ t('networkSection') }}</h3>

      <div class="setting-row">
        <div class="setting-info">
          <span class="setting-label">{{ t('proxyMode') }}</span>
          <span class="setting-desc">{{ t('proxyModeDesc') }}</span>
        </div>
        <select v-model="settings.proxyMode" class="setting-select" @change="save">
          <option value="none">{{ t('proxyNone') }}</option>
          <option value="http">{{ t('proxyHttp') }}</option>
          <option value="socks5">{{ t('proxySocks5') }}</option>
          <option value="tor">{{ t('proxyTor') }}</option>
        </select>
      </div>

      <div v-if="settings.proxyMode !== 'none' && settings.proxyMode !== 'tor'" class="setting-row">
        <div class="setting-info">
          <span class="setting-label">{{ t('proxyHost') }}</span>
          <span class="setting-desc">{{ t('proxyHostDesc') }}</span>
        </div>
        <input
          v-model="settings.proxyHost"
          class="setting-input setting-input-wide"
          placeholder="proxy.example.com"
          @change="save"
        />
      </div>

      <div v-if="settings.proxyMode !== 'none' && settings.proxyMode !== 'tor'" class="setting-row">
        <div class="setting-info">
          <span class="setting-label">{{ t('proxyPort') }}</span>
          <span class="setting-desc">{{ t('proxyPortDesc') }}</span>
        </div>
        <input
          v-model.number="settings.proxyPort"
          type="number"
          min="1"
          max="65535"
          class="setting-input"
          @change="save"
        />
      </div>

      <div v-if="settings.proxyMode !== 'none' && settings.proxyMode !== 'tor'" class="setting-row">
        <div class="setting-info">
          <span class="setting-label">{{ t('proxyUsername') }}</span>
          <span class="setting-desc">{{ t('proxyUsernameDesc') }}</span>
        </div>
        <input
          v-model="settings.proxyUsername"
          class="setting-input setting-input-wide"
          placeholder="username (optional)"
          @change="save"
        />
      </div>

      <div v-if="settings.proxyMode !== 'none' && settings.proxyMode !== 'tor'" class="setting-row">
        <div class="setting-info">
          <span class="setting-label">{{ t('proxyPassword') }}</span>
          <span class="setting-desc">{{ t('proxyPasswordDesc') }}</span>
        </div>
        <input
          v-model="settings.proxyPassword"
          type="password"
          class="setting-input setting-input-wide"
          placeholder="password (optional)"
          @change="save"
        />
      </div>

      <div v-if="settings.proxyMode === 'tor'" class="setting-row">
        <div class="setting-info">
          <span class="setting-label">{{ t('torNote') }}</span>
          <span class="setting-desc">{{ t('torNoteDesc') }}</span>
        </div>
        <div></div>
      </div>

      <div v-if="settings.proxyMode === 'tor'" class="setting-row">
        <div class="setting-info">
          <span class="setting-label">{{ t('startTor') }}</span>
          <span class="setting-desc">{{ t('startTorDesc') }}</span>
        </div>
        <label class="toggle">
          <input
            v-model="settings.startTor"
            type="checkbox"
            @change="save"
          />
          <span class="toggle-track">
            <span class="toggle-thumb"></span>
          </span>
        </label>
      </div>

      <div v-if="settings.proxyMode !== 'none'" class="setting-row">
        <div class="setting-info">
          <span class="setting-label">{{ t('testConnection') }}</span>
          <span class="setting-desc">{{ t('testConnectionDesc') }}</span>
        </div>
        <button class="browse-btn" @click="testConnection">{{ t('test') }}</button>
      </div>
    </div>

    <!-- Remote access section -->
    <div class="settings-section">
      <h3 class="section-title">Acesso remoto local</h3>

      <div class="setting-row">
        <div class="setting-info">
          <span class="setting-label">Web UI na rede local</span>
          <span class="setting-desc">Permite controlar e configurar o app por outro aparelho no mesmo roteador</span>
        </div>
        <label class="toggle">
          <input
            v-model="settings.remoteAccess.enabled"
            type="checkbox"
            @change="save"
          />
          <span class="toggle-track">
            <span class="toggle-thumb"></span>
          </span>
        </label>
      </div>

      <div class="setting-row">
        <div class="setting-info">
          <span class="setting-label">Usuário</span>
          <span class="setting-desc">Login usado no celular ou em outro computador da rede</span>
        </div>
        <input
          v-model="settings.remoteAccess.username"
          class="setting-input setting-input-wide"
          @change="save"
        />
      </div>

      <div class="setting-row">
        <div class="setting-info">
          <span class="setting-label">Senha</span>
          <span class="setting-desc">Senha simples gerada para o acesso local</span>
        </div>
        <div class="remote-inline">
          <input
            v-model="settings.remoteAccess.password"
            class="setting-input"
            type="text"
            @change="save"
          />
          <button class="browse-btn" @click="generateRemoteCredentials">Gerar</button>
        </div>
      </div>

      <div class="setting-row">
        <div class="setting-info">
          <span class="setting-label">Porta local</span>
          <span class="setting-desc">Endereço exposto apenas na rede local, sem TLS público</span>
        </div>
        <input
          v-model.number="settings.remoteAccess.port"
          class="setting-input"
          type="number"
          min="1024"
          max="65535"
          @change="save"
        />
      </div>

      <div class="remote-access-card">
        <div class="remote-access-info">
          <span class="remote-status" :class="{ active: remoteInfo?.running, error: remoteInfo?.error }">
            {{ remoteInfo?.running ? 'Online na rede local' : settings.remoteAccess.enabled ? 'Ativando...' : 'Desativado' }}
          </span>
          <a
            v-if="remoteInfo?.url"
            class="remote-url"
            :href="remoteInfo.url"
            target="_blank"
            rel="noreferrer"
          >
            {{ remoteInfo.url }}
          </a>
          <span v-if="remoteInfo?.error" class="remote-error">{{ remoteInfo.error }}</span>
          <div class="remote-actions">
            <button class="browse-btn" :disabled="!remoteInfo?.url" @click="copyRemoteUrl">Copiar link</button>
            <button class="browse-btn" @click="refreshRemoteInfo">Atualizar</button>
          </div>
        </div>
        <img
          v-if="remoteInfo?.qrCodeDataUrl"
          class="remote-qr"
          :src="remoteInfo.qrCodeDataUrl"
          alt="QR Code do acesso remoto local"
        />
      </div>
    </div>

    <!-- ── Reconnect ─────────────────────────────────────── -->
    <div class="settings-section">
      <h3 class="section-title">{{ t('reconnectSection') }}</h3>

      <div class="setting-row">
        <div class="setting-label-wrap">
          <span class="setting-label">{{ t('reconnectOnRateLimit') }}</span>
          <span class="setting-desc">{{ t('reconnectOnRateLimitDesc') }}</span>
        </div>
        <label class="toggle">
          <input type="checkbox" v-model="settings.useReconnectOnRateLimit" @change="save" />
          <span class="toggle-track"></span>
        </label>
      </div>

      <template v-if="settings.useReconnectOnRateLimit">
        <div class="setting-row">
          <div class="setting-label-wrap">
            <span class="setting-label">{{ t('reconnectMethod') }}</span>
            <span class="setting-desc">{{ t('reconnectMethodDesc') }}</span>
          </div>
          <select class="setting-select" v-model="settings.reconnectMethod" @change="save">
            <option value="none">{{ t('reconnectNone') }}</option>
            <option value="router_script">{{ t('reconnectRouterScript') }}</option>
            <option value="curl_command">{{ t('reconnectCurlCommand') }}</option>
          </select>
        </div>

        <div class="setting-row" v-if="settings.reconnectMethod !== 'none'">
          <div class="setting-label-wrap">
            <span class="setting-label">{{ t('reconnectRouterIp') }}</span>
            <span class="setting-desc">{{ t('reconnectRouterIpDesc') }}</span>
          </div>
          <input
            class="setting-input"
            type="text"
            v-model="settings.routerIp"
            placeholder="192.168.1.1"
            @change="save"
          />
        </div>

        <div class="setting-row" v-if="settings.reconnectMethod !== 'none'">
          <div class="setting-label-wrap">
            <span class="setting-label">{{ t('reconnectCommand') }}</span>
            <span class="setting-desc">{{ t('reconnectCommandDesc') }}</span>
          </div>
          <input
            class="setting-input"
            type="text"
            v-model="settings.reconnectCommand"
            :placeholder="settings.reconnectMethod === 'router_script'
              ? 'curl -s http://{router_ip}/reboot'
              : 'curl -X POST http://{router_ip}/api/reconnect'"
            @change="save"
          />
        </div>
      </template>
    </div>

    <!-- Automation section -->
    <div class="settings-section">
      <h3 class="section-title">{{ t('automationSection') }}</h3>

      <div class="setting-row">
        <div class="setting-info">
          <span class="setting-label">{{ t('postDownloadAction') }}</span>
          <span class="setting-desc">{{ t('postDownloadActionDesc') }}</span>
        </div>
        <select v-model="settings.postDownloadAction" class="setting-select" @change="save">
          <option value="none">{{ t('postDownloadActionNone') }}</option>
          <option value="shutdown">{{ t('postDownloadActionShutdown') }}</option>
          <option value="sleep">{{ t('postDownloadActionSleep') }}</option>
          <option value="custom_command">{{ t('postDownloadActionCommand') }}</option>
          <option value="webhook">{{ t('postDownloadActionWebhook') }}</option>
        </select>
      </div>

      <div v-if="settings.postDownloadAction && settings.postDownloadAction !== 'none'" class="setting-row">
        <div class="setting-info">
          <span class="setting-label">{{ t('postDownloadTrigger') }}</span>
        </div>
        <select v-model="settings.postDownloadActionTrigger" class="setting-select" @change="save">
          <option value="queue_empty">{{ t('postDownloadTriggerQueueEmpty') }}</option>
          <option value="per_item">{{ t('postDownloadTriggerPerItem') }}</option>
        </select>
      </div>

      <div v-if="settings.postDownloadAction === 'custom_command'" class="setting-row">
        <div class="setting-info">
          <span class="setting-label">{{ t('postDownloadCommand') }}</span>
          <span class="setting-desc">{{ t('postDownloadCommandDesc') }}</span>
        </div>
        <input
          v-model="settings.postDownloadCommand"
          class="setting-input setting-input-wide"
          placeholder="notify-send 'Done'"
          @change="save"
        />
      </div>

      <div v-if="settings.postDownloadAction === 'webhook'" class="setting-row">
        <div class="setting-info">
          <span class="setting-label">{{ t('postDownloadWebhookUrl') }}</span>
          <span class="setting-desc">{{ t('postDownloadWebhookUrlDesc') }}</span>
        </div>
        <input
          v-model="settings.postDownloadWebhookUrl"
          class="setting-input setting-input-wide"
          placeholder="https://hooks.example.com/..."
          @change="save"
        />
      </div>
    </div>

    <div class="settings-section">
      <h3 class="section-title">Senhas de archives</h3>

      <div class="setting-row">
        <div class="setting-info">
          <span class="setting-label">Lista aprendida</span>
          <span class="setting-desc">A extração tenta primeiro as 20 senhas com mais acertos</span>
        </div>
        <button class="browse-btn" @click="loadArchivePasswords">Atualizar</button>
      </div>

      <div class="archive-password-list">
        <div v-if="archivePasswords.length === 0" class="archive-password-empty">
          Nenhuma senha aprendida ainda.
        </div>
        <div v-for="entry in archivePasswords" :key="entry.password" class="archive-password-row">
          <code>{{ entry.password }}</code>
          <span>{{ entry.successCount }} hit(s)</span>
          <span>{{ entry.source }}</span>
          <button class="browse-btn" @click="forgetArchivePassword(entry.password)">Esquecer</button>
        </div>
      </div>

      <div class="setting-row setting-row-stack">
        <div class="setting-info">
          <span class="setting-label">Importar / exportar</span>
          <span class="setting-desc">Uma senha por linha. Exportar copia a lista para a área de transferência</span>
        </div>
        <textarea
          v-model="archivePasswordImport"
          class="setting-textarea"
          placeholder="senha1&#10;senha2"
        ></textarea>
        <div class="remote-actions">
          <button class="browse-btn" @click="importArchivePasswords">Importar lista</button>
          <button class="browse-btn" @click="exportArchivePasswords">Exportar</button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { onMounted, reactive, ref } from 'vue'
import type { AppSettingsSnapshot, ArchivePassword } from '../../../shared/types'
import { applyUiPreferences, useTheme, type ThemeId } from '../themes'
import { setLocale, useI18n } from '../i18n'

const { setTheme, themeOptions } = useTheme()
const { t } = useI18n()

const settings = reactive<AppSettingsSnapshot>({
  outputDir: '~/Downloads',
  maxConcurrentDownloads: 3,
  maxRetriesPerDownload: 3,
  parallelPartsPerDownload: 4,
  speedLimitKib: 0,
  theme: 'light',
  nativeNotification: true,
  locale: 'pt-BR',
  fontSize: 14,
  fontFamily: 'Inter',
  uiZoom: 1,
  accentColor: undefined,
  clipboardMonitorEnabled: false,
  nopechaApiKey: undefined,
  proxyMode: 'none',
  proxyHost: '',
  proxyPort: 0,
  proxyUsername: undefined,
  proxyPassword: undefined,
  startTor: false,
  useReconnectOnRateLimit: false,
  reconnectMethod: 'none',
  reconnectCommand: '',
  routerIp: '',
  postDownloadAction: 'none',
  postDownloadActionTrigger: 'queue_empty',
  postDownloadCommand: '',
  postDownloadWebhookUrl: '',
  duplicateAction: 'ask',
  uiDensity: 'comfortable',
  interceptMode: 'off',
  interceptMinSizeMb: 1,
  interceptMimeAllowlist: [
    'application/zip',
    'application/x-rar',
    'application/x-7z-compressed',
    'application/octet-stream',
    'video/',
    'audio/',
    'application/pdf',
  ],
  interceptDomainBlocklist: [],
  interceptAskBeforeAdd: false,
  onboardingCompleted: false,
  remoteAccess: {
    enabled: false,
    username: 'gdownloader',
    password: 'gd-1234',
    port: 9786,
  },
})
let saveFeedbackTimer: ReturnType<typeof setTimeout> | null = null
const saveFeedback = ref('')
const saveFeedbackError = ref(false)
const remoteInfo = ref<Awaited<ReturnType<typeof window.api.remoteAccess.info>> | null>(null)
const interceptInfo = ref<Awaited<ReturnType<typeof window.api.intercept.status>> | null>(null)
const archivePasswords = ref<ArchivePassword[]>([])
const archivePasswordImport = ref('')
const interceptMimeText = ref('')
const interceptBlockText = ref('')

function setSaveFeedback(message: string, error = false): void {
  saveFeedback.value = message
  saveFeedbackError.value = error
  if (saveFeedbackTimer) {
    clearTimeout(saveFeedbackTimer)
  }
  saveFeedbackTimer = setTimeout(() => {
    saveFeedback.value = ''
    saveFeedbackError.value = false
  }, error ? 6000 : 2200)
}

onMounted(async () => {
  const saved = await window.api.settings.load().catch(() => null)
  if (saved) {
    Object.assign(settings, saved)
    syncInterceptText()
    setLocale(saved.locale)
    if (themeOptions.some((option) => option.id === saved.theme)) {
      setTheme(saved.theme as ThemeId)
    } else {
      setTheme('light')
    }
    applyUiPreferences(saved)
    await refreshRemoteInfo()
    await refreshInterceptInfo()
    await loadArchivePasswords()
  }
})

async function save(): Promise<void> {
  try {
    const persisted = await window.api.settings.save({ ...settings })
    Object.assign(settings, persisted)
    syncInterceptText()
    await refreshRemoteInfo()
    await refreshInterceptInfo()
    setSaveFeedback(t('settingsSaved'))
  } catch (error) {
    setSaveFeedback(
      error instanceof Error ? error.message : String(error),
      true,
    )
  }
}

function syncInterceptText(): void {
  interceptMimeText.value = (settings.interceptMimeAllowlist ?? []).join('\n')
  interceptBlockText.value = (settings.interceptDomainBlocklist ?? []).join('\n')
}

function lines(value: string): string[] {
  return value
    .split(/\r?\n/)
    .map((entry) => entry.trim())
    .filter(Boolean)
}

async function saveInterceptLists(): Promise<void> {
  settings.interceptMimeAllowlist = lines(interceptMimeText.value)
  settings.interceptDomainBlocklist = lines(interceptBlockText.value)
  await save()
}

async function refreshInterceptInfo(): Promise<void> {
  interceptInfo.value = await window.api.intercept.status().catch(() => null)
}

async function installInterceptCa(): Promise<void> {
  await window.api.intercept.installCa().catch(() => false)
  await refreshInterceptInfo()
}

async function openProxySettings(): Promise<void> {
  await window.api.intercept.openProxySettings().catch(() => false)
}

async function chooseDirectory(): Promise<void> {
  const chosen = await window.api.settings.chooseDirectory().catch(() => '')
  if (!chosen) return
  settings.outputDir = chosen
  await save()
}

function onThemeChange(): void {
  setTheme(settings.theme as ThemeId)
  applyUiPreferences(settings)
  void save()
}

function onLocaleChange(): void {
  setLocale(settings.locale)
  void save()
}

function onAccentColorChange(e: Event): void {
  const color = (e.target as HTMLInputElement).value
  settings.accentColor = color
  applyUiPreferences(settings)
  void save()
}

function onAppearanceChange(): void {
  applyUiPreferences(settings)
  void save()
}

function resetAccentColor(): void {
  settings.accentColor = undefined
  applyUiPreferences(settings)
  void save()
}

async function testConnection(): Promise<void> {
  try {
    const result = await window.api.config.testProxy()
    setSaveFeedback(`IP atual: ${result.ip}`)
  } catch (error) {
    setSaveFeedback(
      error instanceof Error ? error.message : String(error),
      true,
    )
  }
}

async function refreshRemoteInfo(): Promise<void> {
  remoteInfo.value = await window.api.remoteAccess.info().catch(() => null)
}

async function generateRemoteCredentials(): Promise<void> {
  const generated = await window.api.remoteAccess.generateCredentials()
  settings.remoteAccess = {
    ...generated,
    enabled: settings.remoteAccess.enabled,
    port: settings.remoteAccess.port || generated.port,
  }
  await save()
}

async function copyRemoteUrl(): Promise<void> {
  const value = remoteInfo.value?.credentialUrl || remoteInfo.value?.url
  if (!value) return
  await window.api.clipboard.writeText(value)
  setSaveFeedback('Link remoto copiado')
}

async function loadArchivePasswords(): Promise<void> {
  archivePasswords.value = await window.api.archivePasswords.list().catch(() => [])
}

async function importArchivePasswords(): Promise<void> {
  const passwords = archivePasswordImport.value
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean)
  if (passwords.length === 0) return
  await window.api.archivePasswords.import(passwords)
  archivePasswordImport.value = ''
  await loadArchivePasswords()
  setSaveFeedback('Senhas importadas')
}

async function exportArchivePasswords(): Promise<void> {
  await window.api.clipboard.writeText(archivePasswords.value.map((entry) => entry.password).join('\n'))
  setSaveFeedback('Lista copiada')
}

async function forgetArchivePassword(password: string): Promise<void> {
  await window.api.archivePasswords.forget(password)
  await loadArchivePasswords()
}
</script>

<style scoped>
.settings-panel {
  padding: 24px 28px;
  display: flex;
  flex-direction: column;
  gap: 28px;
  max-width: 640px;
  width: 100%;
  min-height: 0;
  overflow-y: auto;
}

.remote-inline {
  display: flex;
  align-items: center;
  gap: 8px;
}

.remote-access-card {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  gap: 16px;
  border: 1px solid var(--border-color);
  border-radius: 8px;
  padding: 14px;
  background: var(--bg-card);
}

.remote-access-info {
  display: flex;
  min-width: 0;
  flex-direction: column;
  gap: 8px;
}

.remote-status {
  width: fit-content;
  padding: 4px 8px;
  border-radius: 999px;
  background: rgba(148, 163, 184, 0.18);
  color: var(--text-muted);
  font-size: 12px;
  font-weight: 700;
}

.remote-status.active {
  background: rgba(34, 197, 94, 0.14);
  color: #16a34a;
}

.remote-status.error {
  background: rgba(239, 68, 68, 0.14);
  color: #ef4444;
}

.remote-url {
  color: var(--accent-color);
  font-size: 13px;
  font-weight: 700;
  overflow-wrap: anywhere;
  text-decoration: none;
}

.remote-error {
  color: #ef4444;
  font-size: 12px;
}

.remote-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.setting-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
  gap: 10px;
}

.setting-grid label {
  display: flex;
  flex-direction: column;
  gap: 6px;
  color: var(--text-muted);
  font-size: 12px;
}

.remote-qr {
  width: 132px;
  height: 132px;
  border-radius: 8px;
  border: 1px solid var(--border-color);
  background: #fff;
  padding: 6px;
}

.setting-row-stack {
  grid-template-columns: 1fr;
  align-items: stretch;
}

.setting-textarea {
  min-height: 92px;
  width: 100%;
  resize: vertical;
  border: 1px solid var(--border-color);
  border-radius: 8px;
  background: var(--bg-card);
  color: var(--text-primary);
  padding: 10px;
  font: inherit;
  font-size: 12px;
}

.archive-password-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.archive-password-empty,
.archive-password-row {
  padding: 9px 10px;
  border: 1px solid var(--border-color);
  border-radius: 8px;
  background: var(--bg-card);
  color: var(--text-muted);
  font-size: 12px;
}

.archive-password-row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto auto auto;
  align-items: center;
  gap: 8px;
}

.archive-password-row code {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--text-primary);
}

.settings-header {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.settings-title {
  margin: 0;
  font-size: 18px;
  font-weight: 700;
  color: var(--text-primary);
  letter-spacing: -0.3px;
}

.settings-sub {
  margin: 0;
  font-size: 12px;
  color: var(--text-muted);
}

.settings-feedback {
  margin: 0;
  font-size: 12px;
  color: #16a34a;
}

.settings-feedback.error {
  color: #dc2626;
}

/* Section */
.settings-section {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.section-title {
  margin: 0 0 10px;
  font-size: 11px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.8px;
  color: var(--text-muted);
  padding-bottom: 8px;
  border-bottom: 1px solid var(--border-color);
}

/* Setting row */
.setting-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 20px;
  padding: 12px 0;
  border-bottom: 1px solid color-mix(in srgb, var(--border-color) 50%, transparent);
}

.setting-row:last-child {
  border-bottom: none;
}

.setting-info {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.setting-label {
  font-size: 13px;
  font-weight: 500;
  color: var(--text-primary);
}

.setting-desc {
  font-size: 11px;
  color: var(--text-muted);
}

/* Inputs */
.setting-input,
.setting-select {
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: 8px;
  color: var(--text-primary);
  font-size: 13px;
  padding: 7px 12px;
  outline: none;
  transition: border-color 0.15s;
  flex-shrink: 0;
}

.output-folder-actions {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 320px;
}

.browse-btn {
  border: 1px solid var(--border-color);
  background: var(--bg-card);
  color: var(--text-primary);
  border-radius: 8px;
  padding: 8px 12px;
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
}

.setting-input:focus,
.setting-select:focus {
  border-color: var(--accent-color);
}

.setting-input-wide {
  width: 220px;
  font-family: 'JetBrains Mono', 'Courier New', monospace;
}

.setting-select {
  cursor: pointer;
  min-width: 140px;
}

.setting-select option {
  background: var(--bg-secondary);
}

/* Toggle */
.toggle {
  position: relative;
  display: inline-flex;
  cursor: pointer;
  flex-shrink: 0;
}

.toggle input {
  opacity: 0;
  width: 0;
  height: 0;
  position: absolute;
}

.toggle-track {
  width: 42px;
  height: 24px;
  background: var(--border-color);
  border-radius: 999px;
  display: flex;
  align-items: center;
  padding: 2px;
  transition: background 0.2s ease;
}

.toggle input:checked + .toggle-track {
  background: var(--accent-color);
}

.toggle-thumb {
  width: 20px;
  height: 20px;
  background: #fff;
  border-radius: 50%;
  transition: transform 0.2s ease;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.3);
}

.toggle input:checked + .toggle-track .toggle-thumb {
  transform: translateX(18px);
}


.about-version {
  font-size: 12px;
  color: var(--text-muted);
  margin-top: 2px;
}

.color-picker {
  width: 40px;
  height: 32px;
  border: 1px solid var(--border-color);
  border-radius: 8px;
  background: var(--bg-card);
  cursor: pointer;
  padding: 2px;
}
</style>
