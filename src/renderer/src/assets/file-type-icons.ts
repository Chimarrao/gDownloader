// Ícones de aplicativo por formato de arquivo, para quando o provedor não tem um
// SVG de marca (ex.: download HTTP direto). MKV → VLC, RAR → WinRAR, etc.
//
// Observação: os SVGs coloridos do icons8 são um formato PAGO (a API responde
// PAID_FORMAT), então não dá para baixá-los em massa. Para adicionar/atualizar um
// ícone, copie o SVG no icons8 (botão "copiar") e cole em APP_ICONS abaixo, depois
// mapeie as extensões em EXT_TO_APP. O do VLC veio direto do icons8.

export interface FileTypeIcon {
  app: string
  svg: string
}

// Cada chave é um "app"/família de ícone; o valor é o SVG colorido (48x48).
const APP_ICONS: Record<string, string> = {
  // VLC (vídeo) — SVG oficial do icons8.
  vlc: `<svg xmlns="http://www.w3.org/2000/svg" x="0px" y="0px" width="100" height="100" viewBox="0 0 48 48"><path fill="#F57C00" d="M36.258,28.837c0,0-0.11-0.837-1.257-0.837c-0.216,0-2.392,0-3.719,0c0.798,2.671,1.497,5.135,1.497,5.279c0,2.387-3.401,3.393-8.917,3.393c-5.515,0-8.651-0.94-8.651-3.326c0-0.167,0.998-2.692,1.791-5.346c-1.591,0-3.863,0-4.063,0c-0.806,0-0.937,0.749-0.937,0.749L8.159,40.986L8.815,42h30.652l0.376-1.014L36.258,28.837z"></path><path fill="#E0E0E0" d="M24.001,6c-1.029,0-1.864,0.179-1.864,0.398c-0.492,1.483-8.122,26.143-8.122,26.774c0,2.388,4.471,3.827,9.985,3.827s9.986-1.439,9.986-3.827c0-0.549-7.614-25.268-8.122-26.774C25.865,6.179,25.031,6,24.001,6L24.001,6z"></path><path fill="#FF9800" d="M33.196 30.447C32.032 32.232 28.341 34 24.046 34c-4.34 0-8.156-1.696-9.281-3.51-.499 1.483-.892 2.647-.892 3.28 0 2.386 4.533 4.229 10.128 4.229 5.595 0 10.131-1.844 10.131-4.229C34.132 33.222 33.713 31.955 33.196 30.447zM31.387 24.314l-2.074-6.794c0 0-1.857 1.479-5.311 1.479-3.453 0-5.316-1.479-5.316-1.479l-2.081 6.806c0 0 2.068 2.674 7.397 2.674C29.375 27 31.387 24.314 31.387 24.314zM27.241 10.809l-1.376-4.41c0 0-.083-.398-1.864-.398-1.844 0-1.864.398-1.864.398l-1.376 4.407c0 0 .885 1.194 3.239 1.194C26.355 12 27.241 10.809 27.241 10.809z"></path></svg>`,

  // WinRAR (arquivos .rar) — pilha de livros amarrada, nas cores do WinRAR.
  winrar: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48"><path fill="#5c6bc0" d="M10 12h20l6 6v6H10z"/><path fill="#3949ab" d="M10 22h26v16H10z"/><path fill="#ffca28" d="M20 8h8v34h-8z"/><path fill="#f57f17" d="M22 18h4v6h-4zM22 4h4v4h-4z"/><path fill="#fff59d" d="M23 19h2v4h-2z"/></svg>`,

  // 7-Zip / arquivos genéricos (zip, 7z, tar, gz...).
  archive: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48"><path fill="#8d6e63" d="M8 10h32v28H8z"/><path fill="#a1887f" d="M8 10h32v8H8z"/><path fill="#ffe082" d="M21 8h6v10h-6z"/><path fill="#f9a825" d="M22 20h4v4h-4zM22 26h4v4h-4z"/><path fill="#5d4037" d="M8 18h32v2H8z"/></svg>`,

  // PDF (Adobe Acrobat).
  pdf: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48"><path fill="#e53935" d="M12 4h18l8 8v32H12z"/><path fill="#ffcdd2" d="M30 4l8 8h-8z"/><text x="24" y="34" font-family="Arial, sans-serif" font-size="11" font-weight="700" fill="#fff" text-anchor="middle">PDF</text></svg>`,

  word: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48"><path fill="#1565c0" d="M12 4h18l8 8v32H12z"/><path fill="#bbdefb" d="M30 4l8 8h-8z"/><text x="24" y="34" font-family="Arial, sans-serif" font-size="14" font-weight="700" fill="#fff" text-anchor="middle">W</text></svg>`,

  excel: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48"><path fill="#2e7d32" d="M12 4h18l8 8v32H12z"/><path fill="#c8e6c9" d="M30 4l8 8h-8z"/><text x="24" y="34" font-family="Arial, sans-serif" font-size="14" font-weight="700" fill="#fff" text-anchor="middle">X</text></svg>`,

  powerpoint: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48"><path fill="#d84315" d="M12 4h18l8 8v32H12z"/><path fill="#ffccbc" d="M30 4l8 8h-8z"/><text x="24" y="34" font-family="Arial, sans-serif" font-size="14" font-weight="700" fill="#fff" text-anchor="middle">P</text></svg>`,

  audio: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48"><circle cx="24" cy="24" r="20" fill="#7e57c2"/><path fill="#fff" d="M30 12l-12 3v14.2A5 5 0 1 0 21 34V21l7-1.75V26a5 5 0 1 0 2-4z"/></svg>`,

  image: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48"><rect x="6" y="8" width="36" height="32" rx="3" fill="#26a69a"/><circle cx="17" cy="19" r="4" fill="#fff59d"/><path fill="#b2dfdb" d="M10 36l9-11 6 7 5-6 8 10z"/></svg>`,

  disk: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48"><circle cx="24" cy="24" r="19" fill="#546e7a"/><circle cx="24" cy="24" r="6" fill="#eceff1"/><circle cx="24" cy="24" r="2.2" fill="#90a4ae"/><path fill="#78909c" d="M24 5a19 19 0 0 1 13.4 5.6l-4.2 4.2A13 13 0 0 0 24 11z"/></svg>`,

  windows: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48"><path fill="#03a9f4" d="M6 8h17v17H6z"/><path fill="#4fc3f7" d="M25 8h17v17H25z"/><path fill="#039be5" d="M6 27h17v17H6z"/><path fill="#29b6f6" d="M25 27h17v17H25z"/></svg>`,

  android: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48"><path fill="#7cb342" d="M12 20h24v14a3 3 0 0 1-3 3H15a3 3 0 0 1-3-3z"/><rect x="6" y="20" width="4" height="12" rx="2" fill="#7cb342"/><rect x="38" y="20" width="4" height="12" rx="2" fill="#7cb342"/><rect x="16" y="36" width="4" height="8" rx="2" fill="#7cb342"/><rect x="28" y="36" width="4" height="8" rx="2" fill="#7cb342"/><path fill="#aed581" d="M12 19a12 12 0 0 1 24 0z"/><circle cx="18" cy="14" r="1.6" fill="#33691e"/><circle cx="30" cy="14" r="1.6" fill="#33691e"/></svg>`,

  code: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48"><rect x="6" y="8" width="36" height="32" rx="4" fill="#37474f"/><path fill="none" stroke="#4dd0e1" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" d="M18 19l-5 5 5 5M30 19l5 5-5 5M26 17l-4 14"/></svg>`,

  text: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48"><path fill="#eceff1" d="M12 4h18l8 8v32H12z"/><path fill="#cfd8dc" d="M30 4l8 8h-8z"/><path stroke="#90a4ae" stroke-width="2" stroke-linecap="round" d="M17 22h14M17 27h14M17 32h9"/></svg>`,

  subtitle: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48"><rect x="5" y="10" width="38" height="28" rx="4" fill="#455a64"/><rect x="11" y="27" width="12" height="4" rx="2" fill="#fff176"/><rect x="26" y="27" width="11" height="4" rx="2" fill="#fff176"/></svg>`,
}

// Extensão → chave de app. Cobre os formatos mais comuns de download.
const EXT_TO_APP: Record<string, string> = {
  // Vídeo → VLC
  mkv: 'vlc', mp4: 'vlc', avi: 'vlc', mov: 'vlc', wmv: 'vlc', m4v: 'vlc', flv: 'vlc',
  webm: 'vlc', mpg: 'vlc', mpeg: 'vlc', vob: 'vlc', ogv: 'vlc', '3gp': 'vlc', ts: 'vlc',
  mts: 'vlc', m2ts: 'vlc', rmvb: 'vlc', rm: 'vlc', asf: 'vlc', divx: 'vlc',
  // Arquivos
  rar: 'winrar', r00: 'winrar', r01: 'winrar',
  zip: 'archive', '7z': 'archive', tar: 'archive', gz: 'archive', tgz: 'archive',
  bz2: 'archive', xz: 'archive', zst: 'archive', lz: 'archive', cab: 'archive', arj: 'archive',
  // Documentos office
  pdf: 'pdf',
  doc: 'word', docx: 'word', odt: 'word', rtf: 'word',
  xls: 'excel', xlsx: 'excel', ods: 'excel', csv: 'excel',
  ppt: 'powerpoint', pptx: 'powerpoint', odp: 'powerpoint',
  // Mídia
  mp3: 'audio', flac: 'audio', aac: 'audio', ogg: 'audio', wav: 'audio', opus: 'audio',
  m4a: 'audio', wma: 'audio', aiff: 'audio', alac: 'audio', mid: 'audio', midi: 'audio',
  jpg: 'image', jpeg: 'image', png: 'image', gif: 'image', webp: 'image', bmp: 'image',
  tif: 'image', tiff: 'image', heic: 'image', heif: 'image', avif: 'image', svg: 'image',
  psd: 'image', ai: 'image', ico: 'image', raw: 'image',
  // Discos / instaladores
  iso: 'disk', img: 'disk', dmg: 'disk', vhd: 'disk', vhdx: 'disk', vmdk: 'disk',
  exe: 'windows', msi: 'windows', bat: 'windows', cmd: 'windows',
  apk: 'android', xapk: 'android', aab: 'android',
  // Legendas / texto / código
  srt: 'subtitle', vtt: 'subtitle', ass: 'subtitle', ssa: 'subtitle', sub: 'subtitle',
  txt: 'text', md: 'text', log: 'text', nfo: 'text', ini: 'text', cfg: 'text',
  json: 'code', xml: 'code', js: 'code', py: 'code', rs: 'code', html: 'code',
  css: 'code', sh: 'code', c: 'code', cpp: 'code', java: 'code', go: 'code', php: 'code',
}

function extensionOf(filename: string): string {
  const clean = filename.split('?')[0].split('#')[0].trim()
  const dot = clean.lastIndexOf('.')
  if (dot < 0 || dot === clean.length - 1) return ''
  return clean.slice(dot + 1).toLowerCase()
}

// Retorna o ícone de app para o formato do arquivo, ou null se não houver mapeamento.
export function getFileTypeAppIcon(filename: string): FileTypeIcon | null {
  const app = EXT_TO_APP[extensionOf(filename)]
  if (!app) return null
  const svg = APP_ICONS[app]
  if (!svg) return null
  return { app, svg }
}
