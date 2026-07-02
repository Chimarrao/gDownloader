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
          <span class="setting-desc">{{ t('parallelPartsDesc') }} YouTube sempre usa 1 parte.</span>
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
        <div class="speed-limit-control">
          <input
            v-model.number="settings.speedLimitKib"
            type="range"
            min="0"
            max="51200"
            step="100"
            class="setting-range"
            @change="save"
          />
          <input
            v-model.number="settings.speedLimitKib"
            type="number"
            min="0"
            step="100"
            class="setting-input speed-limit-input"
            @change="save"
          />
          <span class="speed-limit-value">{{ speedLimitLabel(settings.speedLimitKib) }}</span>
        </div>
      </div>

      <div class="setting-row">
        <div class="setting-info">
          <span class="setting-label">Duplicatas padrão</span>
          <span class="setting-desc">Ação padrão quando o arquivo já existe na fila ou no histórico</span>
        </div>
        <select v-model="settings.duplicateAction" class="setting-select" @change="save">
          <option value="ask">Perguntar</option>
          <option value="skip">Ignorar existente</option>
          <option value="rename">Salvar com sufixo _2</option>
          <option value="always_download">Baixar mesmo assim</option>
        </select>
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

    <div class="settings-section" data-tour="youtube-settings">
      <h3 class="section-title">YouTube</h3>

      <div class="setting-row">
        <div class="setting-info">
          <span class="setting-label">Usar cookies</span>
          <span class="setting-desc">Usa cookies do navegador ou de um arquivo Netscape para videos restritos</span>
        </div>
        <label class="toggle">
          <input type="checkbox" v-model="settings.youtubeUseCookies" @change="save" />
          <span class="toggle-track">
            <span class="toggle-thumb"></span>
          </span>
        </label>
      </div>

      <div class="setting-row">
        <div class="setting-info">
          <span class="setting-label">Navegador dos cookies</span>
          <span class="setting-desc">Usado quando nenhum arquivo de cookies for informado</span>
        </div>
        <select v-model="settings.youtubeCookieBrowser" class="setting-select" @change="save">
          <option value="chrome">Chrome</option>
          <option value="brave">Brave</option>
          <option value="firefox">Firefox</option>
          <option value="edge">Edge</option>
          <option value="safari">Safari</option>
          <option value="chromium">Chromium</option>
        </select>
      </div>

      <div class="setting-row">
        <div class="setting-info">
          <span class="setting-label">Arquivo de cookies</span>
          <span class="setting-desc">Opcional. Se preenchido, tem prioridade sobre cookies do navegador</span>
        </div>
        <input
          v-model="settings.youtubeCookiesFile"
          class="setting-input setting-input-wide"
          placeholder="/caminho/cookies.txt"
          @change="save"
        />
      </div>

      <div class="setting-row">
        <div class="setting-info">
          <span class="setting-label">Container de merge</span>
          <span class="setting-desc">Formato final quando yt-dlp junta video e audio com ffmpeg</span>
        </div>
        <select v-model="settings.youtubeMergeFormat" class="setting-select" @change="save">
          <option value="mp4">MP4</option>
          <option value="mkv">MKV</option>
          <option value="webm">WEBM</option>
        </select>
      </div>

      <div class="setting-row">
        <div class="setting-info">
          <span class="setting-label">Legendas</span>
          <span class="setting-desc">Baixa legendas manuais e automáticas nos idiomas escolhidos</span>
        </div>
        <label class="setting-toggle">
          <input type="checkbox" v-model="settings.youtubeDownloadSubs" @change="save" />
          <span>{{ settings.youtubeDownloadSubs ? 'Baixar' : 'Nao baixar' }}</span>
        </label>
      </div>

      <div class="setting-row">
        <div class="setting-info">
          <span class="setting-label">Idiomas das legendas</span>
          <span class="setting-desc">Exemplo: pt,en ou all</span>
        </div>
        <input v-model="settings.youtubeSubLangs" class="setting-input" placeholder="pt,en" @change="save" />
      </div>

      <div class="setting-row">
        <div class="setting-info">
          <span class="setting-label">Embutir legendas</span>
          <span class="setting-desc">Requer ffmpeg e container compatível</span>
        </div>
        <label class="setting-toggle">
          <input type="checkbox" v-model="settings.youtubeEmbedSubs" @change="save" />
          <span>{{ settings.youtubeEmbedSubs ? 'Ativado' : 'Desativado' }}</span>
        </label>
      </div>

      <div class="setting-row">
        <div class="setting-info">
          <span class="setting-label">Separar por capitulos</span>
          <span class="setting-desc">Quando disponível, gera arquivos separados por capítulo</span>
        </div>
        <label class="setting-toggle">
          <input type="checkbox" v-model="settings.youtubeSplitChapters" @change="save" />
          <span>{{ settings.youtubeSplitChapters ? 'Ativado' : 'Desativado' }}</span>
        </label>
      </div>

      <div class="setting-row">
        <div class="setting-info">
          <span class="setting-label">yt-dlp</span>
          <span class="setting-desc" v-if="ytdlpStatus.state === 'ready'">
            v{{ ytdlpStatus.version }}<span v-if="ytdlpStatus.updateAvailable"> · atualização disponível</span>
          </span>
          <span class="setting-desc" v-else-if="ytdlpStatus.state === 'downloading'">
            <span v-if="ytdlpProgress">Baixando... {{ Math.round((ytdlpProgress.bytesDownloaded / Math.max(ytdlpProgress.totalBytes, 1)) * 100) }}%</span>
            <span v-else>Baixando yt-dlp...</span>
          </span>
          <span class="setting-desc" v-else-if="ytdlpStatus.state === 'error'" style="color: var(--color-error, #e74c3c)">
            Erro: {{ ytdlpStatus.error ?? 'falha ao obter yt-dlp' }}
          </span>
        </div>
        <button
          class="btn-secondary"
          :disabled="ytdlpCheckingUpdate || ytdlpStatus.state === 'downloading'"
          @click="checkYtdlpUpdate"
        >
          {{ ytdlpCheckingUpdate ? 'Verificando...' : 'Verificar agora' }}
        </button>
      </div>

      <div class="setting-row">
        <div class="setting-info">
          <span class="setting-label">Atualizar automaticamente</span>
          <span class="setting-desc">Verifica e baixa novas versões do yt-dlp ao abrir o app</span>
        </div>
        <label class="toggle">
          <input type="checkbox" v-model="settings.ytdlpAutoUpdate" @change="save" />
          <span class="toggle-track">
            <span class="toggle-thumb"></span>
          </span>
        </label>
      </div>

      <div class="setting-row">
        <div class="setting-info">
          <span class="setting-label">Caminho do yt-dlp (avançado)</span>
          <span class="setting-desc">Deixe em branco para usar o binário gerenciado pelo app</span>
        </div>
        <input
          v-model="settings.ytdlpBinPath"
          class="setting-input setting-input-wide"
          placeholder="(gerenciado automaticamente)"
          @change="save"
        />
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

    <div class="settings-section">
      <h3 class="section-title">Outros</h3>

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
            <button class="browse-btn" :disabled="!remoteInfo?.qrCodeDataUrl" @click="showRemoteQr = !showRemoteQr">
              {{ showRemoteQr ? 'Ocultar QR Code' : 'Gerar QR Code' }}
            </button>
            <button class="browse-btn" @click="refreshRemoteInfo">Atualizar</button>
          </div>
        </div>
        <img
          v-if="showRemoteQr && remoteInfo?.qrCodeDataUrl"
          class="remote-qr"
          :src="remoteInfo.qrCodeDataUrl"
          alt="QR Code do acesso remoto local"
        />
      </div>
      <div v-if="remoteInfo?.insecureCredentials" class="remote-security-alert">
        Credenciais padrão detectadas. Altere usuário e senha antes de manter o acesso remoto ativo na rede local.
      </div>
      <div class="remote-sessions">
        <div class="remote-sessions-header">
          <strong>Sessões conectadas</strong>
          <button class="browse-btn" @click="refreshRemoteInfo">Atualizar</button>
        </div>
        <div v-if="!(remoteInfo?.sessions?.length)" class="remote-session-empty">Nenhum dispositivo autenticado por token.</div>
        <div
          v-for="session in remoteInfo?.sessions ?? []"
          :key="session.id"
          class="remote-session-row"
        >
          <div>
            <strong>{{ session.ip }} <span v-if="session.current">(atual)</span></strong>
            <span>{{ session.userAgent }}</span>
            <em>{{ new Date(session.lastSeenAt).toLocaleString() }}</em>
          </div>
          <button class="browse-btn" :disabled="session.current" @click="revokeRemoteSession(session.id)">Derrubar</button>
        </div>
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
          <span class="toggle-track">
            <span class="toggle-thumb"></span>
          </span>
        </label>
      </div>
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
      <h3 class="section-title">Cache local</h3>

      <div class="cache-summary">
        <div>
          <strong>{{ fmtCacheBytes(cacheInfo?.totalBytes ?? 0) }}</strong>
          <span>dados temporários e metadados locais</span>
        </div>
        <div class="remote-actions">
          <button class="browse-btn" @click="refreshCacheInfo">Atualizar</button>
          <button class="browse-btn" :disabled="clearingCache || !(cacheInfo?.items?.some(item => item.clearable && item.bytes > 0))" @click="clearCache()">
            {{ clearingCache ? 'Limpando...' : 'Limpar tudo' }}
          </button>
        </div>
      </div>

      <div class="cache-list">
        <div v-if="!(cacheInfo?.items?.length)" class="cache-empty">
          Nenhum cache local identificado.
        </div>
        <div v-for="item in cacheInfo?.items ?? []" :key="item.id" class="cache-row">
          <div>
            <strong>{{ item.label }}</strong>
            <span>{{ item.description }}</span>
            <em v-if="item.entries !== undefined">{{ item.entries }} entrada(s)</em>
          </div>
          <span>{{ fmtCacheBytes(item.bytes) }}</span>
          <button class="browse-btn" :disabled="clearingCache || !item.clearable || item.bytes <= 0" @click="clearCache([item.id])">
            Limpar
          </button>
        </div>
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
import { onMounted, onUnmounted, reactive, ref } from 'vue'
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
  reservedDiskMb: 500,
  useReconnectOnRateLimit: false,
  reconnectMethod: 'none',
  reconnectCommand: '',
  routerIp: '',
  postDownloadAction: 'none',
  postDownloadActionTrigger: 'queue_empty',
  postDownloadCommand: '',
  postDownloadWebhookUrl: '',
  duplicateAction: 'rename',
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
  youtubeUseCookies: true,
  youtubeCookieBrowser: 'chrome',
  youtubeCookiesFile: '',
  youtubeMergeFormat: 'mp4',
  youtubeDownloadSubs: false,
  youtubeSubLangs: 'pt,en',
  youtubeEmbedSubs: false,
  youtubeSplitChapters: false,
  ytdlpAutoUpdate: true,
  ytdlpBinPath: '',
  remoteAccess: {
    enabled: false,
    username: 'gdownloader',
    password: 'gd-1234',
    port: 9786,
  },
})
interface YtdlpStatus {
  version: string | null
  updateAvailable: boolean
  state: 'ready' | 'downloading' | 'error'
  error?: string
}

const ytdlpStatus = ref<YtdlpStatus>({ version: null, updateAvailable: false, state: 'downloading' })
const ytdlpProgress = ref<{ bytesDownloaded: number; totalBytes: number } | null>(null)
const ytdlpCheckingUpdate = ref(false)
let ytdlpProgressCleanup: (() => void) | null = null

async function loadYtdlpStatus(): Promise<void> {
  try {
    ytdlpStatus.value = await window.api.ytdlp.status()
  } catch {
    // ignora
  }
}

async function checkYtdlpUpdate(): Promise<void> {
  ytdlpCheckingUpdate.value = true
  try {
    ytdlpStatus.value = await window.api.ytdlp.checkUpdate()
  } finally {
    ytdlpCheckingUpdate.value = false
    ytdlpProgress.value = null
  }
}

let saveFeedbackTimer: ReturnType<typeof setTimeout> | null = null
const saveFeedback = ref('')
const saveFeedbackError = ref(false)
const remoteInfo = ref<Awaited<ReturnType<typeof window.api.remoteAccess.info>> | null>(null)
const showRemoteQr = ref(false)
const archivePasswords = ref<ArchivePassword[]>([])
const archivePasswordImport = ref('')
const cacheInfo = ref<Awaited<ReturnType<typeof window.api.cache.stats>> | null>(null)
const clearingCache = ref(false)

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
    setLocale(saved.locale)
    if (themeOptions.some((option) => option.id === saved.theme)) {
      setTheme(saved.theme as ThemeId)
    } else {
      setTheme('light')
    }
    applyUiPreferences(saved)
    await refreshRemoteInfo()
    await loadArchivePasswords()
    await refreshCacheInfo()
  }
  window.addEventListener(
    'gdownloader-settings-updated',
    onExternalSettingsUpdated as Parameters<typeof window.addEventListener>[1]
  )
  await loadYtdlpStatus()
  ytdlpProgressCleanup = window.api.ytdlp.onProgress((e) => {
    ytdlpProgress.value = e
    ytdlpStatus.value = { ...ytdlpStatus.value, state: 'downloading' }
  })
})

onUnmounted(() => {
  window.removeEventListener(
    'gdownloader-settings-updated',
    onExternalSettingsUpdated as Parameters<typeof window.removeEventListener>[1]
  )
  ytdlpProgressCleanup?.()
})

function onExternalSettingsUpdated(event: CustomEvent<AppSettingsSnapshot>): void {
  if (!event.detail) return
  Object.assign(settings, event.detail)
}

async function save(): Promise<void> {
  try {
    const persisted = await window.api.settings.save(settingsSnapshot())
    Object.assign(settings, persisted)
    window.dispatchEvent(new CustomEvent('gdownloader-settings-updated', { detail: persisted }))
    await refreshRemoteInfo()
    setSaveFeedback(t('settingsSaved'))
  } catch (error) {
    setSaveFeedback(
      error instanceof Error ? error.message : String(error),
      true,
    )
  }
}

function settingsSnapshot(): AppSettingsSnapshot {
  return JSON.parse(JSON.stringify(settings)) as AppSettingsSnapshot
}

function speedLimitLabel(value: number | undefined): string {
  return value && value > 0 ? `${value} KiB/s` : 'Sem limite'
}

function fmtCacheBytes(bytes: number): string {
  if (bytes >= 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`
  if (bytes >= 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(0)} KB`
  return `${bytes} B`
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

function onAppearanceChange(): void {
  applyUiPreferences(settings)
  void save()
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

async function revokeRemoteSession(id: string): Promise<void> {
  await window.api.remoteAccess.revokeSession(id).catch(() => false)
  await refreshRemoteInfo()
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

async function refreshCacheInfo(): Promise<void> {
  cacheInfo.value = await window.api.cache.stats().catch(() => null)
}

async function clearCache(ids?: string[]): Promise<void> {
  const selected = ids ?? cacheInfo.value?.items.filter((item) => item.clearable && item.bytes > 0).map((item) => item.id) ?? []
  if (selected.length === 0 || clearingCache.value) return
  clearingCache.value = true
  try {
    cacheInfo.value = await window.api.cache.clear(selected)
    setSaveFeedback('Cache limpo')
  } finally {
    clearingCache.value = false
  }
}
</script>

<style scoped>
.settings-panel {
  padding: 24px 28px;
  display: grid;
  grid-template-columns: repeat(2, minmax(360px, 1fr));
  align-content: start;
  gap: 18px;
  max-width: none;
  width: 100%;
  min-height: 0;
  overflow-y: auto;
}

.settings-header {
  grid-column: 1 / -1;
}

.settings-section {
  min-width: 0;
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

.remote-security-alert {
  padding: 10px 12px;
  border: 1px solid rgba(245, 158, 11, 0.38);
  border-radius: 8px;
  background: rgba(245, 158, 11, 0.1);
  color: #b45309;
  font-size: 12px;
  font-weight: 700;
}

.remote-sessions {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.remote-sessions-header,
.remote-session-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
}

.remote-session-row {
  padding: 10px;
  border: 1px solid var(--border-color);
  border-radius: 8px;
  background: var(--bg-card);
}

.remote-session-row div {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 3px;
}

.remote-session-row span,
.remote-session-row em,
.remote-session-empty {
  color: var(--text-muted);
  font-size: 12px;
  overflow-wrap: anywhere;
}

.remote-session-row em {
  font-style: normal;
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

.cache-summary {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 12px;
  border: 1px solid var(--border-color);
  border-radius: 8px;
  background: var(--bg-card);
}

.cache-summary div:first-child {
  display: flex;
  flex-direction: column;
  gap: 3px;
}

.cache-summary strong {
  color: var(--text-primary);
  font-size: 18px;
}

.cache-summary span,
.cache-row span,
.cache-row em,
.cache-empty {
  color: var(--text-muted);
  font-size: 12px;
}

.cache-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.cache-empty,
.cache-row {
  padding: 10px;
  border: 1px solid var(--border-color);
  border-radius: 8px;
  background: var(--bg-card);
}

.cache-row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto auto;
  align-items: center;
  gap: 10px;
}

.cache-row div {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 3px;
}

.cache-row strong {
  color: var(--text-primary);
  font-size: 13px;
}

.cache-row em {
  font-style: normal;
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
  padding: 14px;
  border: 1px solid var(--border-color);
  border-radius: 8px;
  background: var(--bg-card);
}

.section-title {
  margin: 0 0 10px;
  font-size: 11px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.8px;
  color: var(--text-secondary);
  padding-bottom: 8px;
  border-bottom: 1px solid var(--border-color);
}

/* Setting row */
.setting-row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(160px, auto);
  align-items: center;
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
  min-width: 0;
}

.setting-label {
  font-size: 13px;
  font-weight: 650;
  color: var(--text-primary);
}

.setting-desc {
  font-size: 11px;
  color: var(--text-secondary);
}

/* Inputs */
.setting-input,
.setting-select {
  background: var(--bg-secondary);
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
  min-width: 0;
}

.browse-btn {
  border: 1px solid var(--border-color);
  background: var(--bg-secondary);
  color: var(--text-primary);
  border-radius: 8px;
  padding: 8px 12px;
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
}

.btn-secondary {
  border: 1px solid var(--border-color);
  background: var(--bg-secondary);
  color: var(--text-primary);
  border-radius: 8px;
  padding: 8px 14px;
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
  white-space: nowrap;
  transition: border-color 0.15s, background 0.15s, opacity 0.15s;
}

.btn-secondary:hover:not(:disabled) {
  border-color: var(--accent-color);
}

.btn-secondary:disabled {
  opacity: 0.55;
  cursor: default;
}

.setting-input:focus,
.setting-select:focus {
  border-color: var(--accent-color);
}

@media (max-width: 980px) {
  .settings-panel {
    grid-template-columns: 1fr;
    padding: 18px;
  }

  .setting-row {
    grid-template-columns: 1fr;
    gap: 8px;
  }
}

.setting-input-wide {
  width: min(420px, 42vw);
  font-family: 'JetBrains Mono', 'Courier New', monospace;
}

.setting-select {
  cursor: pointer;
  min-width: 140px;
}

.speed-limit-control {
  display: grid;
  grid-template-columns: minmax(220px, 1fr) 110px auto;
  align-items: center;
  gap: 10px;
  width: min(520px, 52vw);
}

.setting-range {
  --range-fill: var(--accent-color);
  appearance: none;
  width: 100%;
  height: 8px;
  border-radius: 999px;
  background: linear-gradient(90deg, var(--range-fill), rgba(148, 163, 184, 0.22));
  outline: none;
}

.setting-range::-webkit-slider-thumb {
  appearance: none;
  width: 20px;
  height: 20px;
  border: 3px solid var(--bg-card);
  border-radius: 50%;
  background: var(--accent-color);
  box-shadow: 0 3px 10px rgba(15, 23, 42, 0.22);
  cursor: pointer;
}

.setting-range::-moz-range-thumb {
  width: 18px;
  height: 18px;
  border: 3px solid var(--bg-card);
  border-radius: 50%;
  background: var(--accent-color);
  box-shadow: 0 3px 10px rgba(15, 23, 42, 0.22);
  cursor: pointer;
}

.speed-limit-input {
  width: 110px;
}

.speed-limit-value {
  min-width: 84px;
  color: var(--text-secondary);
  font-size: 12px;
  font-weight: 700;
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
