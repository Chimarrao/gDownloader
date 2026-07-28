import { describe, expect, it } from 'vitest'

import { DownloadStatus } from '../../../../shared/constants'
import type { DownloadItem } from '../../../../shared/types'
import {
  compareDownloads,
  effectiveSpeed,
  getDownloadActions,
  isClearable,
  itemNeedsCountdown,
  resolveErrorKind,
  retryCountdown,
  statusText,
  statusTextKey,
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

  it('ordena por grupo de status (ativos primeiro), estável — sem depender da velocidade', () => {
    const now = 10_000
    const downloading = makeItem({ id: 'dl', status: DownloadStatus.Downloading, addedAt: 100 })
    const paused = makeItem({ id: 'pause', status: DownloadStatus.Paused, addedAt: 200 })
    // Baixando vem antes de pausado, independentemente da data.
    expect(compareDownloads(downloading, paused, 'active_first', now)).toBeLessThan(0)

    // Mesmo grupo (ambos baixando): ordem estável por data (mais novo primeiro);
    // a velocidade NÃO reordena (evita as linhas trocando de lugar o tempo todo).
    const older = makeItem({ id: 'older', status: DownloadStatus.Downloading, speedBps: 5 * 1024 * 1024, addedAt: 100 })
    const newer = makeItem({ id: 'newer', status: DownloadStatus.Downloading, speedBps: 1024, addedAt: 200 })
    expect(compareDownloads(newer, older, 'active_first', now)).toBeLessThan(0)
    older.speedBps = 0
    newer.speedBps = 10 * 1024 * 1024
    expect(compareDownloads(newer, older, 'active_first', now)).toBeLessThan(0)
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

  it('classifica errorKind e marca countdown só quando necessário', () => {
    const now = 10_000
    expect(resolveErrorKind(makeItem({ errorKind: 'premium' }))).toBe('premium')
    expect(resolveErrorKind(makeItem({ status: DownloadStatus.Corrupted }))).toBe('integrity')
    expect(resolveErrorKind(makeItem({ status: DownloadStatus.Error, error: 'Arquivo não localizado' }))).toBe(
      'removed',
    )
    expect(statusTextKey(makeItem({ status: DownloadStatus.Downloading, speedBps: 0 }), now)).toBe(
      'statusConnecting',
    )

    const waiting = makeItem({ status: DownloadStatus.RateLimited, retryAt: now + 5000 })
    expect(itemNeedsCountdown(waiting, now)).toBe(true)

    const complete = makeItem({ status: DownloadStatus.Complete })
    expect(itemNeedsCountdown(complete, now)).toBe(false)
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
