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

  it('trunca urls longas pelo último segmento útil', () => {
    expect(truncateUrl('https://brfiles.com/f/MoQFnG5r/Hannibal.S02e01.1080P.WEB-DL.DUAL.DUBLASERIES.TV.mkv'))
      .toContain('Hannibal')
  })
})
