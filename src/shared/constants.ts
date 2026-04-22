export enum DownloadStatus {
  Pending = 'pending',
  Downloading = 'downloading',
  Complete = 'complete',
  Error = 'error',
  Cancelled = 'cancelled',
  Paused = 'paused',
  RateLimited = 'rate_limited',
  WaitingCaptcha = 'waiting_captcha'
}
