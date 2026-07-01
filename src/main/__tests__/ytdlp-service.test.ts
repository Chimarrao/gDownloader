import { describe, expect, it } from 'vitest'
import { assetNameForPlatform, resolvedBinPath } from '../ytdlp-service'

describe('assetNameForPlatform', () => {
  it('retorna yt-dlp_macos para darwin arm64', () => {
    expect(assetNameForPlatform('darwin', 'arm64')).toBe('yt-dlp_macos')
  })

  it('retorna yt-dlp_macos para darwin x64', () => {
    expect(assetNameForPlatform('darwin', 'x64')).toBe('yt-dlp_macos')
  })

  it('retorna yt-dlp.exe para win32', () => {
    expect(assetNameForPlatform('win32', 'x64')).toBe('yt-dlp.exe')
  })

  it('retorna yt-dlp para linux x64', () => {
    expect(assetNameForPlatform('linux', 'x64')).toBe('yt-dlp')
  })

  it('retorna yt-dlp_linux_aarch64 para linux arm64', () => {
    expect(assetNameForPlatform('linux', 'arm64')).toBe('yt-dlp_linux_aarch64')
  })
})

describe('resolvedBinPath', () => {
  it('retorna caminho customizado quando ytdlpBinPath esta preenchido', () => {
    expect(resolvedBinPath('/usr/local/bin/yt-dlp', '/managed/yt-dlp')).toBe('/usr/local/bin/yt-dlp')
  })

  it('retorna caminho gerenciado quando ytdlpBinPath esta vazio', () => {
    expect(resolvedBinPath('', '/managed/yt-dlp')).toBe('/managed/yt-dlp')
  })

  it('retorna caminho gerenciado quando ytdlpBinPath e apenas espacos', () => {
    expect(resolvedBinPath('   ', '/managed/yt-dlp')).toBe('/managed/yt-dlp')
  })
})
