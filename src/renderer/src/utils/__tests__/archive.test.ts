import { describe, expect, it } from 'vitest'
import { isArchiveFilename } from '../archive'

describe('isArchiveFilename', () => {
  it('reconhece extensões de archive comuns', () => {
    for (const name of ['a.rar', 'b.zip', 'c.7z', 'd.tar', 'e.tar.gz', 'f.tgz', 'g.gz', 'h.bz2', 'i.xz', 'j.zst']) {
      expect(isArchiveFilename(name)).toBe(true)
    }
  })

  it('reconhece partes multipart do rar/7z', () => {
    expect(isArchiveFilename('filme.part1.rar')).toBe(true)
    expect(isArchiveFilename('filme.7z.001')).toBe(true)
  })

  it('ignora não-archives', () => {
    for (const name of ['video.mp4', 'doc.pdf', 'audio.mp3', 'foto.png', '']) {
      expect(isArchiveFilename(name)).toBe(false)
    }
  })

  it('é case-insensitive', () => {
    expect(isArchiveFilename('X.RAR')).toBe(true)
    expect(isArchiveFilename('Y.Zip')).toBe(true)
  })
})
