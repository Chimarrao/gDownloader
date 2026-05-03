import { existsSync, readFileSync } from 'fs'

import type { AppSettingsSnapshot, PersistedSettings } from '../shared/types'
import type { BruploadStoredAccount } from './brupload-service'
import type { TeraboxStoredAccount } from './terabox-service'

export const defaultPublicSettings: PersistedSettings = {
  theme: 'light',
  locale: 'pt-BR',
  outputDir: '~/Downloads',
  maxConcurrentDownloads: 3,
  maxRetriesPerDownload: 3,
  speedLimitKib: 0,
  parallelPartsPerDownload: 4,
  fontSize: 14,
  fontFamily: 'Inter',
  uiZoom: 1,
  nativeNotification: true,
  clipboardMonitorEnabled: false,
  accentColor: undefined,
  duplicateAction: 'ask',
}

export interface LegacyRootSettings extends AppSettingsSnapshot {
  accounts?: {
    terabox?: {
      email: string
      password: string
      cookies?: string[]
      verifiedAt?: string
    }
    brupload?: {
      email: string
      password: string
      cookies?: string[]
      verifiedAt?: string
    }
  }
}

export interface SecureSettingsPayload {
  nopechaApiKey?: string
  teraboxAccount?: TeraboxStoredAccount | null
  bruploadAccount?: BruploadStoredAccount | null
}

export interface PersistedHistoryItem {
  id: string
  url: string
  title: string
  thumbnail: string
  date: string
  formatId: string
  outputPath?: string
  sha256Hash?: string
}

interface LegacyMigrationRecord {
  version: number
  name: string
}

interface CreateAppStorageOptions {
  legacySettingsPaths: string[]
  legacyHistoryPaths: string[]
  fetchBackendConfig: <T>(path: string, init?: RequestInit) => Promise<T>
  postBackend: (path: string, payload?: unknown) => Promise<void>
  deleteBackend: (path: string) => Promise<void>
}

interface AppStorageState {
  nopechaApiKey: string
  teraboxAccount: TeraboxStoredAccount | null
  bruploadAccount: BruploadStoredAccount | null
}

function sanitizeForDisk(input: Partial<LegacyRootSettings> | null | undefined): Partial<PersistedSettings> {
  if (!input) {
    return {}
  }

  const {
    nopechaApiKey: _nopechaApiKey,
    accounts: _accounts,
    ...rest
  } = input

  return rest
}

function normalizeTeraboxAccount(account: TeraboxStoredAccount | null | undefined): TeraboxStoredAccount | null {
  if (!account) {
    return null
  }

  const email = account.email?.trim() ?? ''
  const password = account.password ?? ''
  const cookies = Array.isArray(account.cookies)
    ? account.cookies.map((cookie) => String(cookie).trim()).filter(Boolean)
    : []

  if (!email && !password && cookies.length === 0 && !account.verifiedAt) {
    return null
  }

  return {
    email,
    password,
    cookies,
    verifiedAt: account.verifiedAt,
  }
}

function normalizeBruploadAccount(account: BruploadStoredAccount | null | undefined): BruploadStoredAccount | null {
  if (!account) {
    return null
  }

  const email = account.email?.trim() ?? ''
  const password = account.password ?? ''
  const cookies = Array.isArray(account.cookies)
    ? account.cookies.map((cookie) => String(cookie).trim()).filter(Boolean)
    : []

  if (!email && !password && cookies.length === 0 && !account.verifiedAt) {
    return null
  }

  return {
    email,
    password,
    cookies,
    verifiedAt: account.verifiedAt,
  }
}

function readJsonFile<T>(filePath: string): T | null {
  if (!existsSync(filePath)) {
    return null
  }

  try {
    return JSON.parse(readFileSync(filePath, 'utf8')) as T
  } catch {
    return null
  }
}

function readLegacySettings(paths: string[]): Partial<LegacyRootSettings> | null {
  for (const filePath of paths) {
    const parsed = readJsonFile<Partial<LegacyRootSettings>>(filePath)
    if (parsed) {
      return parsed
    }
  }

  return null
}

function readLegacyHistory(paths: string[]): PersistedHistoryItem[] {
  for (const filePath of paths) {
    const parsed = readJsonFile<PersistedHistoryItem[]>(filePath)
    if (Array.isArray(parsed)) {
      return parsed
    }
  }

  return []
}

function mergeLegacyPublicSettings(
  current: PersistedSettings,
  legacy: Partial<PersistedSettings>,
): PersistedSettings {
  const merged: PersistedSettings = {
    ...defaultPublicSettings,
    ...current,
  }
  const mergedRecord = merged as Record<keyof PersistedSettings, PersistedSettings[keyof PersistedSettings]>

  for (const [rawKey, rawValue] of Object.entries(legacy)) {
    if (typeof rawValue === 'undefined') {
      continue
    }

    const key = rawKey as keyof PersistedSettings
    const currentValue = merged[key]
    const defaultValue = defaultPublicSettings[key]

    if (typeof currentValue === 'undefined' || Object.is(currentValue, defaultValue)) {
      mergedRecord[key] = rawValue as PersistedSettings[keyof PersistedSettings]
    }
  }

  return merged
}

export function createAppStorage(options: CreateAppStorageOptions) {
  const secureState: AppStorageState = {
    nopechaApiKey: '',
    teraboxAccount: null,
    bruploadAccount: null,
  }
  let cachedPublicSettings: PersistedSettings = { ...defaultPublicSettings }

  function applySecureSettingsPayload(payload: SecureSettingsPayload | null | undefined): void {
    secureState.nopechaApiKey = payload?.nopechaApiKey?.trim() ?? ''
    secureState.teraboxAccount = normalizeTeraboxAccount(payload?.teraboxAccount)
    secureState.bruploadAccount = normalizeBruploadAccount(payload?.bruploadAccount)
  }

  function currentSecureSettingsPayload(): SecureSettingsPayload {
    return {
      nopechaApiKey: secureState.nopechaApiKey || undefined,
      teraboxAccount: secureState.teraboxAccount,
      bruploadAccount: secureState.bruploadAccount,
    }
  }

  function currentSettingsSnapshot(): AppSettingsSnapshot {
    return {
      ...defaultPublicSettings,
      ...cachedPublicSettings,
      nopechaApiKey: secureState.nopechaApiKey || undefined,
    }
  }

  async function loadPublicSettings(): Promise<void> {
    const payload = await options.fetchBackendConfig<Partial<PersistedSettings>>('/config/public')
    cachedPublicSettings = {
      ...defaultPublicSettings,
      ...payload,
    }
  }

  async function loadSecureSettings(): Promise<void> {
    const payload = await options.fetchBackendConfig<SecureSettingsPayload>('/config/secure')
    applySecureSettingsPayload(payload)
  }

  async function persistPublicSettings(settings: PersistedSettings): Promise<void> {
    cachedPublicSettings = {
      ...defaultPublicSettings,
      ...settings,
    }
    await options.postBackend('/config/public', cachedPublicSettings)
  }

  async function persistSecureSettings(): Promise<void> {
    await options.postBackend('/config/secure', currentSecureSettingsPayload())
  }

  async function loadHistoryFromBackend(): Promise<PersistedHistoryItem[]> {
    return options.fetchBackendConfig<PersistedHistoryItem[]>('/history')
  }

  async function saveHistoryToBackend(items: PersistedHistoryItem[]): Promise<void> {
    await options.postBackend('/history', items)
  }

  async function clearHistoryInBackend(): Promise<void> {
    await options.deleteBackend('/history')
  }

  async function loadLegacyMigrationHistory(): Promise<LegacyMigrationRecord[]> {
    return options.fetchBackendConfig<LegacyMigrationRecord[]>('/config/legacy-migrations')
  }

  async function markLegacyMigration(version: number, name: string): Promise<void> {
    await options.postBackend('/config/legacy-migrations', { version, name })
  }

  async function migrateLegacySettingsIfNeeded(): Promise<void> {
    const appliedVersions = new Set(
      (await loadLegacyMigrationHistory().catch(() => []))
        .map((item) => item.version),
    )

    if (!appliedVersions.has(1)) {
      const parsed = readLegacySettings(options.legacySettingsPaths)
      if (parsed) {
        const legacyNopechaApiKey = parsed.nopechaApiKey?.trim()
        const legacyTeraboxAccount = normalizeTeraboxAccount(parsed.accounts?.terabox)
        const legacyBruploadAccount = normalizeBruploadAccount(parsed.accounts?.brupload)
        const legacyPublicSettings = sanitizeForDisk(parsed)

        if (!secureState.nopechaApiKey && legacyNopechaApiKey) {
          secureState.nopechaApiKey = legacyNopechaApiKey
        }
        if (!secureState.teraboxAccount && legacyTeraboxAccount) {
          secureState.teraboxAccount = legacyTeraboxAccount
        }
        if (!secureState.bruploadAccount && legacyBruploadAccount) {
          secureState.bruploadAccount = legacyBruploadAccount
        }

        cachedPublicSettings = mergeLegacyPublicSettings(cachedPublicSettings, legacyPublicSettings)

        await persistSecureSettings()
        await persistPublicSettings(cachedPublicSettings)
      }

      await markLegacyMigration(1, 'settings_json_to_sqlite')
    }

    if (!appliedVersions.has(2)) {
      const backendHistory = await loadHistoryFromBackend().catch(() => [])
      if (backendHistory.length === 0) {
        const legacyHistory = readLegacyHistory(options.legacyHistoryPaths)
        if (legacyHistory.length > 0) {
          await saveHistoryToBackend(legacyHistory)
        }
      }
      await markLegacyMigration(2, 'history_json_to_sqlite')
    }
  }

  function setNopechaApiKey(value: string | undefined): void {
    secureState.nopechaApiKey = value?.trim() ?? ''
  }

  function persistTeraboxAccount(account: TeraboxStoredAccount | null): Promise<void> {
    secureState.teraboxAccount = normalizeTeraboxAccount(account)
    return persistSecureSettings()
  }

  function persistBruploadAccount(account: BruploadStoredAccount | null): Promise<void> {
    secureState.bruploadAccount = normalizeBruploadAccount(account)
    return persistSecureSettings()
  }

  function getBruploadAccount(): BruploadStoredAccount | null {
    return secureState.bruploadAccount
  }

  function getTeraboxAccount(): TeraboxStoredAccount | null {
    return secureState.teraboxAccount
  }

  function getNopechaApiKey(): string {
    return secureState.nopechaApiKey
  }

  function getPublicSettings(): PersistedSettings {
    return {
      ...defaultPublicSettings,
      ...cachedPublicSettings,
    }
  }

  return {
    applySecureSettingsPayload,
    currentSecureSettingsPayload,
    currentSettingsSnapshot,
    getBruploadAccount,
    getNopechaApiKey,
    getPublicSettings,
    getTeraboxAccount,
    loadHistoryFromBackend,
    loadPublicSettings,
    loadSecureSettings,
    migrateLegacySettingsIfNeeded,
    persistBruploadAccount,
    persistPublicSettings,
    persistSecureSettings,
    persistTeraboxAccount,
    saveHistoryToBackend,
    clearHistoryInBackend,
    setNopechaApiKey,
  }
}
