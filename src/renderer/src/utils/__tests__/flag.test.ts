import { describe, expect, it } from 'vitest'
import { flagClass } from '../flag'

describe('flagClass', () => {
  it('gera a classe fi para um código de país válido', () => {
    expect(flagClass('US')).toBe('fi fi-us')
    expect(flagClass('de')).toBe('fi fi-de')
  })
  it('retorna string vazia para código inválido/ausente', () => {
    expect(flagClass('')).toBe('')
    expect(flagClass(undefined)).toBe('')
    expect(flagClass('XX!')).toBe('')
  })
})
