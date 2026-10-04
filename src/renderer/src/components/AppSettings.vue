<template>
  <div class="settings-shell">
    <aside class="settings-nav">
      <div class="settings-nav-header">
        <h2>{{ t('settingsTitle') }}</h2>
        <p>{{ t('settingsSub') }}</p>
      </div>
      <button
        v-for="tab in tabs"
        :key="tab.id"
        type="button"
        class="settings-nav-item"
        :class="{ active: activeTab === tab.id }"
        @click="activeTab = tab.id"
      >
        <i class="pi" :class="tab.icon"></i>
        <span>{{ tab.label }}</span>
      </button>
      <transition name="fade">
        <p v-if="saveFeedback" class="settings-feedback" :class="{ error: saveFeedbackError }">
          <i class="pi" :class="saveFeedbackError ? 'pi-exclamation-triangle' : 'pi-check-circle'"></i>
          {{ saveFeedback }}
        </p>
      </transition>
    </aside>

    <div class="settings-content">
      <!-- ── Geral ─────────────────────────────────────────── -->
      <section v-show="activeTab === 'general'" class="settings-page">
        <header class="page-header">
          <h3>Geral</h3>
          <p>Pasta de destino, fila e comportamento padrão de download</p>
        </header>

        <div class="settings-card" data-tour="download-folder">
          <div class="card-title">Pasta e fila</div>

          <div class="setting-row">
            <div class="setting-info">
              <span class="setting-label">{{ t('outputFolder') }}</span>
              <span class="setting-desc">{{ t('outputFolderDesc') }}</span>
            </div>
            <div class="output-folder-actions">
              <input v-model="settings.outputDir" class="setting-input setting-input-wide" placeholder="~/Downloads" @change="save" />
              <button class="browse-btn" @click="chooseDirectory">{{ t('choose') }}</button>
            </div>
          </div>

          <div class="setting-row" data-tour="concurrency">
            <div class="setting-info">
              <span class="setting-label">{{ t('concurrentDownloads') }}</span>
              <span class="setting-desc">{{ t('concurrentDownloadsDesc') }}</span>
            </div>
            <select v-model="settings.maxConcurrentDownloads" class="setting-select" @change="save">
              <option v-for="n in [1, 2, 3, 4, 5, 8, 10]" :key="n" :value="n">{{ n }}</option>
            </select>
          </div>

          <div class="setting-row">
            <div class="setting-info">
              <span class="setting-label">{{ t('parallelParts') }}</span>
              <span class="setting-desc">{{ t('parallelPartsDesc') }} YouTube sempre usa 1 parte.</span>
            </div>
            <select v-model="settings.parallelPartsPerDownload" class="setting-select" @change="save">
              <option v-for="n in [1, 2, 4, 6, 8]" :key="n" :value="n">{{ n }}</option>
            </select>
          </div>

          <div class="setting-row">
            <div class="setting-info">
              <span class="setting-label">{{ t('speedLimitSetting') }}</span>
              <span class="setting-desc">{{ t('speedLimitDesc') }}</span>
            </div>
            <div class="speed-limit-control">
              <input v-model.number="settings.speedLimitKib" type="range" min="0" max="51200" step="100" class="setting-range" @change="save" />
              <input v-model.number="settings.speedLimitKib" type="number" min="0" step="100" class="setting-input speed-limit-input" @change="save" />
              <span class="speed-limit-value">{{ speedLimitLabel(settings.speedLimitKib) }}</span>
            </div>
          </div>
        </div>

        <div class="settings-card">
          <div class="card-title">Repetição e duplicatas</div>

          <div class="setting-row">
            <div class="setting-info">
              <span class="setting-label">{{ t('maxRetries') }}</span>
              <span class="setting-desc">{{ t('maxRetriesDesc') }}</span>
            </div>
            <div class="retries-control">
              <select v-model="settings.maxRetriesPerDownload" class="setting-select" :disabled="settings.infiniteRetries" @change="save">
                <option v-for="n in [1, 2, 3, 5, 8, 10]" :key="n" :value="n">{{ n }}</option>
              </select>
              <label class="retries-infinite">
                <input type="checkbox" v-model="settings.infiniteRetries" @change="save" />
                <span>{{ t('infiniteRetries') }}</span>
              </label>
            </div>
          </div>

          <div class="setting-row" data-tour="duplicates">
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
        </div>

        <div class="settings-card">
          <div class="card-title">Idioma e notificações</div>

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

          <div class="setting-row">
            <div class="setting-info">
              <span class="setting-label">{{ t('notifyOnComplete') }}</span>
              <span class="setting-desc">{{ t('notifyOnCompleteDesc') }}</span>
            </div>
            <label class="toggle">
              <input type="checkbox" v-model="settings.nativeNotification" @change="save" />
              <span class="toggle-track"><span class="toggle-thumb"></span></span>
            </label>
          </div>
        </div>

        <div class="settings-card">
          <div class="card-title">Bloco do download na fila</div>
          <div class="setting-row setting-row-stack">
            <div class="setting-info">
              <span class="setting-label">Densidade e colunas visíveis</span>
              <span class="setting-desc">Controla o tamanho de cada linha e quais dados ela mostra</span>
            </div>
            <div class="display-preferences">
              <label class="display-size-field">
                <span>Tamanho do bloco</span>
                <select v-model="settings.uiDensity" class="setting-select" @change="save">
                  <option value="comfortable">Confortável</option>
                  <option value="compact">Compacto</option>
                  <option value="dense">Denso</option>
                </select>
              </label>
              <div class="display-field-grid">
                <label v-for="field in downloadBlockFields" :key="field.id" class="setting-check">
                  <input type="checkbox" :checked="isDownloadBlockFieldVisible(field.id)" @change="toggleDownloadBlockField(field.id)" />
                  <span>{{ field.label }}</span>
                </label>
              </div>
            </div>
          </div>
        </div>
      </section>

      <!-- ── YouTube ───────────────────────────────────────── -->
      <section v-show="activeTab === 'youtube'" class="settings-page" data-tour="youtube-settings">
        <header class="page-header">
          <h3>YouTube</h3>
          <p>Cookies, legendas, formatos e os binários usados pelo yt-dlp/ffmpeg</p>
        </header>

        <div class="settings-card">
          <div class="card-title">Autenticação</div>
          <div class="setting-row">
            <div class="setting-info">
              <span class="setting-label">Usar cookies</span>
              <span class="setting-desc">Tenta ler cookies do navegador. Para bloqueio anti-bot, prefira um arquivo Netscape exportado de uma sessão privada recém-autenticada.</span>
            </div>
            <label class="toggle">
              <input type="checkbox" v-model="settings.youtubeUseCookies" @change="save" />
              <span class="toggle-track"><span class="toggle-thumb"></span></span>
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
              <span class="setting-desc">Recomendado: arquivo Netscape estável. Tem prioridade sobre cookies do navegador.</span>
            </div>
            <input v-model="settings.youtubeCookiesFile" class="setting-input setting-input-wide" placeholder="/caminho/cookies.txt" @change="save" />
          </div>
        </div>

        <div class="settings-card">
          <div class="card-title">Formato e legendas</div>
          <div class="setting-row">
            <div class="setting-info">
              <span class="setting-label">Container de merge</span>
              <span class="setting-desc">Formato final quando yt-dlp junta vídeo e áudio com ffmpeg</span>
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
              <span>{{ settings.youtubeDownloadSubs ? 'Baixar' : 'Não baixar' }}</span>
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
              <span class="setting-label">Separar por capítulos</span>
              <span class="setting-desc">Quando disponível, gera arquivos separados por capítulo</span>
            </div>
            <label class="setting-toggle">
              <input type="checkbox" v-model="settings.youtubeSplitChapters" @change="save" />
              <span>{{ settings.youtubeSplitChapters ? 'Ativado' : 'Desativado' }}</span>
            </label>
          </div>
          <div class="setting-row">
            <div class="setting-info">
              <span class="setting-label">Pack do YouTube</span>
              <span class="setting-desc">Cria uma pasta com vídeo, thumbnail, legendas, descrição e metadados</span>
            </div>
            <label class="setting-toggle">
              <input type="checkbox" v-model="settings.youtubeDownloadPack" @change="save" />
              <span>{{ settings.youtubeDownloadPack ? 'Ativado' : 'Desativado' }}</span>
            </label>
          </div>
        </div>

        <div class="settings-card">
          <div class="card-title">Binários (yt-dlp / ffmpeg)</div>
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
              <span class="setting-desc setting-desc-error" v-else-if="ytdlpStatus.state === 'error'">Erro: {{ ytdlpStatus.error ?? 'falha ao obter yt-dlp' }}</span>
            </div>
            <button class="btn-secondary" :disabled="ytdlpCheckingUpdate || ytdlpStatus.state === 'downloading'" @click="checkYtdlpUpdate">
              {{ ytdlpCheckingUpdate ? 'Verificando...' : 'Verificar agora' }}
            </button>
          </div>
          <div class="setting-row">
            <div class="setting-info">
              <span class="setting-label">Atualizar yt-dlp automaticamente</span>
              <span class="setting-desc">Verifica e baixa novas versões ao abrir o app</span>
            </div>
            <label class="toggle">
              <input type="checkbox" v-model="settings.ytdlpAutoUpdate" @change="save" />
              <span class="toggle-track"><span class="toggle-thumb"></span></span>
            </label>
          </div>
          <div class="setting-row">
            <div class="setting-info">
              <span class="setting-label">Caminho do yt-dlp (avançado)</span>
              <span class="setting-desc">Deixe em branco para usar o binário gerenciado pelo app</span>
            </div>
            <input v-model="settings.ytdlpBinPath" class="setting-input setting-input-wide" placeholder="(gerenciado automaticamente)" @change="save" />
          </div>
          <div class="setting-row">
            <div class="setting-info">
              <span class="setting-label">ffmpeg</span>
              <span class="setting-desc" v-if="ffmpegStatus.state === 'ready'">
                v{{ ffmpegStatus.version }} ·
                <span v-if="ffmpegStatus.source === 'system'">usando o do sistema</span>
                <span v-else-if="ffmpegStatus.source === 'custom'">caminho personalizado</span>
                <span v-else>gerenciado pelo app</span>
              </span>
              <span class="setting-desc" v-else-if="ffmpegStatus.state === 'downloading'">
                <span v-if="ffmpegProgress">Baixando... {{ Math.round((ffmpegProgress.bytesDownloaded / Math.max(ffmpegProgress.totalBytes, 1)) * 100) }}%</span>
                <span v-else>Baixando ffmpeg...</span>
              </span>
              <span class="setting-desc setting-desc-error" v-else-if="ffmpegStatus.state === 'error'">Erro: {{ ffmpegStatus.error ?? 'falha ao obter ffmpeg' }}</span>
              <span class="setting-desc" v-else>Não encontrado. Necessário para juntar vídeo+áudio do YouTube.</span>
            </div>
            <button class="btn-secondary" :disabled="ffmpegStatus.state === 'downloading' || ffmpegStatus.state === 'ready'" @click="downloadFfmpeg">
              {{ ffmpegStatus.state === 'downloading' ? 'Baixando...' : 'Baixar ffmpeg' }}
            </button>
          </div>
          <div class="setting-row">
            <div class="setting-info">
              <span class="setting-label">Caminho do ffmpeg (avançado)</span>
              <span class="setting-desc">Deixe em branco para detectar o do sistema ou usar o gerenciado</span>
            </div>
            <input v-model="settings.ffmpegBinPath" class="setting-input setting-input-wide" placeholder="(detectado automaticamente)" @change="onFfmpegPathChange" />
          </div>
        </div>
      </section>

      <!-- ── Rede & Privacidade ───────────────────────────────── -->
      <section v-show="activeTab === 'network'" class="settings-page">
        <header class="page-header">
          <h3>Rede &amp; Privacidade</h3>
          <p>Reconexão automática e o servidor de acesso remoto local</p>
        </header>

        <div class="settings-card">
          <div class="card-title">Reconexão</div>
          <div class="setting-row">
            <div class="setting-label-wrap">
              <span class="setting-label">{{ t('reconnectOnRateLimit') }}</span>
              <span class="setting-desc">{{ t('reconnectOnRateLimitDesc') }}</span>
            </div>
            <label class="toggle">
              <input type="checkbox" v-model="settings.useReconnectOnRateLimit" @change="save" />
              <span class="toggle-track"><span class="toggle-thumb"></span></span>
            </label>
          </div>
        </div>

        <div class="settings-card" data-tour="remote-access">
          <div class="card-title">Acesso remoto local</div>
          <div class="setting-row">
            <div class="setting-info">
              <span class="setting-label">Web UI local</span>
              <span class="setting-desc">Sobe a interface web do gDownloader (por padrão só em 127.0.0.1)</span>
            </div>
            <label class="toggle">
              <input v-model="settings.remoteAccess.enabled" type="checkbox" @change="save" />
              <span class="toggle-track"><span class="toggle-thumb"></span></span>
            </label>
          </div>
          <div class="setting-row">
            <div class="setting-info">
              <span class="setting-label">Expor na LAN</span>
              <span class="setting-desc">Quando ligado, escuta em 0.0.0.0 e outro dispositivo da rede consegue acessar (HTTP sem TLS)</span>
            </div>
            <label class="toggle">
              <input v-model="settings.remoteAccess.allowLan" type="checkbox" @change="save" />
              <span class="toggle-track"><span class="toggle-thumb"></span></span>
            </label>
          </div>
          <div class="setting-row">
            <div class="setting-info">
              <span class="setting-label">Usuário</span>
              <span class="setting-desc">Login usado no celular ou em outro computador da rede</span>
            </div>
            <input v-model="settings.remoteAccess.username" class="setting-input setting-input-wide" @change="save" />
          </div>
          <div class="setting-row">
            <div class="setting-info">
              <span class="setting-label">Senha</span>
              <span class="setting-desc">Use “Gerar” para criar senha forte (mín. 16 caracteres). Sem isso o servidor não sobe.</span>
            </div>
            <div class="remote-inline">
              <input v-model="settings.remoteAccess.password" class="setting-input" type="password" autocomplete="new-password" @change="save" />
              <button class="browse-btn" @click="generateRemoteCredentials">Gerar</button>
            </div>
          </div>
          <div class="setting-row">
            <div class="setting-info">
              <span class="setting-label">Porta local</span>
              <span class="setting-desc">Porta HTTP local (sem TLS). Prefira não expor na internet.</span>
            </div>
            <input v-model.number="settings.remoteAccess.port" class="setting-input" type="number" min="1024" max="65535" @change="save" />
          </div>

          <div class="remote-access-card">
            <div class="remote-access-info">
              <span class="remote-status" :class="{ active: remoteInfo?.running, error: remoteInfo?.error }">
                {{ remoteInfo?.running ? (settings.remoteAccess.allowLan ? 'Online na rede local (LAN)' : 'Online só neste computador') : settings.remoteAccess.enabled ? 'Ativando...' : 'Desativado' }}
              </span>
              <a v-if="remoteInfo?.url" class="remote-url" :href="remoteInfo.url" target="_blank" rel="noreferrer">{{ remoteInfo.url }}</a>
              <span v-if="remoteInfo?.error" class="remote-error">{{ remoteInfo.error }}</span>
              <div class="remote-actions">
                <button class="browse-btn" :disabled="!remoteInfo?.credentialUrl" @click="copyRemoteUrl">Copiar link de login</button>
                <button class="browse-btn" :disabled="!remoteInfo?.qrCodeDataUrl" @click="showRemoteQr = !showRemoteQr">
                  {{ showRemoteQr ? 'Ocultar QR Code' : 'Gerar QR Code' }}
                </button>
                <button class="browse-btn" @click="refreshRemoteInfo">Atualizar</button>
              </div>
            </div>
            <img v-if="showRemoteQr && remoteInfo?.qrCodeDataUrl" class="remote-qr" :src="remoteInfo.qrCodeDataUrl" alt="QR Code do acesso remoto local" />
          </div>
          <div v-if="remoteInfo?.insecureCredentials" class="remote-security-alert">
            Senha fraca ou ausente. Clique em “Gerar” para criar uma senha forte — o servidor remoto não sobe sem isso.
          </div>
          <div v-if="settings.remoteAccess.allowLan && settings.remoteAccess.enabled" class="remote-security-alert">
            Acesso na LAN ativo em HTTP sem TLS. Use só em rede confiável e com senha forte.
          </div>
          <div class="remote-sessions">
            <div class="remote-sessions-header">
              <strong>Sessões conectadas</strong>
              <button class="browse-btn" @click="refreshRemoteInfo">Atualizar</button>
            </div>
            <div v-if="!(remoteInfo?.sessions?.length)" class="remote-session-empty">Nenhum dispositivo autenticado por token.</div>
            <div v-for="session in remoteInfo?.sessions ?? []" :key="session.id" class="remote-session-row">
              <div>
                <strong>{{ session.ip }} <span v-if="session.current">(atual)</span></strong>
                <span>{{ session.userAgent }}</span>
                <em>{{ new Date(session.lastSeenAt).toLocaleString() }}</em>
              </div>
              <button class="browse-btn" :disabled="session.current" @click="revokeRemoteSession(session.id)">Derrubar</button>
            </div>
          </div>
        </div>
      </section>

      <!-- ── Captcha ─────────────────────────────────────────── -->
      <section v-show="activeTab === 'captcha'" class="settings-page">
        <header class="page-header">
          <h3>Solvers de captcha</h3>
          <p>EzSolver, Icemellow, Surafel e FlareSolverr — instalação e atualização</p>
        </header>

        <div class="settings-card">
          <div class="setting-row">
            <div class="setting-info">
              <span class="setting-label">Atualizar automaticamente</span>
              <span class="setting-desc">Verifica e baixa novas versões dos solvers ao abrir o app</span>
            </div>
            <label class="toggle">
              <input type="checkbox" v-model="settings.turnstileAutoUpdate" @change="save" />
              <span class="toggle-track"><span class="toggle-thumb"></span></span>
            </label>
          </div>
          <div class="setting-row">
            <div class="setting-info">
              <span class="setting-label">Usar solvers de captcha</span>
              <span class="setting-desc">Desative para nunca acionar os solvers (o download fica pendente ao encontrar um captcha)</span>
            </div>
            <label class="toggle">
              <input type="checkbox" v-model="settings.turnstileEnabled" @change="save" />
              <span class="toggle-track"><span class="toggle-thumb"></span></span>
            </label>
          </div>
        </div>

        <div class="settings-card">
          <div class="card-title">Solvers instalados</div>
          <div class="solver-grid">
            <div v-for="solver in solverStatuses" :key="solver.id" class="solver-card">
              <div class="solver-card-head">
                <strong>{{ solver.name }}</strong>
                <span class="solver-dot" :class="solver.state"></span>
              </div>
              <span class="setting-desc" v-if="solver.state === 'ready'">
                v{{ solver.version ?? '?' }}<span v-if="solver.updateAvailable"> · atualização em {{ solver.latestVersion }}</span>
              </span>
              <span class="setting-desc" v-else-if="solver.state === 'downloading' || solver.state === 'updating'">
                <span v-if="solverProgress[solver.id]">Baixando... {{ Math.round((solverProgress[solver.id].bytesDownloaded / Math.max(solverProgress[solver.id].totalBytes, 1)) * 100) }}%</span>
                <span v-else>Instalando...</span>
              </span>
              <span class="setting-desc setting-desc-error" v-else-if="solver.state === 'error'">Erro: {{ solver.error ?? 'falha ao obter o solver' }}</span>
              <span class="setting-desc" v-else>Não instalado</span>
              <button
                class="btn-secondary"
                :disabled="!!solverUpdating[solver.id] || solver.state === 'downloading' || solver.state === 'updating'"
                @click="solver.state === 'absent' || solver.state === 'error' ? updateSolver(solver.id) : checkSolverUpdate(solver.id)"
              >
                {{ solverUpdating[solver.id] ? 'Verificando...' : (solver.state === 'absent' || solver.state === 'error') ? 'Instalar' : 'Verificar agora' }}
              </button>
            </div>
          </div>
        </div>
      </section>

      <!-- ── Automação ───────────────────────────────────────── -->
      <section v-show="activeTab === 'automation'" class="settings-page">
        <header class="page-header">
          <h3>{{ t('automationSection') }}</h3>
          <p>O que fazer automaticamente quando a fila terminar</p>
        </header>

        <div class="settings-card">
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
            <div class="setting-info"><span class="setting-label">{{ t('postDownloadTrigger') }}</span></div>
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
            <input v-model="settings.postDownloadCommand" class="setting-input setting-input-wide" placeholder="notify-send 'Done'" @change="save" />
          </div>
          <div v-if="settings.postDownloadAction === 'webhook'" class="setting-row">
            <div class="setting-info">
              <span class="setting-label">{{ t('postDownloadWebhookUrl') }}</span>
              <span class="setting-desc">{{ t('postDownloadWebhookUrlDesc') }}</span>
            </div>
            <input v-model="settings.postDownloadWebhookUrl" class="setting-input setting-input-wide" placeholder="https://hooks.example.com/..." @change="save" />
          </div>
        </div>
      </section>

      <!-- ── Aparência ───────────────────────────────────────── -->
      <section v-show="activeTab === 'appearance'" class="settings-page">
        <header class="page-header">
          <h3>{{ t('appearance') }}</h3>
          <p>Tema, tipografia e zoom da interface</p>
        </header>

        <div class="settings-card">
          <div class="theme-swatches">
            <button
              v-for="option in themeOptions"
              :key="option.id"
              type="button"
              class="theme-swatch"
              :class="[`swatch-${option.id}`, { active: settings.theme === option.id }]"
              @click="settings.theme = option.id; onThemeChange()"
            >
              <span class="swatch-preview"><i class="pi" :class="option.icon"></i></span>
              <span>{{ option.label }}</span>
            </button>
          </div>
        </div>

        <div class="settings-card">
          <div class="card-title">Tipografia</div>
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
              <option v-for="size in [12, 13, 14, 15, 16, 18]" :key="size" :value="size">{{ size }} px</option>
            </select>
          </div>
          <div class="setting-row">
            <div class="setting-info">
              <span class="setting-label">{{ t('uiZoomLabel') }}</span>
              <span class="setting-desc">{{ t('uiZoomDesc') }}</span>
            </div>
            <select v-model.number="settings.uiZoom" class="setting-select" @change="onAppearanceChange">
              <option v-for="zoom in [0.9, 1, 1.1, 1.2, 1.3]" :key="zoom" :value="zoom">{{ zoom.toFixed(1) }}x</option>
            </select>
          </div>
        </div>
      </section>

      <!-- ── Armazenamento ───────────────────────────────────── -->
      <section v-show="activeTab === 'storage'" class="settings-page">
        <header class="page-header">
          <h3>Armazenamento</h3>
          <p>Cache local do app e senhas de arquivos compactados aprendidas</p>
        </header>

        <div class="settings-card">
          <div class="card-title">Cache local</div>
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
            <div v-if="!(cacheInfo?.items?.length)" class="cache-empty">Nenhum cache local identificado.</div>
            <div v-for="item in cacheInfo?.items ?? []" :key="item.id" class="cache-row">
              <div>
                <strong>{{ item.label }}</strong>
                <span>{{ item.description }}</span>
                <em v-if="item.entries !== undefined">{{ item.entries }} entrada(s)</em>
              </div>
              <span>{{ fmtCacheBytes(item.bytes) }}</span>
              <button class="browse-btn" :disabled="clearingCache || !item.clearable || item.bytes <= 0" @click="clearCache([item.id])">Limpar</button>
            </div>
          </div>
        </div>

        <div class="settings-card">
          <div class="card-title">Senhas de archives</div>
          <div class="setting-row">
            <div class="setting-info">
              <span class="setting-label">Extrair automaticamente</span>
              <span class="setting-desc">Ao concluir, extrai .zip/.rar/.7z — respeita grupos multi-parte, só extrai quando todas as partes chegarem</span>
            </div>
            <label class="toggle">
              <input type="checkbox" v-model="settings.autoExtract" @change="save" />
              <span class="toggle-track"><span class="toggle-thumb"></span></span>
            </label>
          </div>
          <div class="archive-password-list">
            <div v-if="archivePasswords.length === 0" class="archive-password-empty">Nenhuma senha aprendida ainda.</div>
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
            <textarea v-model="archivePasswordImport" class="setting-textarea" placeholder="senha1&#10;senha2"></textarea>
            <div class="remote-actions">
              <button class="browse-btn" @click="importArchivePasswords">Importar lista</button>
              <button class="browse-btn" @click="exportArchivePasswords">Exportar</button>
            </div>
          </div>
        </div>
      </section>
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

type SettingsTab = 'general' | 'youtube' | 'network' | 'captcha' | 'automation' | 'appearance' | 'storage'
const activeTab = ref<SettingsTab>('general')
const tabs: Array<{ id: SettingsTab; label: string; icon: string }> = [
  { id: 'general', label: 'Geral', icon: 'pi-sliders-h' },
  { id: 'youtube', label: 'YouTube', icon: 'pi-youtube' },
  { id: 'network', label: 'Rede & Privacidade', icon: 'pi-shield' },
  { id: 'captcha', label: 'Captcha', icon: 'pi-key' },
  { id: 'automation', label: 'Automação', icon: 'pi-bolt' },
  { id: 'appearance', label: 'Aparência', icon: 'pi-palette' },
  { id: 'storage', label: 'Armazenamento', icon: 'pi-database' },
]

const defaultDownloadBlockFields = ['status', 'name', 'size', 'progress', 'speed', 'eta', 'host', 'package', 'added', 'completed', 'hash']
const downloadBlockFields = [
  { id: 'name', label: 'Nome' },
  { id: 'status', label: 'Status' },
  { id: 'size', label: 'Tamanho' },
  { id: 'progress', label: 'Progresso' },
  { id: 'speed', label: 'Velocidade' },
  { id: 'eta', label: 'ETA' },
  { id: 'host', label: 'Host / ícone' },
  { id: 'package', label: 'Pacote' },
  { id: 'added', label: 'Adicionado' },
  { id: 'completed', label: 'Concluído' },
  { id: 'hash', label: 'Hash' },
]

const settings = reactive<AppSettingsSnapshot>({
  outputDir: '~/Downloads',
  maxConcurrentDownloads: 3,
  maxRetriesPerDownload: 3,
  infiniteRetries: false,
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
  visibleColumns: [...defaultDownloadBlockFields],
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
  youtubeDownloadPack: false,
  ytdlpAutoUpdate: true,
  ytdlpBinPath: '',
  ffmpegBinPath: '',
  remoteAccess: {
    enabled: false,
    allowLan: false,
    username: 'gdownloader',
    password: '',
    port: 9786,
  },
})

function isDownloadBlockFieldVisible(field: string): boolean {
  return (settings.visibleColumns ?? defaultDownloadBlockFields).includes(field)
}

function toggleDownloadBlockField(field: string): void {
  const visible = settings.visibleColumns ?? [...defaultDownloadBlockFields]
  settings.visibleColumns = visible.includes(field)
    ? visible.filter((item) => item !== field)
    : defaultDownloadBlockFields.filter((item) => item === field || visible.includes(item))
  void save()
}
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

interface FfmpegStatus {
  version: string | null
  state: 'ready' | 'downloading' | 'absent' | 'error'
  source: 'system' | 'custom' | 'managed' | 'none'
  path: string | null
  error?: string
}

const ffmpegStatus = ref<FfmpegStatus>({ version: null, state: 'absent', source: 'none', path: null })
const ffmpegProgress = ref<{ bytesDownloaded: number; totalBytes: number } | null>(null)
let ffmpegProgressCleanup: (() => void) | null = null

async function loadFfmpegStatus(): Promise<void> {
  try {
    ffmpegStatus.value = await window.api.ffmpeg.status()
  } catch {
    // ignora
  }
}

async function downloadFfmpeg(): Promise<void> {
  ffmpegStatus.value = { ...ffmpegStatus.value, state: 'downloading' }
  try {
    ffmpegStatus.value = await window.api.ffmpeg.download()
  } finally {
    ffmpegProgress.value = null
  }
}

async function onFfmpegPathChange(): Promise<void> {
  await save()
  await loadFfmpegStatus()
}

interface SolverStatus {
  id: string
  name: string
  version: string | null
  latestVersion: string | null
  updateAvailable: boolean
  state: 'ready' | 'downloading' | 'error' | 'absent' | 'updating'
  error?: string
}

const solverStatuses = ref<SolverStatus[]>([])
const solverProgress = ref<Record<string, { bytesDownloaded: number; totalBytes: number }>>({})
const solverUpdating = ref<Record<string, boolean>>({})
let solverProgressCleanup: (() => void) | null = null

async function loadSolverStatuses(): Promise<void> {
  try {
    solverStatuses.value = (await window.api.turnstile.statusAll()) as unknown as SolverStatus[]
  } catch {
    // ignora
  }
}

async function updateSolver(id: string): Promise<void> {
  solverUpdating.value = { ...solverUpdating.value, [id]: true }
  try {
    const result = (await window.api.turnstile.update(id)) as unknown as SolverStatus
    solverStatuses.value = solverStatuses.value.map((s) => (s.id === id ? result : s))
  } finally {
    solverUpdating.value = { ...solverUpdating.value, [id]: false }
    solverProgress.value = { ...solverProgress.value, [id]: undefined as never }
  }
}

async function checkSolverUpdate(id: string): Promise<void> {
  solverUpdating.value = { ...solverUpdating.value, [id]: true }
  try {
    const result = (await window.api.turnstile.checkUpdate(id)) as unknown as SolverStatus
    if (result) solverStatuses.value = solverStatuses.value.map((s) => (s.id === id ? result : s))
  } finally {
    solverUpdating.value = { ...solverUpdating.value, [id]: false }
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
  await loadFfmpegStatus()
  ffmpegProgressCleanup = window.api.ffmpeg.onProgress((e) => {
    ffmpegProgress.value = e
    ffmpegStatus.value = { ...ffmpegStatus.value, state: 'downloading' }
  })
  await loadSolverStatuses()
  solverProgressCleanup = window.api.turnstile.onProgress((e) => {
    solverProgress.value = { ...solverProgress.value, [e.solverId]: e }
  })
})

onUnmounted(() => {
  window.removeEventListener(
    'gdownloader-settings-updated',
    onExternalSettingsUpdated as Parameters<typeof window.removeEventListener>[1]
  )
  ytdlpProgressCleanup?.()
  ffmpegProgressCleanup?.()
  solverProgressCleanup?.()
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
    allowLan: settings.remoteAccess.allowLan ?? false,
    port: settings.remoteAccess.port || generated.port,
  }
  await save()
  await refreshRemoteInfo()
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
/* ── Shell: nav lateral + conteúdo ────────────────────────── */
.settings-shell {
  display: grid;
  grid-template-columns: 232px minmax(0, 1fr);
  height: 100%;
  min-height: 0;
}

.settings-nav {
  display: flex;
  flex-direction: column;
  gap: 3px;
  padding: 22px 14px;
  border-right: 1px solid var(--border-color);
  overflow-y: auto;
}

.settings-nav-header {
  padding: 0 10px 16px;
  margin-bottom: 6px;
  border-bottom: 1px solid var(--border-color);
}

.settings-nav-header h2 {
  margin: 0 0 4px;
  font-size: 17px;
  font-weight: 750;
  color: var(--text-primary);
  letter-spacing: -0.3px;
}

.settings-nav-header p {
  margin: 0;
  font-size: 12px;
  color: var(--text-muted);
  line-height: 1.4;
}

.settings-nav-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 9px 10px;
  border: none;
  border-radius: 9px;
  background: transparent;
  color: var(--text-secondary);
  font-size: 13px;
  font-weight: 600;
  text-align: left;
  cursor: pointer;
  transition: background 0.14s ease, color 0.14s ease;
}

.settings-nav-item i {
  width: 16px;
  text-align: center;
  font-size: 14px;
  opacity: 0.85;
}

.settings-nav-item:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}

.settings-nav-item.active {
  background: color-mix(in srgb, var(--accent-color) 14%, transparent);
  color: var(--accent-color);
}

.settings-feedback {
  margin: 14px 10px 0;
  padding: 8px 10px;
  border-radius: 8px;
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  font-weight: 650;
  color: #16a34a;
  background: rgba(34, 197, 94, 0.12);
}

.settings-feedback.error {
  color: #dc2626;
  background: rgba(220, 38, 38, 0.1);
}

.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.18s ease;
}
.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}

/* ── Conteúdo ──────────────────────────────────────────────── */
.settings-content {
  overflow-y: auto;
  padding: 26px 32px 60px;
}

.settings-page {
  max-width: 720px;
  display: flex;
  flex-direction: column;
  gap: 16px;
  animation: settings-page-in 0.16s ease;
}

@keyframes settings-page-in {
  from { opacity: 0; transform: translateY(4px); }
  to { opacity: 1; transform: translateY(0); }
}

.page-header {
  margin-bottom: 2px;
}

.page-header h3 {
  margin: 0 0 4px;
  font-size: 19px;
  font-weight: 750;
  color: var(--text-primary);
  letter-spacing: -0.3px;
}

.page-header p {
  margin: 0;
  font-size: 13px;
  color: var(--text-muted);
}

.settings-card {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 18px 20px;
  border: 1px solid var(--border-color);
  border-radius: 14px;
  background: var(--bg-card);
  box-shadow: var(--shadow-card, none);
}

.card-title {
  font-size: 11px;
  font-weight: 750;
  text-transform: uppercase;
  letter-spacing: 0.7px;
  color: var(--accent-color);
  padding-bottom: 10px;
  margin-bottom: 6px;
  border-bottom: 1px solid var(--border-color);
}

/* ── Setting row ───────────────────────────────────────────── */
.setting-row {
  display: grid;
  grid-template-columns: minmax(180px, 1fr) minmax(0, auto);
  align-items: center;
  gap: 20px;
  padding: 13px 0;
  border-bottom: 1px solid color-mix(in srgb, var(--border-color) 45%, transparent);
}

.setting-row:last-child {
  border-bottom: none;
}

.setting-row-stack {
  grid-template-columns: 1fr;
  align-items: stretch;
}

.setting-info,
.setting-label-wrap {
  display: flex;
  flex-direction: column;
  gap: 3px;
  min-width: 0;
}

.setting-label {
  font-size: 13.5px;
  font-weight: 650;
  color: var(--text-primary);
}

.setting-desc {
  font-size: 11.5px;
  color: var(--text-secondary);
  line-height: 1.45;
}

.setting-desc-error {
  color: var(--status-error, #ef4444);
}

/* ── Inputs ────────────────────────────────────────────────── */
.setting-input,
.retries-control {
  display: flex;
  align-items: center;
  gap: 12px;
}

.setting-input {
  background: var(--bg-primary);
  border: 1px solid var(--border-color);
  border-radius: 9px;
  color: var(--text-primary);
  font-size: 13px;
  padding: 8px 12px;
  outline: none;
  transition: border-color 0.15s;
}

.retries-infinite {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  color: var(--text-secondary);
  white-space: nowrap;
  cursor: pointer;
}

.retries-infinite input {
  cursor: pointer;
  accent-color: var(--accent-color);
}

.setting-select {
  background: var(--bg-primary);
  border: 1px solid var(--border-color);
  border-radius: 9px;
  color: var(--text-primary);
  font-size: 13px;
  padding: 8px 12px;
  outline: none;
  cursor: pointer;
  min-width: 150px;
  transition: border-color 0.15s;
}

.setting-select option {
  background: var(--bg-secondary);
}

.output-folder-actions {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
  flex: 1 1 auto;
}
.output-folder-actions .setting-input {
  flex: 1 1 auto;
  min-width: 0;
}

.browse-btn {
  border: 1px solid var(--border-color);
  background: var(--bg-secondary);
  color: var(--text-primary);
  border-radius: 9px;
  padding: 8px 13px;
  font-size: 12px;
  font-weight: 650;
  cursor: pointer;
  transition: border-color 0.15s, background 0.15s;
}

.browse-btn:hover {
  border-color: var(--accent-color);
}

.btn-secondary {
  border: 1px solid var(--border-color);
  background: var(--bg-secondary);
  color: var(--text-primary);
  border-radius: 9px;
  padding: 8px 15px;
  font-size: 12px;
  font-weight: 650;
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

.setting-input-wide {
  width: 100%;
  max-width: 320px;
  min-width: 0;
  font-family: 'JetBrains Mono', 'Courier New', monospace;
}

.speed-limit-control {
  display: grid;
  grid-template-columns: minmax(200px, 1fr) 100px auto;
  align-items: center;
  gap: 10px;
  width: 100%;
  max-width: 460px;
}

.setting-range {
  --range-fill: var(--accent-color);
  appearance: none;
  width: 100%;
  height: 7px;
  border-radius: 999px;
  background: linear-gradient(90deg, var(--range-fill), rgba(148, 163, 184, 0.22));
  outline: none;
}

.setting-range::-webkit-slider-thumb {
  appearance: none;
  width: 18px;
  height: 18px;
  border: 3px solid var(--bg-card);
  border-radius: 50%;
  background: var(--accent-color);
  box-shadow: 0 3px 10px rgba(15, 23, 42, 0.22);
  cursor: pointer;
}

.setting-range::-moz-range-thumb {
  width: 16px;
  height: 16px;
  border: 3px solid var(--bg-card);
  border-radius: 50%;
  background: var(--accent-color);
  box-shadow: 0 3px 10px rgba(15, 23, 42, 0.22);
  cursor: pointer;
}

.speed-limit-input {
  width: 100px;
}

.speed-limit-value {
  min-width: 78px;
  color: var(--text-secondary);
  font-size: 12px;
  font-weight: 650;
}

.setting-toggle {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  font-size: 12.5px;
  font-weight: 600;
  color: var(--text-secondary);
  cursor: pointer;
}

.setting-toggle input {
  accent-color: var(--accent-color);
  cursor: pointer;
}

/* ── Toggle switch ─────────────────────────────────────────── */
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
  width: 40px;
  height: 23px;
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
  width: 19px;
  height: 19px;
  background: #fff;
  border-radius: 50%;
  transition: transform 0.2s ease;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.3);
}

.toggle input:checked + .toggle-track .toggle-thumb {
  transform: translateX(17px);
}

/* ── Densidade / colunas ───────────────────────────────────── */
.display-preferences {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.display-size-field {
  display: inline-flex;
  align-items: center;
  gap: 10px;
  color: var(--text-secondary);
  font-size: 12px;
}

.display-field-grid {
  display: flex;
  flex-wrap: wrap;
  gap: 8px 16px;
}

.setting-check {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  color: var(--text-secondary);
  font-size: 12px;
  cursor: pointer;
}

.setting-check input {
  accent-color: var(--accent-color);
  cursor: pointer;
}

/* ── Tema (swatches) ───────────────────────────────────────── */
.theme-swatches {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(120px, 1fr));
  gap: 10px;
}

.theme-swatch {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  padding: 14px 8px;
  border: 1.5px solid var(--border-color);
  border-radius: 12px;
  background: var(--bg-primary);
  color: var(--text-secondary);
  font-size: 11.5px;
  font-weight: 650;
  cursor: pointer;
  transition: border-color 0.15s, transform 0.1s ease;
}

.theme-swatch:hover {
  transform: translateY(-1px);
}

.theme-swatch.active {
  border-color: var(--accent-color);
  color: var(--text-primary);
}

.swatch-preview {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 40px;
  height: 40px;
  border-radius: 10px;
  font-size: 16px;
}

.swatch-light .swatch-preview { background: linear-gradient(135deg, #f4f6fb, #d7deea); color: #182132; }
.swatch-dark-purple .swatch-preview { background: linear-gradient(135deg, #171c32, #7c6fff); color: #fff; }
.swatch-dark-monokai .swatch-preview { background: linear-gradient(135deg, #282a22, #a6e22e); color: #181914; }
.swatch-dark-default .swatch-preview { background: linear-gradient(135deg, #242932, #569cd6); color: #fff; }
.swatch-system .swatch-preview { background: linear-gradient(135deg, #d7deea 50%, #171c32 50%); color: var(--text-primary); }

/* ── Solvers ───────────────────────────────────────────────── */
.solver-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
  gap: 12px;
}

.solver-card {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 14px;
  border: 1px solid var(--border-color);
  border-radius: 12px;
  background: var(--bg-primary);
}

.solver-card-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.solver-card-head strong {
  font-size: 13px;
  color: var(--text-primary);
}

.solver-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--text-muted);
}

.solver-dot.ready { background: #16a34a; }
.solver-dot.downloading,
.solver-dot.updating { background: #eab308; }
.solver-dot.error { background: #ef4444; }

.solver-card .btn-secondary {
  margin-top: 4px;
  align-self: flex-start;
}

/* ── Acesso remoto ─────────────────────────────────────────── */
.remote-inline {
  display: flex;
  align-items: center;
  gap: 8px;
}

.remote-access-card {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  gap: 16px;
  margin-top: 8px;
  border: 1px solid var(--border-color);
  border-radius: 10px;
  padding: 14px;
  background: var(--bg-primary);
}

.remote-access-info {
  display: flex;
  min-width: 0;
  flex-direction: column;
  gap: 8px;
}

.remote-status {
  width: fit-content;
  padding: 4px 9px;
  border-radius: 999px;
  background: rgba(148, 163, 184, 0.18);
  color: var(--text-muted);
  font-size: 11.5px;
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

.remote-qr {
  width: 128px;
  height: 128px;
  border-radius: 10px;
  border: 1px solid var(--border-color);
  background: #fff;
  padding: 6px;
}

.remote-security-alert {
  margin-top: 8px;
  padding: 10px 12px;
  border: 1px solid rgba(245, 158, 11, 0.38);
  border-radius: 9px;
  background: rgba(245, 158, 11, 0.1);
  color: #b45309;
  font-size: 12px;
  font-weight: 650;
}

.remote-sessions {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin-top: 10px;
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
  border-radius: 9px;
  background: var(--bg-primary);
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

/* ── Cache ─────────────────────────────────────────────────── */
.cache-summary {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 12px;
  border: 1px solid var(--border-color);
  border-radius: 10px;
  background: var(--bg-primary);
  margin-bottom: 10px;
}

.cache-summary div:first-child {
  display: flex;
  flex-direction: column;
  gap: 3px;
}

.cache-summary strong {
  color: var(--text-primary);
  font-size: 19px;
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
  border-radius: 9px;
  background: var(--bg-primary);
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

/* ── Senhas de archives ────────────────────────────────────── */
.setting-textarea {
  min-height: 92px;
  width: 100%;
  resize: vertical;
  border: 1px solid var(--border-color);
  border-radius: 9px;
  background: var(--bg-primary);
  color: var(--text-primary);
  padding: 10px;
  font: inherit;
  font-size: 12px;
  margin-bottom: 10px;
}

.archive-password-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-bottom: 10px;
}

.archive-password-empty,
.archive-password-row {
  padding: 9px 10px;
  border: 1px solid var(--border-color);
  border-radius: 9px;
  background: var(--bg-primary);
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

@media (max-width: 900px) {
  .settings-shell {
    grid-template-columns: 1fr;
    grid-template-rows: auto 1fr;
  }

  .settings-nav {
    flex-direction: row;
    flex-wrap: wrap;
    border-right: none;
    border-bottom: 1px solid var(--border-color);
    padding: 14px;
  }

  .settings-nav-header {
    width: 100%;
    border-bottom: none;
    padding-bottom: 4px;
  }

  .settings-content {
    padding: 18px;
  }

  .setting-row {
    grid-template-columns: 1fr;
    gap: 8px;
  }
}
</style>
