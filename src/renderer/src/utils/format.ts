export function formatBytes(bytes: number): string {
  if (!bytes || bytes <= 0) return '0 B'
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 ** 2) return `${(bytes / 1024).toFixed(1)} KB`
  if (bytes < 1024 ** 3) return `${(bytes / 1024 ** 2).toFixed(1)} MB`
  return `${(bytes / 1024 ** 3).toFixed(2)} GB`
}

export function formatSpeed(bytesPerSecond: number): string {
  if (!bytesPerSecond || bytesPerSecond <= 0) return '0 KB/s'
  if (bytesPerSecond < 1024 ** 2) return `${(bytesPerSecond / 1024).toFixed(0)} KB/s`
  return `${(bytesPerSecond / (1024 ** 2)).toFixed(1)} MB/s`
}

const MINUTE = 60
const HOUR = 3600
const DAY = 86_400
const WEEK = 7 * DAY
const MONTH = 30 * DAY // aproximação para exibição de ETA
const YEAR = 365 * DAY

export function formatEta(seconds: number): string {
  if (!seconds || seconds <= 0) return '0s'
  if (seconds < MINUTE) return `${seconds}s`
  if (seconds < HOUR) {
    const minutes = Math.floor(seconds / MINUTE)
    const remainingSeconds = seconds % MINUTE
    return `${minutes}m ${String(remainingSeconds).padStart(2, '0')}s`
  }
  if (seconds < DAY) {
    const hours = Math.floor(seconds / HOUR)
    const remainingMinutes = Math.floor((seconds % HOUR) / MINUTE)
    return `${hours}h ${String(remainingMinutes).padStart(2, '0')}m`
  }
  // Downloads muito longos (>24h): dias, semanas, meses e anos.
  if (seconds < WEEK) {
    const days = Math.floor(seconds / DAY)
    const hours = Math.floor((seconds % DAY) / HOUR)
    return `${days}d ${hours}h`
  }
  if (seconds < MONTH) {
    const weeks = Math.floor(seconds / WEEK)
    const days = Math.floor((seconds % WEEK) / DAY)
    return `${weeks}sem ${days}d`
  }
  if (seconds < YEAR) {
    const months = Math.floor(seconds / MONTH)
    const days = Math.floor((seconds % MONTH) / DAY)
    return `${months}mês ${days}d`
  }
  const years = Math.floor(seconds / YEAR)
  const months = Math.floor((seconds % YEAR) / MONTH)
  return `${years}a ${months}mês`
}

export function formatMediaDuration(seconds: number): string {
  if (!seconds || seconds <= 0) return ''

  const totalSeconds = Math.round(seconds)
  const hours = Math.floor(totalSeconds / 3600)
  const minutes = Math.floor((totalSeconds % 3600) / 60)
  const remainingSeconds = totalSeconds % 60

  if (hours > 0) {
    return `${hours}h ${String(minutes).padStart(2, '0')}m ${String(remainingSeconds).padStart(2, '0')}s`
  }

  return `${minutes}m ${String(remainingSeconds).padStart(2, '0')}s`
}

export function formatDuration(milliseconds: number): string {
  if (!milliseconds || milliseconds <= 0) return '0s'
  return formatEta(Math.max(0, Math.round(milliseconds / 1000)))
}
