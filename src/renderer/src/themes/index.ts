/**
 * Sistema de temas visuais.
 * 5 temas aplicados via CSS class no root.
 * Persiste no localStorage.
 */

import { ref, watch, computed, type ComputedRef } from 'vue';

export type ThemeId = 'dark-purple' | 'dark-monokai' | 'dark-default' | 'light' | 'system';

export interface ThemeOption {
  id: ThemeId;
  label: string;
  icon: string;
}

export const THEME_OPTIONS: ThemeOption[] = [
  { id: 'dark-purple', label: 'Dark Purple', icon: 'pi pi-moon' },
  { id: 'dark-monokai', label: 'Dark Monokai', icon: 'pi pi-code' },
  { id: 'dark-default', label: 'Dark Default', icon: 'pi pi-moon' },
  { id: 'light', label: 'Light', icon: 'pi pi-sun' },
  { id: 'system', label: 'System', icon: 'pi pi-desktop' }
];

const STORAGE_KEY = 'ytdlp-gui-theme';

const currentTheme = ref<ThemeId>(
  (localStorage.getItem(STORAGE_KEY) as ThemeId) || 'dark-purple'
);

function systemPrefersDark(): boolean {
  return window.matchMedia('(prefers-color-scheme: dark)').matches;
}

const effectiveTheme = computed<ThemeId>(() => {
  if (currentTheme.value === 'system') {
    return systemPrefersDark() ? 'dark-default' : 'light';
  }
  return currentTheme.value;
});

function applyTheme(themeId: ThemeId): void {
  const root = document.documentElement;

  THEME_OPTIONS.forEach((t) => root.classList.remove(`theme-${t.id}`));
  root.classList.remove('theme-light', 'theme-dark-default', 'theme-dark-purple', 'theme-dark-monokai');

  const resolved = themeId === 'system'
    ? (systemPrefersDark() ? 'dark-default' : 'light')
    : themeId;

  root.classList.add(`theme-${resolved}`);
}

export interface UseThemeResult {
  currentTheme: typeof currentTheme
  effectiveTheme: ComputedRef<ThemeId>
  setTheme: (themeId: ThemeId) => void
  initTheme: () => void
  themeOptions: ThemeOption[]
}

export function useTheme(): UseThemeResult {
  function setTheme(themeId: ThemeId): void {
    currentTheme.value = themeId;
    localStorage.setItem(STORAGE_KEY, themeId);
    applyTheme(themeId);
  }

  function initTheme(): void {
    applyTheme(currentTheme.value);

    window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => {
      if (currentTheme.value === 'system') {
        applyTheme('system');
      }
    });
  }

  watch(currentTheme, (newTheme) => {
    applyTheme(newTheme);
  });

  return {
    currentTheme,
    effectiveTheme,
    setTheme,
    initTheme,
    themeOptions: THEME_OPTIONS
  };
}
