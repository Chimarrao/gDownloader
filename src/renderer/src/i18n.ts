import { computed, ref } from 'vue'

import ptBR from './locales/pt-BR.json'
import enUS from './locales/en-US.json'
import esES from './locales/es-ES.json'
import deDE from './locales/de-DE.json'
import frFR from './locales/fr-FR.json'
import ruRU from './locales/ru-RU.json'
import itIT from './locales/it-IT.json'
import zhCN from './locales/zh-CN.json'
import jaJP from './locales/ja-JP.json'

type LocaleCode = 'pt-BR' | 'en-US' | 'es-ES' | 'de-DE' | 'fr-FR' | 'ru-RU' | 'it-IT' | 'zh-CN' | 'ja-JP'

const locale = ref<LocaleCode>('pt-BR')

const messages = {
  'pt-BR': ptBR,
  'en-US': enUS,
  'es-ES': esES,
  'de-DE': deDE,
  'fr-FR': frFR,
  'ru-RU': ruRU,
  'it-IT': itIT,
  'zh-CN': zhCN,
  'ja-JP': jaJP,
}
type MessageKey = keyof (typeof messages)['pt-BR']

export function setLocale(nextLocale: string | undefined | null): void {
  const supported: LocaleCode[] = ['pt-BR', 'en-US', 'es-ES', 'de-DE', 'fr-FR', 'ru-RU', 'it-IT', 'zh-CN', 'ja-JP']
  locale.value = (supported.includes(nextLocale as LocaleCode) ? nextLocale : 'pt-BR') as LocaleCode
}

export function useI18n() {
  const t = (key: MessageKey): string => messages[locale.value][key]
  const currentLocale = computed(() => locale.value)
  return { t, locale: currentLocale, setLocale }
}
