import { describe, expect, it } from 'vitest'

import { getFileTypeAppIcon } from '../file-type-icons'

describe('file-type app icons', () => {
  it('mapeia vídeo para o VLC', () => {
    expect(getFileTypeAppIcon('filme.mkv')?.app).toBe('vlc')
    expect(getFileTypeAppIcon('a/b/clip.MP4')?.app).toBe('vlc')
  })

  it('mapeia arquivos compactados', () => {
    expect(getFileTypeAppIcon('pacote.rar')?.app).toBe('winrar')
    expect(getFileTypeAppIcon('backup.zip')?.app).toBe('archive')
    expect(getFileTypeAppIcon('x.7z')?.app).toBe('archive')
  })

  it('mapeia documentos e instaladores', () => {
    expect(getFileTypeAppIcon('doc.pdf')?.app).toBe('pdf')
    expect(getFileTypeAppIcon('planilha.xlsx')?.app).toBe('excel')
    expect(getFileTypeAppIcon('app.apk')?.app).toBe('android')
    expect(getFileTypeAppIcon('setup.exe')?.app).toBe('windows')
    expect(getFileTypeAppIcon('imagem.iso')?.app).toBe('disk')
  })

  it('ignora query/hash e sem extensão', () => {
    expect(getFileTypeAppIcon('video.mkv?token=abc#x')?.app).toBe('vlc')
    expect(getFileTypeAppIcon('semextensao')).toBeNull()
    expect(getFileTypeAppIcon('desconhecido.xyz')).toBeNull()
  })

  it('cada ícone de app resolvido tem um SVG', () => {
    expect(getFileTypeAppIcon('a.mkv')?.svg.startsWith('<svg')).toBe(true)
  })
})
