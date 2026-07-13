import { describe, expect, it } from 'vitest'

import { normalizeUrlCandidate, parseUrls, truncateUrl } from '../link-grabber'

describe('link-grabber utils', () => {
  it('normaliza hash e barra final ao deduplicar urls', () => {
    expect(normalizeUrlCandidate('https://katfile.com/u1ifmhkgsyjx/#frag')).toBe('https://katfile.com/u1ifmhkgsyjx')
    expect(normalizeUrlCandidate('https://brfiles.com/d/kAZST1Am/')).toBe('https://brfiles.com/d/kAZST1Am')
  })

  it('preserva a chave do Mega no fragmento da url', () => {
    expect(normalizeUrlCandidate('https://mega.nz/folder/hsxRgK6Z#P6nUQv6FdRpVH32P_llqYg')).toBe(
      'https://mega.nz/folder/hsxRgK6Z#P6nUQv6FdRpVH32P_llqYg',
    )
  })

  it('deduplica urls em lote', () => {
    const urls = parseUrls([
      'https://katfile.com/u1ifmhkgsyjx',
      'https://katfile.com/u1ifmhkgsyjx#abc',
      'https://brfiles.com/d/kAZST1Am/',
    ].join('\n'))

    expect(urls).toEqual([
      'https://katfile.com/u1ifmhkgsyjx',
      'https://brfiles.com/d/kAZST1Am',
    ])
  })

  it('extrai varios links do youtube mesmo quando vierem colados com espacos', () => {
    const urls = parseUrls(
      'https://youtu.be/xF8l17MJkMk?si=S-AEA4ToNixGWrE5 https://youtu.be/YirJG0bNHUI?si=4h3LQhrIk7q2I7Sp\n  https://youtu.be/pUscL1uCSrk?si=aoPm1kILDPwluytb',
    )

    expect(urls).toEqual([
      'https://youtu.be/xF8l17MJkMk?si=S-AEA4ToNixGWrE5',
      'https://youtu.be/YirJG0bNHUI?si=4h3LQhrIk7q2I7Sp',
      'https://youtu.be/pUscL1uCSrk?si=aoPm1kILDPwluytb',
    ])
  })

  it('aceita links sem esquema (IP:porta/caminho) prefixando http://', () => {
    expect(
      normalizeUrlCandidate('151.247.155.169:3000/download/abc/letters.mkv'),
    ).toBe('http://151.247.155.169:3000/download/abc/letters.mkv')
    expect(normalizeUrlCandidate('http://151.247.155.169:3000/download/abc/letters.mkv')).toBe(
      'http://151.247.155.169:3000/download/abc/letters.mkv',
    )
  })

  it('não confunde nome de arquivo solto com url', () => {
    expect(normalizeUrlCandidate('video.mkv')).toBe('')
    expect(normalizeUrlCandidate('arquivo final.rar')).toBe('')
  })

  it('extrai link cru com caminho de um texto', () => {
    const urls = parseUrls('baixe em exemplo.com/pasta/arquivo.zip agora')
    expect(urls).toEqual(['http://exemplo.com/pasta/arquivo.zip'])
  })

  it('trunca urls longas pelo último segmento útil', () => {
    expect(truncateUrl('https://brfiles.com/f/MoQFnG5r/Hannibal.S02e01.1080P.WEB-DL.DUAL.DUBLASERIES.TV.mkv'))
      .toContain('Hannibal')
  })
})
