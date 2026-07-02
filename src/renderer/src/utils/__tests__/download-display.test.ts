import { describe, expect, it } from 'vitest'

import { DownloadStatus } from '../../../../shared/constants'
import type { DownloadItem } from '../../../../shared/types'
import {
  compareDownloads,
  effectiveSpeed,
  getDownloadActions,
  isClearable,
  retryCountdown,
  statusText,
} from '../download-display'

function makeItem(overrides: Partial<DownloadItem> = {}): DownloadItem {
  return {
    id: '1',
    url: 'https://example.com/file',
    moduleId: 'mega',
    title: 'arquivo.mkv',
    size: 1000,
    status: DownloadStatus.Pending,
    percent: 0,
    speedBps: 0,
    etaSec: 0,
    error: '',
    addedAt: 1000,
    ...overrides,
  }
}

describe('download-display helpers', () => {
  it('zera velocidade efetiva quando progresso ficou stale', () => {
    const now = 10_000
    const item = makeItem({
      status: DownloadStatus.Downloading,
      speedBps: 2048,
      lastProgressAt: now - 5500,
    })

    expect(effectiveSpeed(item, now)).toBe(0)
    expect(statusText(item, now)).toBe('Conectando')
  })

  it('ordena ativos primeiro e depois por velocidade', () => {
    const now = 10_000
    const fast = makeItem({
      id: 'fast',
      status: DownloadStatus.Downloading,
      speedBps: 1024 * 1024,
      lastProgressAt: now,
    })
    const slow = makeItem({
      id: 'slow',
      status: DownloadStatus.Downloading,
      speedBps: 128 * 1024,
      lastProgressAt: now,
    })

    expect(compareDownloads(fast, slow, 'active_first', now)).toBeLessThan(0)
  })

  it('expõe contagem regressiva de retry', () => {
    const now = 5_000
    const item = makeItem({
      retryAt: now + 8_000,
    })

    expect(retryCountdown(item, now)).toBe(8)
  })

  it('expõe ações corretas para waiting captcha', () => {
    const item = makeItem({
      status: DownloadStatus.WaitingCaptcha,
      captchaSitekey: 'abc',
    })

    const actions = getDownloadActions(item)
    expect(actions.canOpenCaptcha).toBe(true)
    expect(actions.canCancel).toBe(true)
    expect(actions.canPause).toBe(false)
  })

  it('limpar concluídos preserva downloads interrompidos na metade', () => {
    // Bug: um download do 1fichier na metade (Error, 50%) era apagado ao limpar.
    const halfErrored = makeItem({ status: DownloadStatus.Error, percent: 50 })
    expect(isClearable(halfErrored)).toBe(false)

    // Concluídos e falhas sem progresso continuam sendo limpáveis.
    expect(isClearable(makeItem({ status: DownloadStatus.Complete, percent: 100 }))).toBe(true)
    expect(isClearable(makeItem({ status: DownloadStatus.Error, percent: 0 }))).toBe(true)
    expect(isClearable(makeItem({ status: DownloadStatus.Cancelled, percent: 0 }))).toBe(true)

    // Ativos nunca são limpáveis.
    expect(isClearable(makeItem({ status: DownloadStatus.Downloading, percent: 30 }))).toBe(false)
    expect(isClearable(makeItem({ status: DownloadStatus.RateLimited, percent: 10 }))).toBe(false)
  })
})
