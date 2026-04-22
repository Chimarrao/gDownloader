import { DownloadStatus } from '../../../shared/constants'
import type { DownloadChild, DownloadItem } from '../../../shared/types'

export type DownloadSortMode =
  | 'newest'
  | 'oldest'
  | 'active_first'
  | 'name_asc'
  | 'name_desc'
  | 'size_desc'
  | 'size_asc'
  | 'progress_desc'
  | 'speed_desc'

export const DOWNLOAD_SORT_OPTIONS: Array<{ value: DownloadSortMode; label: string }> = [
  { value: 'newest', label: 'Mais recentes' },
  { value: 'oldest', label: 'Mais antigos' },
  { value: 'active_first', label: 'Ativos primeiro' },
  { value: 'name_asc', label: 'Nome A-Z' },
  { value: 'name_desc', label: 'Nome Z-A' },
  { value: 'size_desc', label: 'Maior tamanho' },
  { value: 'size_asc', label: 'Menor tamanho' },
  { value: 'progress_desc', label: 'Maior progresso' },
  { value: 'speed_desc', label: 'Maior velocidade' },
]

export const STATUS_LABELS: Record<string, string> = {
  [DownloadStatus.Pending]: 'Na fila',
  [DownloadStatus.Downloading]: 'Baixando',
  [DownloadStatus.Complete]: 'Concluído',
  [DownloadStatus.Error]: 'Erro',
  [DownloadStatus.Cancelled]: 'Cancelado',
  [DownloadStatus.Paused]: 'Pausado',
  [DownloadStatus.RateLimited]: 'Aguardando',
  [DownloadStatus.WaitingCaptcha]: 'Captcha',
}

export function isTerminal(status: DownloadItem['status']): boolean {
  return (
    status === DownloadStatus.Complete
    || status === DownloadStatus.Error
    || status === DownloadStatus.Cancelled
  )
}

export function activePriority(item: DownloadItem): number {
  if (item.status === DownloadStatus.Downloading) return 0
  if (
    item.status === DownloadStatus.Pending
    || item.status === DownloadStatus.RateLimited
    || item.status === DownloadStatus.WaitingCaptcha
  ) return 1
  if (item.status === DownloadStatus.Paused) return 2
  if (item.status === DownloadStatus.Error) return 3
  if (item.status === DownloadStatus.Cancelled) return 4
  return 5
}

export function effectiveSpeed(item: DownloadItem, nowTick: number): number {
  if (item.status !== DownloadStatus.Downloading) {
    return item.speedBps ?? 0
  }
  if (!item.lastProgressAt) {
    return item.speedBps ?? 0
  }
  return nowTick - item.lastProgressAt > 1800 ? 0 : (item.speedBps ?? 0)
}

export function effectiveEta(item: DownloadItem, nowTick: number): number {
  return effectiveSpeed(item, nowTick) > 0 ? item.etaSec ?? 0 : 0
}

export function statusText(item: DownloadItem, nowTick: number): string {
  if (item.status === DownloadStatus.Downloading && effectiveSpeed(item, nowTick) <= 0) {
    return 'Conectando'
  }
  return STATUS_LABELS[item.status] ?? item.status
}

export function childStatusText(status?: DownloadChild['status']): string {
  return STATUS_LABELS[status ?? DownloadStatus.Pending] ?? 'Na fila'
}

export function retryCountdown(item: DownloadItem, nowTick: number): number {
  if (!item.retryAt || item.retryAt <= nowTick) {
    return 0
  }
  return Math.ceil((item.retryAt - nowTick) / 1000)
}

export function isWaitingRetry(item: DownloadItem, nowTick: number): boolean {
  return item.status === DownloadStatus.Pending && retryCountdown(item, nowTick) > 0
}

export function compareDownloads(
  left: DownloadItem,
  right: DownloadItem,
  sortMode: DownloadSortMode,
  nowTick: number,
): number {
  switch (sortMode) {
    case 'oldest':
      return left.addedAt - right.addedAt || left.title.localeCompare(right.title)
    case 'active_first': {
      const priorityDiff = activePriority(left) - activePriority(right)
      if (priorityDiff !== 0) return priorityDiff
      if (effectiveSpeed(right, nowTick) !== effectiveSpeed(left, nowTick)) {
        return effectiveSpeed(right, nowTick) - effectiveSpeed(left, nowTick)
      }
      return right.addedAt - left.addedAt
    }
    case 'name_asc':
      return left.title.localeCompare(right.title, 'pt-BR', { sensitivity: 'base' })
    case 'name_desc':
      return right.title.localeCompare(left.title, 'pt-BR', { sensitivity: 'base' })
    case 'size_desc':
      return right.size - left.size || right.addedAt - left.addedAt
    case 'size_asc':
      return left.size - right.size || right.addedAt - left.addedAt
    case 'progress_desc':
      return right.percent - left.percent || right.addedAt - left.addedAt
    case 'speed_desc':
      return effectiveSpeed(right, nowTick) - effectiveSpeed(left, nowTick) || right.addedAt - left.addedAt
    case 'newest':
    default:
      return right.addedAt - left.addedAt || left.title.localeCompare(right.title)
  }
}

export function getDownloadActions(item: DownloadItem): Record<string, boolean> {
  const retryable =
    item.status === DownloadStatus.Paused
    || item.status === DownloadStatus.Error
    || item.status === DownloadStatus.Cancelled
    || item.status === DownloadStatus.RateLimited

  return {
    canPause: item.status === DownloadStatus.Pending || item.status === DownloadStatus.Downloading,
    canResume: item.status === DownloadStatus.Paused,
    canOpenCaptcha: item.status === DownloadStatus.WaitingCaptcha && Boolean(item.captchaSitekey),
    canCancel:
      item.status === DownloadStatus.Pending
      || item.status === DownloadStatus.Downloading
      || item.status === DownloadStatus.Paused
      || item.status === DownloadStatus.RateLimited
      || item.status === DownloadStatus.WaitingCaptcha,
    canRetry: retryable,
    canForce:
      item.status === DownloadStatus.Pending
      || item.status === DownloadStatus.Paused
      || item.status === DownloadStatus.Error
      || item.status === DownloadStatus.RateLimited,
    canRestart:
      item.status === DownloadStatus.Paused
      || item.status === DownloadStatus.Error
      || item.status === DownloadStatus.Cancelled
      || item.status === DownloadStatus.Complete,
    canOpenFolder: item.status === DownloadStatus.Complete && Boolean(item.outputPath),
    canRemove: isTerminal(item.status),
    canRemoveWithFiles: isTerminal(item.status) && Boolean(item.outputPath),
  }
}
