<template>
  <div class="settings-panel">
    <div class="settings-header">
      <h2 class="settings-title">{{ t('settingsTitle') }}</h2>
      <p class="settings-sub">{{ t('settingsSub') }}</p>
    </div>

    <!-- Download section -->
    <div class="settings-section">
      <h3 class="section-title">{{ t('downloadsSection') }}</h3>

      <div class="setting-row">
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
          <button class="browse-btn" @click="chooseDirectory">Escolher</button>
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
          <option value="dark-purple">{{ t('themeDarkPurple') }}</option>
          <option value="dark-monokai">{{ t('themeDarkMonokai') }}</option>
          <option value="dark-default">{{ t('themeDarkDefault') }}</option>
          <option value="light">{{ t('themeLight') }}</option>
        </select>
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

  </div>
</template>

<script setup lang="ts">
import { onMounted, reactive } from 'vue'
import { useTheme, type ThemeId } from '../themes'
import { setLocale, useI18n } from '../i18n'

const { setTheme } = useTheme()
const { t } = useI18n()

interface AppSettings {
  outputDir: string
  maxConcurrentDownloads: number
  maxRetriesPerDownload: number
  parallelPartsPerDownload: number
  speedLimitKib: number
  theme: string
  nativeNotification: boolean
  locale: string
  fontSize: number
  fontFamily: string
  uiZoom: number
}

const settings = reactive<AppSettings>({
  outputDir: '~/Downloads',
  maxConcurrentDownloads: 3,
  maxRetriesPerDownload: 3,
  parallelPartsPerDownload: 4,
  speedLimitKib: 0,
  theme: 'dark-purple',
  nativeNotification: true,
  locale: 'pt-BR',
  fontSize: 14,
  fontFamily: 'Inter',
  uiZoom: 1,
})

onMounted(async () => {
  const saved = await window.api.settings.load().catch(() => null)
  if (saved) {
    Object.assign(settings, saved)
    setLocale(saved.locale)
  }
})

async function save(): Promise<void> {
  await window.api.settings.save({ ...settings }).catch(() => null)
}

async function chooseDirectory(): Promise<void> {
  const chosen = await window.api.settings.chooseDirectory().catch(() => '')
  if (!chosen) return
  settings.outputDir = chosen
  await save()
}

function onThemeChange(): void {
  setTheme(settings.theme as ThemeId)
  void save()
}

function onLocaleChange(): void {
  setLocale(settings.locale)
  void save()
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
</style>
