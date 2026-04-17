import megaSvg from './provider-icons/mega.svg?raw'
import mediafireSvg from './provider-icons/mediafire.svg?raw'
import googledriveSvg from './provider-icons/googledrive.svg?raw'

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
  googledrive: {
    color: '#4285F4',
    svg: googledriveSvg,
  },
  pixeldrain: {
    color: '#ff7b00',
    svg: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 36 36" fill="none">
    <rect width="36" height="36" rx="8" fill="#ff7b00"/>
    <path d="M18 8 C18 8, 10 17, 10 22 C10 26.4 13.6 30 18 30 C22.4 30 26 26.4 26 22 C26 17 18 8 18 8Z" fill="white"/>
    <text x="18" y="24" text-anchor="middle" font-size="10" font-weight="bold" fill="#ff7b00" font-family="sans-serif">P</text>
  </svg>`,
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

export function getProviderIcon(moduleId: string): ProviderIcon {
  const key = moduleId.toLowerCase().replace(/[^a-z]/g, '')
  return ICONS[key] ?? ICONS.default
}

export function getProviderColor(moduleId: string): string {
  return getProviderIcon(moduleId).color
}

export { ICONS }
