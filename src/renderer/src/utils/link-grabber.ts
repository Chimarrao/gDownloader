// Casa uma URL com esquema (http/https) OU "crua" — ou seja, sem `http://`:
//  - IP (com porta opcional) + caminho:  151.247.155.169:3000/download/arquivo.mkv
//  - domínio (com porta opcional) + caminho:  exemplo.com/pasta/arquivo
// A exigência de um caminho `/...` (ou porta) na forma crua evita casar nomes de
// arquivo soltos como "video.mkv" no meio de um texto.
const URL_CANDIDATE_RE =
  /(?:https?:\/\/\S+)|(?:(?:\d{1,3}(?:\.\d{1,3}){3}|(?:[a-z0-9-]+\.)+[a-z]{2,})(?::\d+)?\/\S*)/gi

// magnet:?xt=urn:btih:... não bate no padrão acima (sem http/domínio) — casado
// à parte pra virar torrent em vez de link de hoster.
const MAGNET_RE = /magnet:\?xt=urn:btih:[a-z0-9]+(?:&\S*)?/gi

// Pseudo-URI interna usada só no textarea de captura pra representar um
// arquivo .torrent escolhido pelo picker nativo, sem inventar outro fluxo de
// entrada — mesma caixa de texto, mesmo botão "Adicionar" (ver LinkGrabber.vue).
export const TORRENT_FILE_PREFIX = 'torrentfile://'
const TORRENT_FILE_RE = /torrentfile:\/\/\S+/gi

export function isMagnetUri(url: string): boolean {
  return /^magnet:\?xt=urn:btih:/i.test(url)
}

export function isTorrentFileUri(url: string): boolean {
  return url.startsWith(TORRENT_FILE_PREFIX)
}

export function torrentFilePath(url: string): string {
  return decodeURIComponent(url.slice(TORRENT_FILE_PREFIX.length))
}

export function torrentFileUriFromPath(path: string): string {
  return `${TORRENT_FILE_PREFIX}${encodeURIComponent(path)}`
}

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
  for (const match of text.match(MAGNET_RE) ?? []) {
    seen.add(match.trim())
  }
  for (const match of text.match(TORRENT_FILE_RE) ?? []) {
    seen.add(match.trim())
  }
  const rest = text.replace(MAGNET_RE, ' ').replace(TORRENT_FILE_RE, ' ')
  const matches = rest.match(URL_CANDIDATE_RE) ?? []
  for (const match of matches) {
    const url = normalizeUrlCandidate(match)
    if (url) seen.add(url)
  }
  return [...seen]
}

export function truncateUrl(url: string): string {
  if (isMagnetUri(url)) {
    const queryIndex = url.indexOf('?')
    const dn = queryIndex >= 0 ? new URLSearchParams(url.slice(queryIndex + 1)).get('dn') : null
    const name = dn?.trim() || 'Torrent (magnet)'
    return name.length > 44 ? `${name.slice(0, 41)}...` : name
  }
  if (isTorrentFileUri(url)) {
    const path = torrentFilePath(url)
    const name = path.split(/[/\\]/).filter(Boolean).at(-1) || path
    return name.length > 44 ? `${name.slice(0, 41)}...` : name
  }
  try {
    const parsed = new URL(url)
    const last = parsed.pathname.split('/').filter(Boolean).at(-1) || parsed.hostname
    return last.length > 44 ? `${last.slice(0, 41)}...` : last
  } catch {
    return url.length > 50 ? `${url.slice(0, 47)}...` : url
  }
}

// Nome-base de um arquivo para agrupar itens relacionados em um pacote.
//
// Além de multipart (partNN, .rNN, .NNN, cd/disc/vol NN), reconhece episódios
// no formato S01E01. Assim arquivos como "Donos.do.Oeste.S01E01...mkv" e
// "Donos.do.Oeste.S01E08...mkv" entram no mesmo pacote de temporada, sem
// misturar episódios de outra temporada.
export function packageGroupName(filename: string): string {
  let name = filename.replace(/\.[a-z0-9]{1,5}$/i, '')

  const seasonEpisode = name.match(/^(.+?)[._\-\s]+s(\d{1,2})[._\-\s]*e\d{1,3}(?:[._\-\s]|$)/i)
  if (seasonEpisode) {
    const title = seasonEpisode[1]
      .replace(/[._]+/g, ' ')
      .replace(/[\-\s]+/g, ' ')
      .trim()
    const season = Number.parseInt(seasonEpisode[2], 10)
    if (title && Number.isFinite(season)) {
      return `${title} — Temporada ${season}`
    }
  }

  name = name
    .replace(/[._\-\s]*part\s*\d+$/i, '')
    .replace(/[._\-\s]*(?:cd|disc|disco|vol|volume)\s*\d+$/i, '')
    .replace(/\.r\d{2,3}$/i, '')
    .replace(/\.\d{2,3}$/i, '')
    .replace(/[._\-\s]+$/, '')
    .trim()
  return name
}
