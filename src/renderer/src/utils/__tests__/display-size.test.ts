import { describe, expect, it } from 'vitest'
import { effectiveSize } from '../display-size'

describe('effectiveSize', () => {
  it('usa o tamanho do próprio item quando > 0', () => {
    expect(effectiveSize(1000, [{ size: 10 }, { size: 20 }])).toBe(1000)
  })

  it('cai para a soma dos filhos quando o item é 0 e é pasta', () => {
    expect(effectiveSize(0, [{ size: 10 }, { size: 20 }], true)).toBe(30)
  })

  it('cai para o maior filho quando o item é 0 e NÃO é pasta (formatos alternativos)', () => {
    expect(effectiveSize(0, [{ size: 10 }, { size: 25 }], false)).toBe(25)
  })

  it('retorna 0 quando não há tamanho conhecido', () => {
    expect(effectiveSize(0, [], false)).toBe(0)
    expect(effectiveSize(0, undefined, false)).toBe(0)
  })
})
