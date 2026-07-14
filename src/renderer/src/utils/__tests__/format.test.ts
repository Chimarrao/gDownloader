import { describe, expect, it } from 'vitest'

import { formatEta } from '../format'

describe('formatEta', () => {
  it('segundos, minutos e horas (curto)', () => {
    expect(formatEta(0)).toBe('0s')
    expect(formatEta(45)).toBe('45s')
    expect(formatEta(90)).toBe('1m 30s')
    expect(formatEta(3660)).toBe('1h 01m')
  })

  it('dias, semanas, meses e anos (longos)', () => {
    expect(formatEta(86_400 + 3600 * 5)).toBe('1d 5h') // 1 dia e 5h
    expect(formatEta(86_400 * 10)).toBe('1sem 3d') // 10 dias
    expect(formatEta(86_400 * 45)).toBe('1mês 15d') // 45 dias
    expect(formatEta(86_400 * 400)).toBe('1a 1mês') // ~1 ano e 1 mês
  })
})
