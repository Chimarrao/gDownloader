/**
 * Provider SVG icons — inline geometric shapes, color-coded per service.
 */

export interface ProviderIcon {
  svg: string
  color: string
}

const ICONS: Record<string, ProviderIcon> = {
  mega: {
    color: '#e84d3d',
    svg: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" fill="none">
      <circle cx="16" cy="16" r="14" fill="#e84d3d" opacity="0.15"/>
      <path d="M6 11 L16 21 L26 11" stroke="#e84d3d" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" fill="none"/>
      <path d="M6 11 L11 21 M26 11 L21 21" stroke="#e84d3d" stroke-width="2.5" stroke-linecap="round" fill="none"/>
      <path d="M6 11 L16 16 L26 11" stroke="#e84d3d" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" fill="none"/>
    </svg>`
  },
  mediafire: {
    color: '#0062C7',
    svg: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" fill="none">
      <circle cx="16" cy="16" r="14" fill="#0062C7" opacity="0.15"/>
      <path d="M10 22 C10 22 8 14 16 10 C24 6 24 14 22 16 C20 18 18 16 16 18 C14 20 16 24 16 24" stroke="#0062C7" stroke-width="2.2" stroke-linecap="round" fill="none"/>
      <circle cx="16" cy="24" r="1.5" fill="#0062C7"/>
    </svg>`
  },
  googledrive: {
    color: '#4285F4',
    svg: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" fill="none">
      <circle cx="16" cy="16" r="14" fill="#4285F4" opacity="0.12"/>
      <path d="M16 7 L24 22 H8 Z" fill="none" stroke="#4285F4" stroke-width="2" stroke-linejoin="round"/>
      <path d="M8 22 L14 11 M24 22 L18 11" stroke="#FBBC05" stroke-width="2" stroke-linecap="round"/>
      <path d="M8 22 H24" stroke="#34A853" stroke-width="2.2" stroke-linecap="round"/>
    </svg>`
  },
  pixeldrain: {
    color: '#ff6600',
    svg: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" fill="none">
      <circle cx="16" cy="16" r="14" fill="#ff6600" opacity="0.15"/>
      <path d="M16 8 L16 20 M11 15 L16 21 L21 15" stroke="#ff6600" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"/>
      <path d="M9 23 H23" stroke="#ff6600" stroke-width="2.2" stroke-linecap="round"/>
    </svg>`
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
