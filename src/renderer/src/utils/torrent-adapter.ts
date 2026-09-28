import { DownloadStatus, type DownloadChild, type DownloadItem, type TorrentStatus } from '../../../shared/types'

// IDs de torrent vêm do Go (tabela `torrents`), IDs de download vêm do Rust
// (tabela `downloads`) — os dois espaços de UUID nunca colidem, mas o prefixo
// deixa explícito de onde cada item da lista fundida veio e evita qualquer
// ambiguidade ao decidir pra qual API (torrents.* vs downloads.*) uma ação
// deve ir.
const TORRENT_ID_PREFIX = 'torrent:'

export function torrentItemId(rawId: string): string {
  return `${TORRENT_ID_PREFIX}${rawId}`
}

export function isTorrentItemId(id: string): boolean {
  return id.startsWith(TORRENT_ID_PREFIX)
}

export function torrentRawId(id: string): string {
  return id.slice(TORRENT_ID_PREFIX.length)
}

function mapTorrentStatus(status: TorrentStatus['status']): DownloadStatus {
  switch (status) {
    case 'fetching_metadata':
      return DownloadStatus.Pending
    case 'downloading':
      return DownloadStatus.Downloading
    case 'checking':
      return DownloadStatus.Verifying
    case 'paused':
      return DownloadStatus.Paused
    case 'error':
      return DownloadStatus.Error
    case 'seeding':
    case 'done':
      return DownloadStatus.Complete
    default:
      return DownloadStatus.Pending
  }
}

function magnetDisplayName(source: string): string {
  const queryIndex = source.indexOf('?')
  if (queryIndex < 0) return 'Torrent'
  const params = new URLSearchParams(source.slice(queryIndex + 1))
  return params.get('dn')?.trim() || 'Torrent'
}

function torrentChildStatus(bytesCompleted: number, size: number, selected: boolean): DownloadStatus {
  if (size > 0 && bytesCompleted >= size) return DownloadStatus.Complete
  if (!selected) return DownloadStatus.Paused
  return DownloadStatus.Downloading
}

export function torrentToDownloadItem(t: TorrentStatus): DownloadItem {
  const children: DownloadChild[] | undefined = t.files?.map((f) => ({
    filename: f.path,
    size: f.size,
    isFolder: false,
    bytesDownloaded: f.bytesCompleted,
    status: torrentChildStatus(f.bytesCompleted, f.size, f.selected),
  }))

  const remaining = t.totalBytes - t.bytesCompleted
  const etaSec = t.downloadBps > 0 && remaining > 0 ? Math.round(remaining / t.downloadBps) : 0

  return {
    id: torrentItemId(t.id),
    url: t.source,
    moduleId: 'torrent',
    title: t.name?.trim() || magnetDisplayName(t.source),
    size: t.totalBytes,
    isFolder: (children?.length ?? 0) > 1,
    children,
    status: mapTorrentStatus(t.status),
    percent: Math.round((t.progress ?? 0) * 100),
    speedBps: t.downloadBps,
    etaSec,
    error: t.error ?? '',
    errorKind: t.error ? 'temporary' : undefined,
    outputPath: t.destDir,
    addedAt: t.createdAt * 1000,
    torRequired: t.torRequired,
    numPeers: t.numPeers,
    numSeeds: t.numSeeds,
    uploadBps: t.uploadBps,
    torrentInfoHash: t.infoHash,
  }
}
