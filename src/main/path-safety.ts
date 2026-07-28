import { homedir } from 'os'
import { isAbsolute, resolve, sep } from 'path'

/**
 * Helpers de segurança para IPC e janelas auxiliares.
 * Qualquer path/URL vindo do renderer deve passar por aqui antes de tocar no SO.
 */

export function isPathInside(parent: string, child: string): boolean {
  const root = resolve(parent)
  const target = resolve(child)
  if (root === target) return true
  const prefix = root.endsWith(sep) ? root : root + sep
  return target.startsWith(prefix)
}

export function assertSafeFilesystemPath(
  raw: unknown,
  roots: Array<string | null | undefined>,
): string {
  if (typeof raw !== 'string' || !raw.trim()) {
    throw new Error('Caminho inválido')
  }
  if (raw.includes('\0')) {
    throw new Error('Caminho inválido')
  }

  const resolved = resolve(raw)
  const allowed = roots
    .filter((root): root is string => typeof root === 'string' && root.trim().length > 0)
    .map((root) => resolve(root))

  // Sempre permite a home do usuário e o próprio path se for relativo resolvido dentro dela.
  const home = resolve(homedir())
  if (!allowed.some((root) => root === home || isPathInside(home, root))) {
    allowed.push(home)
  }

  const ok = allowed.some((root) => isPathInside(root, resolved))
  if (!ok) {
    throw new Error('Caminho fora das pastas permitidas do usuário')
  }
  return resolved
}

export function assertSafeHttpUrl(raw: unknown): string {
  if (typeof raw !== 'string' || !raw.trim()) {
    throw new Error('URL inválida')
  }
  let parsed: URL
  try {
    parsed = new URL(raw)
  } catch {
    throw new Error('URL inválida')
  }
  if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') {
    throw new Error('Somente http/https são permitidos')
  }
  // Bloqueia javascript:, file:, data: etc. via protocol check acima.
  return parsed.toString()
}

export function isAbsolutePathString(value: string): boolean {
  return isAbsolute(value)
}
