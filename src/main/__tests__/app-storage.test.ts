import { mkdtempSync, rmSync, writeFileSync } from 'fs'
import { tmpdir } from 'os'
import { join } from 'path'
import { afterEach, describe, expect, it } from 'vitest'

import { createAppStorage } from '../app-storage'

const tempDirs: string[] = []

afterEach(() => {
  for (const dir of tempDirs.splice(0)) {
    rmSync(dir, { recursive: true, force: true })
  }
})

function makeTempDir(): string {
  const dir = mkdtempSync(join(tmpdir(), 'gdownloader-app-storage-'))
  tempDirs.push(dir)
  return dir
}

describe('app-storage legacy migration', () => {
  it('migra settings legadas para estado público e seguro no SQLite backend', async () => {
    const dir = makeTempDir()
    const legacySettingsPath = join(dir, 'settings.json')
    const legacyHistoryPath = join(dir, 'history.json')

    writeFileSync(legacySettingsPath, JSON.stringify({
      theme: 'dark-default',
      outputDir: '/tmp/downloads',
      maxConcurrentDownloads: 5,
      nopechaApiKey: 'nopecha_test',
      accounts: {
        terabox: {
          email: 'user@example.com',
          password: 'secret',
          cookies: ['a=b'],
          verifiedAt: '2026-04-21T12:00:00.000Z',
        },
      },
    }))
    writeFileSync(legacyHistoryPath, JSON.stringify([
      { id: '1', url: 'https://example.com', title: 'Arquivo', thumbnail: '', date: '2026-04-21', formatId: 'mp4' },
    ]))

    let publicSettings = {}
    let secureSettings = {}
    let history: unknown[] = []
    const migrations: Array<{ version: number; name: string }> = []

    const storage = createAppStorage({
      legacySettingsPaths: [legacySettingsPath],
      legacyHistoryPaths: [legacyHistoryPath],
      fetchBackendConfig: async <T>(path: string) => {
        switch (path) {
          case '/config/public':
            return publicSettings as T
          case '/config/secure':
            return secureSettings as T
          case '/config/legacy-migrations':
            return migrations as T
          case '/history':
            return history as T
          default:
            return null as T
        }
      },
      postBackend: async (path, payload) => {
        if (path === '/config/public') publicSettings = payload as Record<string, unknown>
        if (path === '/config/secure') secureSettings = payload as Record<string, unknown>
        if (path === '/history') history = payload as unknown[]
        if (path === '/config/legacy-migrations') migrations.push(payload as { version: number; name: string })
      },
      deleteBackend: async () => undefined,
    })

    await storage.loadPublicSettings()
    await storage.loadSecureSettings()
    await storage.migrateLegacySettingsIfNeeded()

    const snapshot = storage.currentSettingsSnapshot()
    expect(snapshot.theme).toBe('dark-default')
    expect(snapshot.outputDir).toBe('/tmp/downloads')
    expect(snapshot.maxConcurrentDownloads).toBe(5)
    expect(snapshot.nopechaApiKey).toBe('nopecha_test')
    expect(storage.getTeraboxAccount()?.email).toBe('user@example.com')
    expect(history).toHaveLength(1)
    expect(migrations.map((entry) => entry.version)).toEqual([1, 2])
  })
})
