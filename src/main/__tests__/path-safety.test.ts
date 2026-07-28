import { homedir } from 'os'
import { join } from 'path'
import { describe, expect, it } from 'vitest'

import { assertSafeFilesystemPath, assertSafeHttpUrl, isPathInside } from '../path-safety'

describe('path-safety', () => {
  it('aceita paths dentro da home', () => {
    const home = homedir()
    const target = join(home, 'Downloads', 'arquivo.zip')
    expect(assertSafeFilesystemPath(target, [home])).toBe(target)
  })

  it('rejeita paths fora das raízes permitidas', () => {
    const home = homedir()
    expect(() => assertSafeFilesystemPath('/etc/passwd', [home])).toThrow(/permitidas/)
  })

  it('rejeita null bytes', () => {
    expect(() => assertSafeFilesystemPath(`${homedir()}/a\0b`, [homedir()])).toThrow(/inválido/)
  })

  it('isPathInside respeita prefixo de diretório', () => {
    const home = homedir()
    expect(isPathInside(home, join(home, 'x'))).toBe(true)
    expect(isPathInside(join(home, 'a'), join(home, 'ab'))).toBe(false)
  })

  it('só aceita http/https', () => {
    expect(assertSafeHttpUrl('https://example.com/captcha')).toContain('https://')
    expect(() => assertSafeHttpUrl('file:///etc/passwd')).toThrow(/http/)
    expect(() => assertSafeHttpUrl('javascript:alert(1)')).toThrow()
  })
})
