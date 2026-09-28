import { describe, expect, it } from 'vitest'

import { DownloadStatus } from '../../../../shared/types'
import { isTorrentItemId, torrentRawId, torrentToDownloadItem } from '../torrent-adapter'

function baseTorrent() {
  return {
    id: 'abc-123',
    infoHash: 'deadbeef',
    name: '',
    source: 'magnet:?xt=urn:btih:deadbeef&dn=Filme.1080p',
    sourceKind: 'magnet' as const,
    destDir: '/tmp/dest',
    torRequired: false,
    paused: false,
    checking: false,
    status: 'fetching_metadata' as const,
    bytesCompleted: 0,
    totalBytes: 0,
    progress: 0,
    downloadBps: 0,
    uploadBps: 0,
    numPeers: 0,
    numSeeds: 0,
    createdAt: 1_700_000_000,
  }
}

describe('torrentItemId / isTorrentItemId / torrentRawId', () => {
  it('prefixa e desfaz o prefixo sem perder o id original', () => {
    const item = torrentToDownloadItem(baseTorrent())
    expect(isTorrentItemId(item.id)).toBe(true)
    expect(torrentRawId(item.id)).toBe('abc-123')
  })

  it('não confunde id de download normal com id de torrent', () => {
    expect(isTorrentItemId('550e8400-e29b-41d4-a716-446655440000')).toBe(false)
  })
})

describe('torrentToDownloadItem', () => {
  it('usa o dn do magnet como título quando ainda não há metadados', () => {
    const item = torrentToDownloadItem(baseTorrent())
    expect(item.title).toBe('Filme.1080p')
    expect(item.moduleId).toBe('torrent')
    expect(item.status).toBe(DownloadStatus.Pending)
  })

  it('mapeia status de swarm pros status de DownloadItem', () => {
    expect(torrentToDownloadItem({ ...baseTorrent(), status: 'downloading' }).status).toBe(DownloadStatus.Downloading)
    expect(torrentToDownloadItem({ ...baseTorrent(), status: 'seeding' }).status).toBe(DownloadStatus.Complete)
    expect(torrentToDownloadItem({ ...baseTorrent(), status: 'done' }).status).toBe(DownloadStatus.Complete)
    expect(torrentToDownloadItem({ ...baseTorrent(), status: 'paused' }).status).toBe(DownloadStatus.Paused)
    expect(torrentToDownloadItem({ ...baseTorrent(), status: 'checking' }).status).toBe(DownloadStatus.Verifying)
    expect(torrentToDownloadItem({ ...baseTorrent(), status: 'error', error: 'falhou' }).status).toBe(DownloadStatus.Error)
  })

  it('converte createdAt de segundos (Go) pra milissegundos (addedAt)', () => {
    const item = torrentToDownloadItem(baseTorrent())
    expect(item.addedAt).toBe(1_700_000_000_000)
  })

  it('mapeia files[] pra children[] preservando bytes baixados por arquivo', () => {
    const item = torrentToDownloadItem({
      ...baseTorrent(),
      status: 'downloading',
      totalBytes: 300,
      files: [
        { index: 0, path: 'a.mkv', size: 100, bytesCompleted: 100, selected: true },
        { index: 1, path: 'b.mkv', size: 200, bytesCompleted: 50, selected: true },
      ],
    })
    expect(item.isFolder).toBe(true)
    expect(item.children).toHaveLength(2)
    expect(item.children?.[0]).toMatchObject({ filename: 'a.mkv', status: DownloadStatus.Complete })
    expect(item.children?.[1]).toMatchObject({ filename: 'b.mkv', bytesDownloaded: 50, status: DownloadStatus.Downloading })
  })

  it('calcula eta a partir de bytes restantes e velocidade atual', () => {
    const item = torrentToDownloadItem({
      ...baseTorrent(),
      status: 'downloading',
      totalBytes: 1000,
      bytesCompleted: 400,
      downloadBps: 100,
    })
    expect(item.etaSec).toBe(6)
  })
})
