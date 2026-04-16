import megaSvg from './provider-icons/mega.svg?raw'
import mediafireSvg from './provider-icons/mediafire.svg?raw'
import googledriveSvg from './provider-icons/googledrive.svg?raw'
import pixeldrainPng from './provider-icons/pixeldrain.png'

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
    svg: `<img src="${pixeldrainPng}" style="width:100%;height:100%;object-fit:contain;" />`,
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
