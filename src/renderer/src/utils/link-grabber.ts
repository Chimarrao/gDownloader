export function normalizeUrlCandidate(line: string): string {
  const trimmed = line.trim()
  if (!trimmed) return ''
  const match = trimmed.match(/https?:\/\/\S+/i)
  if (!match) return ''
  try {
    const parsed = new URL(match[0].replace(/[),.;]+$/, ''))
    const isMega = /(^|\.)mega\.(nz|co\.nz)$/i.test(parsed.hostname)
    if (!isMega) {
      parsed.hash = ''
    }
    if (parsed.pathname !== '/' && parsed.pathname.endsWith('/')) {
      parsed.pathname = parsed.pathname.replace(/\/+$/, '')
    }
    return parsed.toString()
  } catch {
    return match[0].replace(/[),.;]+$/, '')
  }
}

export function parseUrls(text: string): string[] {
  const seen = new Set<string>()
  const matches = text.match(/https?:\/\/\S+/gi) ?? []
  for (const match of matches) {
    const url = normalizeUrlCandidate(match)
    if (url) seen.add(url)
  }
  return [...seen]
}

export function truncateUrl(url: string): string {
  try {
    const parsed = new URL(url)
    const last = parsed.pathname.split('/').filter(Boolean).at(-1) || parsed.hostname
    return last.length > 44 ? `${last.slice(0, 41)}...` : last
  } catch {
    return url.length > 50 ? `${url.slice(0, 47)}...` : url
  }
}
