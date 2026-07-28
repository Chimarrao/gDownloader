import { describe, expect, it } from 'vitest'

import {
  credentialsAreInsecure,
  generateRemoteAccessCredentials,
  generateStrongRemotePassword,
  normalizeRemoteAccess,
} from '../remote-access-server'
import type { PersistedSettings } from '../../shared/types'

function baseSettings(remote: Partial<PersistedSettings['remoteAccess']> = {}): PersistedSettings {
  return {
    theme: 'light',
    locale: 'pt-BR',
    outputDir: '~/Downloads',
    maxConcurrentDownloads: 3,
    fontSize: 14,
    fontFamily: 'Inter',
    uiZoom: 1,
    nativeNotification: true,
    clipboardMonitorEnabled: false,
    remoteAccess: {
      enabled: false,
      allowLan: false,
      username: 'gdownloader',
      password: '',
      port: 9786,
      ...remote,
    },
  }
}

describe('remote-access security', () => {
  it('gera senha forte com entropia suficiente', () => {
    const password = generateStrongRemotePassword()
    expect(password.startsWith('gd-')).toBe(true)
    expect(password.length).toBeGreaterThanOrEqual(16 + 3)
    expect(credentialsAreInsecure({
      enabled: false,
      allowLan: false,
      username: 'gdownloader',
      password,
      port: 9786,
    })).toBe(false)
  })

  it('marca senhas fracas/ausentes como inseguras', () => {
    expect(credentialsAreInsecure(normalizeRemoteAccess(baseSettings({ password: '' })))).toBe(true)
    expect(credentialsAreInsecure(normalizeRemoteAccess(baseSettings({ password: 'gd-1234' })))).toBe(true)
    expect(credentialsAreInsecure(normalizeRemoteAccess(baseSettings({ password: 'curta' })))).toBe(true)
  })

  it('allowLan default é false e credenciais geradas nascem fortes', () => {
    const generated = generateRemoteAccessCredentials()
    expect(generated.allowLan).toBe(false)
    expect(generated.enabled).toBe(false)
    expect(credentialsAreInsecure(generated)).toBe(false)

    const normalized = normalizeRemoteAccess(baseSettings({ allowLan: true, password: generateStrongRemotePassword() }))
    expect(normalized.allowLan).toBe(true)
  })
})
