import { describe, expect, it } from 'vitest'
import { commonSystemPaths, downloadSourceFor } from '../ffmpeg-service'

describe('downloadSourceFor', () => {
  it('usa zip do BtbN no Windows', () => {
    const source = downloadSourceFor('win32', 'x64')
    expect(source.kind).toBe('zip')
    expect(source.url).toContain('win64-gpl.zip')
  })

  it('usa o zip do evermeet no macOS', () => {
    const source = downloadSourceFor('darwin', 'arm64')
    expect(source.kind).toBe('zip')
    expect(source.url).toContain('evermeet.cx')
  })

  it('usa tar.xz do BtbN no Linux x64', () => {
    const source = downloadSourceFor('linux', 'x64')
    expect(source.kind).toBe('tarxz')
    expect(source.url).toContain('linux64-gpl.tar.xz')
  })

  it('usa o build arm64 no Linux arm64', () => {
    const source = downloadSourceFor('linux', 'arm64')
    expect(source.url).toContain('linuxarm64-gpl.tar.xz')
  })
})

describe('commonSystemPaths', () => {
  it('inclui os caminhos do Homebrew no macOS', () => {
    const paths = commonSystemPaths('darwin')
    expect(paths).toContain('/opt/homebrew/bin/ffmpeg')
    expect(paths).toContain('/usr/local/bin/ffmpeg')
  })

  it('inclui ffmpeg.exe no Windows', () => {
    expect(commonSystemPaths('win32').every((p) => p.endsWith('.exe'))).toBe(true)
  })
})
