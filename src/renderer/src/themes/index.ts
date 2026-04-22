/**
 * Sistema de temas visuais.
 * O estado persistido fica nas settings do app; aqui só aplicamos a classe.
 */

import { ref, watch, computed, type ComputedRef } from 'vue';
import type { AppSettingsSnapshot } from '../../../shared/types';

export type ThemeId = 'dark-purple' | 'dark-monokai' | 'dark-default' | 'light' | 'system';

export interface ThemeOption {
  id: ThemeId;
  label: string;
  icon: string;
}

export const THEME_OPTIONS: ThemeOption[] = [
  { id: 'light', label: 'Light', icon: 'pi pi-sun' },
  { id: 'dark-purple', label: 'Dark Purple', icon: 'pi pi-moon' },
  { id: 'dark-monokai', label: 'Dark Monokai', icon: 'pi pi-code' },
  { id: 'dark-default', label: 'Dark Default', icon: 'pi pi-moon' },
  { id: 'system', label: 'System', icon: 'pi pi-desktop' }
];

const currentTheme = ref<ThemeId>('light');
let systemThemeListenerInstalled = false

function rootElement(): HTMLElement | null {
  return typeof document !== 'undefined' ? document.documentElement : null
}

function systemPrefersDark(): boolean {
  return typeof window !== 'undefined' && window.matchMedia('(prefers-color-scheme: dark)').matches;
}

const effectiveTheme = computed<ThemeId>(() => {
  if (currentTheme.value === 'system') {
    return systemPrefersDark() ? 'dark-default' : 'light';
  }
  return currentTheme.value;
});

function applyTheme(themeId: ThemeId): void {
  const root = rootElement()
  if (!root) return

  THEME_OPTIONS.forEach((t) => root.classList.remove(`theme-${t.id}`));
  root.classList.remove('theme-light', 'theme-dark-default', 'theme-dark-purple', 'theme-dark-monokai');

  const resolved = themeId === 'system'
    ? (systemPrefersDark() ? 'dark-default' : 'light')
    : themeId;

  root.classList.add(`theme-${resolved}`);
}

function normalizeHexColor(color: string): string {
  const raw = color.trim()
  if (/^#[0-9a-f]{6}$/i.test(raw)) return raw
  if (/^#[0-9a-f]{3}$/i.test(raw)) {
    const [_, r, g, b] = raw
    return `#${r}${r}${g}${g}${b}${b}`
  }
  return '#5b7cff'
}

function mixHex(color: string, target: string, weight: number): string {
  const source = normalizeHexColor(color)
  const other = normalizeHexColor(target)
  const ratio = Math.min(1, Math.max(0, weight))
  const toRgb = (hex: string) => ({
    r: Number.parseInt(hex.slice(1, 3), 16),
    g: Number.parseInt(hex.slice(3, 5), 16),
    b: Number.parseInt(hex.slice(5, 7), 16),
  })
  const a = toRgb(source)
  const b = toRgb(other)
  const channel = (left: number, right: number) =>
    Math.round(left * (1 - ratio) + right * ratio)
      .toString(16)
      .padStart(2, '0')
  return `#${channel(a.r, b.r)}${channel(a.g, b.g)}${channel(a.b, b.b)}`
}

export function applyAccentColor(color?: string): void {
  const root = rootElement()
  if (!root) return

  if (!color) {
    root.style.removeProperty('--accent-color')
    root.style.removeProperty('--accent-light')
    root.style.removeProperty('--accent-gradient')
    return
  }

  const base = normalizeHexColor(color)
  root.style.setProperty('--accent-color', base)
  root.style.setProperty('--accent-light', mixHex(base, '#ffffff', 0.18))
  root.style.setProperty(
    '--accent-gradient',
    `linear-gradient(90deg, ${mixHex(base, '#0f172a', 0.08)} 0%, ${mixHex(base, '#ffffff', 0.18)} 100%)`,
  )
}

export function applyUiPreferences(settings: Pick<AppSettingsSnapshot, 'fontFamily' | 'fontSize' | 'uiZoom' | 'accentColor'>): void {
  const root = rootElement()
  if (!root) return
  root.style.setProperty('--ui-font-family', settings.fontFamily?.trim() || 'Inter')
  root.style.setProperty('--ui-font-size', `${Math.max(12, Number(settings.fontSize) || 14)}px`)
  root.style.setProperty('--ui-zoom', String(Math.min(1.5, Math.max(0.8, Number(settings.uiZoom) || 1))))
  applyAccentColor(settings.accentColor)
}

export interface UseThemeResult {
  currentTheme: typeof currentTheme
  effectiveTheme: ComputedRef<ThemeId>
  setTheme: (themeId: ThemeId) => void
  initTheme: () => void
  disposeTheme: () => void
  themeOptions: ThemeOption[]
}

export function useTheme(): UseThemeResult {
  function setTheme(themeId: ThemeId): void {
    currentTheme.value = themeId;
    applyTheme(themeId);
  }

  function initTheme(): void {
    applyTheme(currentTheme.value);
    if (typeof window === 'undefined' || systemThemeListenerInstalled) {
      return
    }
    const media = window.matchMedia('(prefers-color-scheme: dark)')
    const onChange = () => {
      if (currentTheme.value === 'system') {
        applyTheme('system')
      }
    }
    media.addEventListener('change', onChange)
    systemThemeListenerInstalled = true
  }

  function disposeTheme(): void {
    // initTheme now installs the listener only once globally.
  }

  watch(currentTheme, (newTheme) => {
    applyTheme(newTheme);
  });

  return {
    currentTheme,
    effectiveTheme,
    setTheme,
    initTheme,
    disposeTheme,
    themeOptions: THEME_OPTIONS
  };
}
