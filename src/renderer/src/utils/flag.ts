/**
 * Classe CSS do flag-icons para um código de país ISO 3166-1 alpha-2.
 * Retorna '' quando o código é inválido/ausente.
 */
export function flagClass(countryCode: string | undefined | null): string {
  if (!countryCode) return ''
  const code = countryCode.trim().toLowerCase()
  if (!/^[a-z]{2}$/.test(code)) return ''
  return `fi fi-${code}`
}
