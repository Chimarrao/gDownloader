export enum DownloadStatus {
  Pending = 'pending',
  Downloading = 'downloading',
  Verifying = 'verifying',
  Complete = 'complete',
  Corrupted = 'corrupted',
  Error = 'error',
  Cancelled = 'cancelled',
  Paused = 'paused',
  RateLimited = 'rate_limited',
  WaitingCaptcha = 'waiting_captcha',
  DiskFull = 'disk_full'
}
