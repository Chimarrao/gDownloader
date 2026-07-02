import { describe, expect, it } from 'vitest'

import { pruneCapturedRows } from '../capture-selection'

// Minimal mock shapes matching CapturedRow structure
type MockChild = { sourceUrl?: string; selected?: boolean }
type MockInfo = { children?: MockChild[] | null } | null
type MockRow = { url: string; selected: boolean; info: MockInfo }

describe('pruneCapturedRows', () => {
  it('(a) remove arquivo simples selecionado, mantém não-selecionado', () => {
    const rows: MockRow[] = [
      { url: 'https://example.com/file1.zip', selected: true, info: null },
      { url: 'https://example.com/file2.zip', selected: false, info: null },
    ]

    const result = pruneCapturedRows(rows)

    expect(result).toHaveLength(1)
    expect(result[0].url).toBe('https://example.com/file2.zip')
  })

  it('(b) pasta com todos os filhos escolhidos é removida por completo', () => {
    const rows: MockRow[] = [
      {
        url: 'https://example.com/folder',
        selected: true,
        info: {
          children: [
            { sourceUrl: 'https://example.com/folder/a.mp4', selected: true },
            { sourceUrl: 'https://example.com/folder/b.mp4' }, // selected === undefined → chosen
          ],
        },
      },
    ]

    const result = pruneCapturedRows(rows)

    expect(result).toHaveLength(0)
  })

  it('(c) pasta com alguns filhos escolhidos mantém a linha com apenas os não-escolhidos', () => {
    const rows: MockRow[] = [
      {
        url: 'https://example.com/folder',
        selected: true,
        info: {
          children: [
            { sourceUrl: 'https://example.com/folder/a.mp4', selected: true },
            { sourceUrl: 'https://example.com/folder/b.mp4', selected: false }, // not chosen
            { sourceUrl: 'https://example.com/folder/c.mp4' }, // undefined → chosen
          ],
        },
      },
    ]

    const result = pruneCapturedRows(rows)

    expect(result).toHaveLength(1)
    expect(result[0].url).toBe('https://example.com/folder')
    const children = (result[0].info as { children: MockChild[] }).children
    expect(children).toHaveLength(1)
    expect(children[0].sourceUrl).toBe('https://example.com/folder/b.mp4')
  })

  it('(d) linhas sem nada selecionado ficam intactas', () => {
    const rows: MockRow[] = [
      { url: 'https://example.com/file1.zip', selected: false, info: null },
      {
        url: 'https://example.com/folder',
        selected: false,
        info: {
          children: [
            { sourceUrl: 'https://example.com/folder/a.mp4', selected: false },
            { sourceUrl: 'https://example.com/folder/b.mp4', selected: false },
          ],
        },
      },
    ]

    const result = pruneCapturedRows(rows)

    expect(result).toHaveLength(2)
    expect(result).toEqual(rows)
  })

  it('linha com info.children vazia é tratada como arquivo simples', () => {
    const rows: MockRow[] = [
      { url: 'https://example.com/file.zip', selected: true, info: { children: [] } },
      { url: 'https://example.com/other.zip', selected: false, info: { children: [] } },
    ]

    const result = pruneCapturedRows(rows)

    expect(result).toHaveLength(1)
    expect(result[0].url).toBe('https://example.com/other.zip')
  })
})
