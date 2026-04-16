import appPng from './file-icons/app.png'
import archivePng from './file-icons/archive.png'
import audioPng from './file-icons/audio.png'
import codePng from './file-icons/code.png'
import databasePng from './file-icons/database.png'
import diskPng from './file-icons/disk.png'
import docPng from './file-icons/doc.png'
import filePng from './file-icons/file.png'
import folderPng from './file-icons/folder.png'
import imagePng from './file-icons/image.png'
import pdfPng from './file-icons/pdf.png'
import sheetPng from './file-icons/sheet.png'
import slidesPng from './file-icons/slides.png'
import textPng from './file-icons/text.png'
import videoPng from './file-icons/video.png'

export interface FileIconDef {
  src: string
  alt: string
  kind: string
}

const ICONS = {
  folder: { src: folderPng, alt: 'Folder', kind: 'folder' },
  file: { src: filePng, alt: 'File', kind: 'file' },
  video: { src: videoPng, alt: 'Video', kind: 'video' },
  archive: { src: archivePng, alt: 'Archive', kind: 'archive' },
  audio: { src: audioPng, alt: 'Audio', kind: 'audio' },
  image: { src: imagePng, alt: 'Image', kind: 'image' },
  pdf: { src: pdfPng, alt: 'PDF', kind: 'pdf' },
  doc: { src: docPng, alt: 'Document', kind: 'doc' },
  sheet: { src: sheetPng, alt: 'Spreadsheet', kind: 'sheet' },
  slides: { src: slidesPng, alt: 'Presentation', kind: 'slides' },
  text: { src: textPng, alt: 'Text', kind: 'text' },
  code: { src: codePng, alt: 'Code', kind: 'code' },
  disk: { src: diskPng, alt: 'Disk image', kind: 'disk' },
  app: { src: appPng, alt: 'Application', kind: 'app' },
  database: { src: databasePng, alt: 'Database', kind: 'database' },
} satisfies Record<string, FileIconDef>

const EXT_MAP: Record<string, keyof typeof ICONS> = {
  // Video
  mkv: 'video',
  mp4: 'video',
  avi: 'video',
  mov: 'video',
  wmv: 'video',
  m4v: 'video',
  flv: 'video',
  webm: 'video',
  mts: 'video',
  m2ts: 'video',
  m2t: 'video',
  mpg: 'video',
  mpeg: 'video',
  vob: 'video',
  ogv: 'video',
  '3gp': 'video',
  asf: 'video',
  rm: 'video',
  rmvb: 'video',
  // Audio
  mp3: 'audio',
  flac: 'audio',
  aac: 'audio',
  ogg: 'audio',
  wav: 'audio',
  opus: 'audio',
  m4a: 'audio',
  wma: 'audio',
  aiff: 'audio',
  alac: 'audio',
  mid: 'audio',
  midi: 'audio',
  amr: 'audio',
  // Archives
  zip: 'archive',
  rar: 'archive',
  '7z': 'archive',
  tar: 'archive',
  gz: 'archive',
  tgz: 'archive',
  bz2: 'archive',
  xz: 'archive',
  lz: 'archive',
  zst: 'archive',
  cab: 'archive',
  iso: 'disk',
  img: 'disk',
  dmg: 'disk',
  vhd: 'disk',
  vhdx: 'disk',
  vmdk: 'disk',
  qcow2: 'disk',
  toast: 'disk',
  // Images
  jpg: 'image',
  jpeg: 'image',
  png: 'image',
  gif: 'image',
  webp: 'image',
  svg: 'image',
  bmp: 'image',
  tif: 'image',
  tiff: 'image',
  heic: 'image',
  heif: 'image',
  avif: 'image',
  raw: 'image',
  psd: 'image',
  ai: 'image',
  eps: 'image',
  ico: 'image',
  icns: 'image',
  // Documents
  pdf: 'pdf',
  doc: 'doc',
  docx: 'doc',
  odt: 'doc',
  rtf: 'doc',
  pages: 'doc',
  xls: 'sheet',
  xlsx: 'sheet',
  csv: 'sheet',
  ods: 'sheet',
  numbers: 'sheet',
  tsv: 'sheet',
  ppt: 'slides',
  pptx: 'slides',
  odp: 'slides',
  key: 'slides',
  txt: 'text',
  md: 'text',
  markdown: 'text',
  log: 'text',
  nfo: 'text',
  ini: 'text',
  cfg: 'text',
  conf: 'text',
  yaml: 'text',
  yml: 'text',
  toml: 'text',
  // Code / data
  json: 'code',
  xml: 'code',
  sql: 'database',
  sqlite: 'database',
  db: 'database',
  db3: 'database',
  sqlite3: 'database',
  js: 'code',
  mjs: 'code',
  cjs: 'code',
  ts: 'code',
  jsx: 'code',
  tsx: 'code',
  rs: 'code',
  py: 'code',
  rb: 'code',
  php: 'code',
  go: 'code',
  java: 'code',
  kt: 'code',
  swift: 'code',
  c: 'code',
  cc: 'code',
  cpp: 'code',
  h: 'code',
  hpp: 'code',
  cs: 'code',
  sh: 'code',
  bash: 'code',
  zsh: 'code',
  fish: 'code',
  ps1: 'code',
  bat: 'code',
  cmd: 'code',
  html: 'code',
  css: 'code',
  scss: 'code',
  less: 'code',
  vue: 'code',
  svelte: 'code',
  lock: 'code',
  env: 'code',
  // Executables / packages
  exe: 'app',
  msi: 'app',
  apk: 'app',
  ipa: 'app',
  deb: 'app',
  rpm: 'app',
  pkg: 'app',
  appimage: 'app',
  jar: 'app',
  bin: 'app',
  dmgpart: 'app',
  // Subtitles / ebooks / misc
  srt: 'text',
  vtt: 'text',
  ass: 'text',
  epub: 'doc',
  mobi: 'doc',
  azw3: 'doc',
  torrent: 'archive',
  cer: 'file',
  crt: 'file',
  pem: 'file',
  p12: 'file',
  pfx: 'file',
  otf: 'file',
  ttf: 'file',
  woff: 'file',
  woff2: 'file',
}

const MIME_EXACT_MAP: Record<string, keyof typeof ICONS> = {
  'application/pdf': 'pdf',
  'application/zip': 'archive',
  'application/x-rar-compressed': 'archive',
  'application/vnd.rar': 'archive',
  'application/x-7z-compressed': 'archive',
  'application/x-tar': 'archive',
  'application/gzip': 'archive',
  'application/x-bzip2': 'archive',
  'application/x-xz': 'archive',
  'application/json': 'code',
  'application/xml': 'code',
  'application/sql': 'database',
  'application/x-sqlite3': 'database',
  'application/vnd.sqlite3': 'database',
  'application/vnd.ms-excel': 'sheet',
  'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet': 'sheet',
  'application/msword': 'doc',
  'application/vnd.openxmlformats-officedocument.wordprocessingml.document': 'doc',
  'application/vnd.ms-powerpoint': 'slides',
  'application/vnd.openxmlformats-officedocument.presentationml.presentation': 'slides',
  'application/x-iso9660-image': 'disk',
  'application/x-apple-diskimage': 'disk',
  'application/x-msdownload': 'app',
  'application/vnd.android.package-archive': 'app',
}

const MIME_PREFIX_MAP: Array<[string, keyof typeof ICONS]> = [
  ['video/', 'video'],
  ['audio/', 'audio'],
  ['image/', 'image'],
  ['text/', 'text'],
  ['font/', 'file'],
  ['application/vnd.ms-', 'doc'],
  ['application/vnd.openxmlformats-officedocument.wordprocessingml', 'doc'],
  ['application/vnd.openxmlformats-officedocument.spreadsheetml', 'sheet'],
  ['application/vnd.openxmlformats-officedocument.presentationml', 'slides'],
  ['application/x-sharedlib', 'app'],
  ['application/x-executable', 'app'],
]

export function getFileIcon(filename: string, mimeType?: string, isFolder = false): FileIconDef {
  if (isFolder) {
    return ICONS.folder
  }

  const normalizedMime = mimeType?.split(';')[0].trim().toLowerCase()
  if (normalizedMime && MIME_EXACT_MAP[normalizedMime]) {
    return ICONS[MIME_EXACT_MAP[normalizedMime]]
  }

  if (normalizedMime) {
    const match = MIME_PREFIX_MAP.find(([prefix]) => normalizedMime.startsWith(prefix))
    if (match) {
      return ICONS[match[1]]
    }
  }

  const cleanName = filename.split('?')[0].split('#')[0]
  const ext = cleanName.includes('.') ? cleanName.split('.').pop()?.toLowerCase() ?? '' : ''
  return ICONS[EXT_MAP[ext] ?? 'file']
}
