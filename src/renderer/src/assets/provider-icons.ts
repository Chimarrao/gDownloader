import megaSvg from './provider-icons/mega.svg?raw'
import mediafireSvg from './provider-icons/mediafire.svg?raw'
import googledriveSvg from './provider-icons/googledrive.svg?raw'
import anonfilesSvg from './provider-icons/anonfiles.svg?raw'
import onedriveSvg from './provider-icons/onedrive.svg?raw'
import drimeSvg from './provider-icons/drime.svg?raw'
import rapidgatorSvg from './provider-icons/rapidgator.svg?raw'
import teraboxSvg from './provider-icons/terabox.svg?raw'
import akiraboxSvg from './provider-icons/akirabox.svg?raw'
import brfilesSvg from './provider-icons/brfiles.svg?raw'
import katfileSvg from './provider-icons/katfile.svg?raw'
import pixeldrainSvg from './provider-icons/pixeldrain.svg?raw'
import transferitSvg from './provider-icons/transferit.svg?raw'
import youtubeSvg from './provider-icons/youtube.svg?raw'
import fichierSvg from './provider-icons/1fichier.svg?raw'

export interface ProviderIcon {
  svg: string
  color: string
}

const ICONS: Record<string, ProviderIcon> = {
  mega: {
    color: '#e8352c',
    svg: megaSvg,
  },
  mediafire: {
    color: '#0261CB',
    svg: mediafireSvg,
  },
  fichier: {
    color: '#ef7c14',
    svg: fichierSvg,
  },
  drime: {
    color: '#2ec4b6',
    svg: drimeSvg,
  },
  rapidgator: {
    color: '#23a2dc',
    svg: rapidgatorSvg,
  },
  brupload: {
    color: '#f97316',
    svg: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 36 36" fill="none">
      <rect width="36" height="36" rx="8" fill="#f97316"/>
      <path d="M12 22.5h12M18 10.5v12M13.5 18l4.5 4.5L22.5 18" stroke="white" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"/>
      <path d="M11 26h14" stroke="white" stroke-width="2.2" stroke-linecap="round"/>
    </svg>`,
  },
  googledrive: {
    color: '#4285F4',
    svg: googledriveSvg,
  },
  onedrive: {
    color: '#0a66d9',
    svg: onedriveSvg,
  },
  terabox: {
    color: '#0ea5e9',
    svg: teraboxSvg,
  },
  anonfiles: {
    color: '#00d4ff',
    svg: anonfilesSvg,
  },
  pixeldrain: {
    color: '#ff7b00',
    svg: pixeldrainSvg,
  },
  gofile: {
    color: '#7b5cff',
    svg: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 36 36" fill="none">
      <rect width="36" height="36" rx="8" fill="#1a1d29"/>
      <path d="M18 9c-4.97 0-9 4.03-9 9s4.03 9 9 9c3.53 0 6.59-2.03 8.06-4.99H18v-3.6h9.6C27.86 17.9 28 18.94 28 20" stroke="#7b5cff" stroke-width="2.4" stroke-linecap="round"/>
    </svg>`,
  },
  transferit: {
    color: '#1D81FF',
    svg: transferitSvg,
  },
  youtube: {
    color: '#FF0000',
    svg: youtubeSvg,
  },
  brfiles: {
    color: '#22c55e',
    svg: brfilesSvg,
  },
  moondl: {
    color: '#64748b',
    svg: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 36 36" fill="none">
      <rect width="36" height="36" rx="8" fill="#64748b"/>
      <path d="M10 22C10 16.477 14.477 12 20 12C23.732 12 26.986 14.043 28.715 17.074C27.43 16.338 25.94 15.917 24.353 15.917C19.499 15.917 15.564 19.852 15.564 24.706C15.564 25.338 15.63 25.954 15.756 26.549C12.355 24.994 10 21.557 10 22Z" fill="white"/>
      <circle cx="23.5" cy="23.5" r="5.5" fill="#e2e8f0"/>
    </svg>`,
  },
  akirabox: {
    color: '#0f172a',
    svg: akiraboxSvg,
  },
  katfile: {
    color: '#2563eb',
    svg: katfileSvg,
  },
  default: {
    color: '#7c6fff',
    svg: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" fill="none">
      <circle cx="16" cy="16" r="14" fill="#7c6fff" opacity="0.15"/>
      <path d="M16 9 L16 19 M12 16 L16 21 L20 16" stroke="#7c6fff" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"/>
      <path d="M10 23 H22" stroke="#7c6fff" stroke-width="2" stroke-linecap="round"/>
    </svg>`
  }
}

function normalizeProviderKey(moduleId: string): string {
  const raw = moduleId.toLowerCase().replace(/[^a-z]/g, '')
  const aliases: Record<string, string> = {
    googledrive: 'googledrive',
    googledrivecom: 'googledrive',
    gdrive: 'googledrive',
    drivegooglecom: 'googledrive',
    onedrive: 'onedrive',
    terabox: 'terabox',
    rapidgator: 'rapidgator',
    brupload: 'brupload',
    brfiles: 'brfiles',
    moondl: 'moondl',
    akirabox: 'akirabox',
    katfile: 'katfile',
    mediafire: 'mediafire',
    mega: 'mega',
    pixeldrain: 'pixeldrain',
    gofile: 'gofile',
    transferit: 'transferit',
    transferitt: 'transferit',
    youtube: 'youtube',
    youtu: 'youtube',
    youtubemusic: 'youtube',
    drime: 'drime',
    fichier: 'fichier',
  }
  return aliases[raw] ?? raw
}

export function getProviderIcon(moduleId: string): ProviderIcon {
  const key = normalizeProviderKey(moduleId)
  return ICONS[key] ?? ICONS.default
}

// Existe um ícone de marca específico para este provedor? (false para Direct HTTP e
// desconhecidos, onde preferimos mostrar um ícone por formato de arquivo.)
export function hasProviderIcon(moduleId: string): boolean {
  return normalizeProviderKey(moduleId) in ICONS
}

export function getProviderColor(moduleId: string): string {
  return getProviderIcon(moduleId).color
}

export { ICONS, normalizeProviderKey }
