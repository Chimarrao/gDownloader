import 'file-icon-vectors/dist/file-icon-vivid.css'

export interface FileIconDef {
  className: string
  alt: string
  kind: string
  icon: string
}

const CATALOG = new Set<string>([
  '7z','aac','ai','aiff','amr','apk','app','asf','avi','bash','bat','bin','blank','bmp','bz2',
  'c','cab','cer','cfg','cmd','conf','cpp','crt','cs','css','csv','db','deb','dll','dmg','doc',
  'docx','eot','eps','epub','exe','flac','flv','folder','gif','go','gz','h','heic','html','icns',
  'ico','image','img','ini','iso','jar','java','jpeg','jpg','js','json','key','less','log','lua',
  'm4a','m4v','md','midi','mkv','mov','mp3','mp4','mpeg','mpg','msi','nfo','ods','odt','ogg','otf',
  'pages','pdf','pem','pfx','php','pkg','pl','png','ppt','pptx','ps1','psd','py','rar','rb','rm',
  'rpm','rtf','sass','scss','sh','sql','sqlite','svg','swift','tar','tgz','tif','tiff','torrent',
  'ts','tsv','ttf','txt','vbs','vmdk','vob','vue','wav','webm','webp','woff','woff2','wma','wmv',
  'xls','xlsx','xml','xz','yaml','yml','zip','zsh'
])

const FALLBACK_ICON_BY_KIND: Record<string, string> = {
  folder: 'folder',
  file: 'blank',
  video: 'mp4',
  archive: 'rar',
  audio: 'mp3',
  image: 'image',
  pdf: 'pdf',
  doc: 'docx',
  sheet: 'xlsx',
  slides: 'pptx',
  text: 'txt',
  code: 'json',
  disk: 'iso',
  app: 'exe',
  database: 'sql',
  font: 'ttf',
  subtitle: 'txt'
}

const EXT_KIND_MAP: Record<string, string> = {
  mkv: 'video', mp4: 'video', avi: 'video', mov: 'video', wmv: 'video', m4v: 'video',
  flv: 'video', webm: 'video', mts: 'video', m2ts: 'video', m2t: 'video', mpg: 'video',
  mpeg: 'video', vob: 'video', ogv: 'video', '3gp': 'video', asf: 'video', rm: 'video', rmvb: 'video',
  mp3: 'audio', flac: 'audio', aac: 'audio', ogg: 'audio', wav: 'audio', opus: 'audio',
  m4a: 'audio', wma: 'audio', aiff: 'audio', alac: 'audio', mid: 'audio', midi: 'audio', amr: 'audio',
  zip: 'archive', rar: 'archive', '7z': 'archive', tar: 'archive', gz: 'archive', tgz: 'archive',
  bz2: 'archive', xz: 'archive', lz: 'archive', zst: 'archive', cab: 'archive', torrent: 'archive',
  iso: 'disk', img: 'disk', dmg: 'disk', vhd: 'disk', vhdx: 'disk', vmdk: 'disk', qcow2: 'disk', toast: 'disk',
  jpg: 'image', jpeg: 'image', png: 'image', gif: 'image', webp: 'image', svg: 'image', bmp: 'image',
  tif: 'image', tiff: 'image', heic: 'image', heif: 'image', avif: 'image', raw: 'image', psd: 'image', ai: 'image',
  eps: 'image', ico: 'image', icns: 'image',
  pdf: 'pdf',
  doc: 'doc', docx: 'doc', odt: 'doc', rtf: 'doc', pages: 'doc', epub: 'doc', mobi: 'doc', azw3: 'doc',
  xls: 'sheet', xlsx: 'sheet', csv: 'sheet', ods: 'sheet', numbers: 'sheet', tsv: 'sheet',
  ppt: 'slides', pptx: 'slides', odp: 'slides', key: 'slides',
  txt: 'text', md: 'text', markdown: 'text', log: 'text', nfo: 'text', ini: 'text', cfg: 'text',
  conf: 'text', yaml: 'text', yml: 'text', toml: 'text', srt: 'subtitle', vtt: 'subtitle', ass: 'subtitle', ssa: 'subtitle',
  otf: 'font', ttf: 'font', woff: 'font', woff2: 'font',
  json: 'code', xml: 'code', js: 'code', mjs: 'code', cjs: 'code', ts: 'code', jsx: 'code', tsx: 'code',
  rs: 'code', py: 'code', rb: 'code', php: 'code', go: 'code', java: 'code', kt: 'code', swift: 'code',
  c: 'code', cc: 'code', cpp: 'code', h: 'code', hpp: 'code', cs: 'code', sh: 'code', bash: 'code', zsh: 'code',
  fish: 'code', ps1: 'code', bat: 'code', cmd: 'code', html: 'code', css: 'code', scss: 'code', less: 'code',
  vue: 'code', svelte: 'code', lock: 'code', env: 'code',
  sql: 'database', sqlite: 'database', db: 'database', db3: 'database', sqlite3: 'database',
  exe: 'app', msi: 'app', apk: 'app', ipa: 'app', deb: 'app', rpm: 'app', pkg: 'app', appimage: 'app', jar: 'app', bin: 'app'
}

const MIME_KIND_MAP: Array<[string, string]> = [
  ['application/pdf', 'pdf'],
  ['application/zip', 'archive'],
  ['application/x-rar-compressed', 'archive'],
  ['application/vnd.rar', 'archive'],
  ['application/x-7z-compressed', 'archive'],
  ['application/x-tar', 'archive'],
  ['application/gzip', 'archive'],
  ['application/x-bzip2', 'archive'],
  ['application/x-xz', 'archive'],
  ['application/json', 'code'],
  ['application/xml', 'code'],
  ['application/sql', 'database'],
  ['application/x-sqlite3', 'database'],
  ['application/vnd.sqlite3', 'database'],
  ['application/vnd.ms-excel', 'sheet'],
  ['application/vnd.openxmlformats-officedocument.spreadsheetml.sheet', 'sheet'],
  ['application/msword', 'doc'],
  ['application/vnd.openxmlformats-officedocument.wordprocessingml.document', 'doc'],
  ['application/vnd.ms-powerpoint', 'slides'],
  ['application/vnd.openxmlformats-officedocument.presentationml.presentation', 'slides'],
  ['application/x-iso9660-image', 'disk'],
  ['application/x-apple-diskimage', 'disk'],
  ['application/x-msdownload', 'app'],
  ['application/vnd.android.package-archive', 'app'],
  ['video/', 'video'],
  ['audio/', 'audio'],
  ['image/', 'image'],
  ['text/', 'text'],
  ['font/', 'font']
]

function normalizeExt(filename: string): string {
  const cleanName = filename.split('?')[0].split('#')[0]
  return cleanName.includes('.') ? cleanName.split('.').pop()?.toLowerCase() ?? '' : ''
}

function iconNameFor(filename: string, mimeType?: string, isFolder = false): { icon: string; kind: string } {
  if (isFolder) {
    return { icon: 'folder', kind: 'folder' }
  }

  const ext = normalizeExt(filename)
  if (ext && CATALOG.has(ext)) {
    return { icon: ext, kind: EXT_KIND_MAP[ext] ?? 'file' }
  }

  const normalizedMime = mimeType?.split(';')[0].trim().toLowerCase()
  if (normalizedMime) {
    const mimeMatch = MIME_KIND_MAP.find(([matcher]) =>
      matcher.endsWith('/') ? normalizedMime.startsWith(matcher) : normalizedMime === matcher
    )
    if (mimeMatch) {
      const kind = mimeMatch[1]
      return { icon: FALLBACK_ICON_BY_KIND[kind] ?? 'blank', kind }
    }
  }

  const fallbackKind = EXT_KIND_MAP[ext] ?? 'file'
  return { icon: FALLBACK_ICON_BY_KIND[fallbackKind] ?? 'blank', kind: fallbackKind }
}

export function getFileIcon(filename: string, mimeType?: string, isFolder = false): FileIconDef {
  const { icon, kind } = iconNameFor(filename, mimeType, isFolder)
  return {
    className: `fiv-viv fiv-icon-${icon}`,
    alt: kind,
    kind,
    icon
  }
}
