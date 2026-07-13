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

  // WinRAR (.rar) — caixa comprimida com cinta, nas cores do WinRAR.
  winrar: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48"><rect x="8" y="14" width="32" height="26" rx="3" fill="#7986cb"/><rect x="8" y="14" width="32" height="8" rx="3" fill="#5c6bc0"/><rect x="18" y="8" width="12" height="34" rx="2" fill="#ffca28"/><rect x="18" y="8" width="12" height="6" rx="2" fill="#ffb300"/><rect x="22" y="20" width="4" height="7" rx="1" fill="#e65100"/><rect x="23" y="21" width="2" height="5" fill="#fff8e1"/></svg>`,

  // Arquivos compactados genéricos (zip, 7z, tar…) — caixa com zíper.
  archive: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48"><rect x="8" y="10" width="32" height="30" rx="3" fill="#a1887f"/><rect x="8" y="10" width="32" height="9" rx="3" fill="#8d6e63"/><rect x="21" y="6" width="6" height="36" fill="#6d4c41"/><rect x="22" y="9" width="4" height="3" fill="#ffe082"/><rect x="22" y="15" width="4" height="3" fill="#ffe082"/><rect x="22" y="21" width="4" height="3" fill="#ffe082"/><rect x="20" y="28" width="8" height="9" rx="1.5" fill="#ffca28"/><circle cx="24" cy="32" r="1.6" fill="#6d4c41"/></svg>`,

  // Documento PDF.
  pdf: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48"><path fill="#ffebee" d="M13 4h15l8 8v30a2 2 0 0 1-2 2H13a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2z"/><path fill="#ef9a9a" d="M28 4l8 8h-8z"/><rect x="7" y="27" width="30" height="12" rx="2" fill="#e53935"/><text x="22" y="36" font-family="Arial, Helvetica, sans-serif" font-size="9" font-weight="700" fill="#fff" text-anchor="middle">PDF</text></svg>`,

  word: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48"><path fill="#e3f2fd" d="M13 4h15l8 8v30a2 2 0 0 1-2 2H13a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2z"/><path fill="#90caf9" d="M28 4l8 8h-8z"/><rect x="7" y="27" width="30" height="12" rx="2" fill="#1565c0"/><text x="22" y="36" font-family="Arial, Helvetica, sans-serif" font-size="9" font-weight="700" fill="#fff" text-anchor="middle">DOC</text></svg>`,

  excel: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48"><path fill="#e8f5e9" d="M13 4h15l8 8v30a2 2 0 0 1-2 2H13a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2z"/><path fill="#a5d6a7" d="M28 4l8 8h-8z"/><rect x="7" y="27" width="30" height="12" rx="2" fill="#2e7d32"/><text x="22" y="36" font-family="Arial, Helvetica, sans-serif" font-size="9" font-weight="700" fill="#fff" text-anchor="middle">XLS</text></svg>`,

  powerpoint: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48"><path fill="#fbe9e7" d="M13 4h15l8 8v30a2 2 0 0 1-2 2H13a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2z"/><path fill="#ffab91" d="M28 4l8 8h-8z"/><rect x="7" y="27" width="30" height="12" rx="2" fill="#d84315"/><text x="22" y="36" font-family="Arial, Helvetica, sans-serif" font-size="9" font-weight="700" fill="#fff" text-anchor="middle">PPT</text></svg>`,

  audio: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48"><circle cx="24" cy="24" r="19" fill="#7e57c2"/><circle cx="24" cy="24" r="19" fill="none" stroke="#5e35b1" stroke-width="1.5"/><path fill="#fff" d="M31 13.5l-13 3.25v14.4A5.2 5.2 0 1 0 21 35.3V22l7.6-1.9v7.3a5.2 5.2 0 1 0 2.4-4.4z"/></svg>`,

  image: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48"><rect x="6" y="9" width="36" height="30" rx="4" fill="#009688"/><rect x="9" y="12" width="30" height="24" rx="2" fill="#b2dfdb"/><circle cx="18" cy="20" r="3.5" fill="#fff59d"/><path fill="#00897b" d="M12 36l8-9 5 5.5 6-7.5 6 11z"/></svg>`,

  disk: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48"><circle cx="24" cy="24" r="19" fill="#607d8b"/><path fill="#90a4ae" d="M24 5a19 19 0 0 1 15.6 8.2l-6 4.2A11.7 11.7 0 0 0 24 12.3z"/><circle cx="24" cy="24" r="6.5" fill="#eceff1"/><circle cx="24" cy="24" r="2.3" fill="#607d8b"/></svg>`,

  windows: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48"><path fill="#039be5" d="M7 9l15-2.1V22H7z"/><path fill="#4fc3f7" d="M24 6.6L41 4v18H24z"/><path fill="#0288d1" d="M7 26h15v13.1L7 37z"/><path fill="#29b6f6" d="M24 26h17v18l-17-2.4z"/></svg>`,

  android: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48"><path fill="#aed581" d="M14 18a10 10 0 0 1 20 0z"/><path fill="#33691e" d="M18.5 13.2l-2.2-3.4a.7.7 0 0 1 1.2-.75l2.25 3.5a12 12 0 0 1 8.5 0l2.25-3.5a.7.7 0 1 1 1.2.75l-2.2 3.4"/><circle cx="19" cy="15" r="1.4" fill="#fff"/><circle cx="29" cy="15" r="1.4" fill="#fff"/><path fill="#7cb342" d="M13 20h22v13a3 3 0 0 1-3 3H16a3 3 0 0 1-3-3z"/><rect x="6.5" y="20" width="4.5" height="13" rx="2.25" fill="#7cb342"/><rect x="37" y="20" width="4.5" height="13" rx="2.25" fill="#7cb342"/><rect x="16.5" y="35" width="4.5" height="9" rx="2.25" fill="#7cb342"/><rect x="27" y="35" width="4.5" height="9" rx="2.25" fill="#7cb342"/></svg>`,

  code: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48"><rect x="6" y="8" width="36" height="32" rx="5" fill="#37474f"/><rect x="6" y="8" width="36" height="7" rx="5" fill="#455a64"/><circle cx="11" cy="11.5" r="1.2" fill="#ff5f56"/><circle cx="15" cy="11.5" r="1.2" fill="#ffbd2e"/><circle cx="19" cy="11.5" r="1.2" fill="#27c93f"/><path fill="none" stroke="#4dd0e1" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" d="M18 22l-5 5 5 5M30 22l5 5-5 5M27 20l-4 14"/></svg>`,

  text: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48"><path fill="#fafafa" d="M13 4h15l8 8v30a2 2 0 0 1-2 2H13a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2z"/><path fill="#cfd8dc" d="M28 4l8 8h-8z"/><path stroke="#90a4ae" stroke-width="2" stroke-linecap="round" d="M16 21h13M16 26h13M16 31h8"/></svg>`,

  subtitle: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48"><rect x="5" y="11" width="38" height="26" rx="5" fill="#455a64"/><rect x="10" y="26" width="13" height="4" rx="2" fill="#fff176"/><rect x="26" y="26" width="12" height="4" rx="2" fill="#fff176"/><rect x="10" y="19" width="8" height="3.5" rx="1.75" fill="#b0bec5"/><rect x="21" y="19" width="17" height="3.5" rx="1.75" fill="#b0bec5"/></svg>`,
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
