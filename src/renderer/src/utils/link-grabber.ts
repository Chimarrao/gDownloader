// Casa uma URL com esquema (http/https) OU "crua" — ou seja, sem `http://`:
//  - IP (com porta opcional) + caminho:  151.247.155.169:3000/download/arquivo.mkv
//  - domínio (com porta opcional) + caminho:  exemplo.com/pasta/arquivo
// A exigência de um caminho `/...` (ou porta) na forma crua evita casar nomes de
// arquivo soltos como "video.mkv" no meio de um texto.
const URL_CANDIDATE_RE =
  /(?:https?:\/\/\S+)|(?:(?:\d{1,3}(?:\.\d{1,3}){3}|(?:[a-z0-9-]+\.)+[a-z]{2,})(?::\d+)?\/\S*)/gi

// Garante um esquema: prefixa http:// quando o candidato vem "cru". Usamos http://
// (e não https://) porque servidores diretos por IP:porta costumam ser http; sites
// que exigem https redirecionam sozinhos.
function ensureScheme(candidate: string): string {
  return /^https?:\/\//i.test(candidate) ? candidate : `http://${candidate}`
}

export function normalizeUrlCandidate(line: string): string {
  const trimmed = line.trim()
  if (!trimmed) return ''
  const match = trimmed.match(URL_CANDIDATE_RE)
  if (!match) return ''
  const candidate = ensureScheme(match[0].replace(/[),.;]+$/, ''))
  try {
    const parsed = new URL(candidate)
    const isMega = /(^|\.)mega\.(nz|co\.nz)$/i.test(parsed.hostname)
    if (!isMega) {
      parsed.hash = ''
    }
    if (parsed.pathname !== '/' && parsed.pathname.endsWith('/')) {
      parsed.pathname = parsed.pathname.replace(/\/+$/, '')
    }
    return parsed.toString()
  } catch {
    return candidate.replace(/[),.;]+$/, '')
  }
}

export function parseUrls(text: string): string[] {
  const seen = new Set<string>()
  const matches = text.match(URL_CANDIDATE_RE) ?? []
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
