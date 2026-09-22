import { derived, writable } from 'svelte/store'
import { en } from './en'
import { pl } from './pl'

export type Locale = 'en' | 'pl'
export type Dict = Record<keyof typeof en, string>
export type Key = keyof Dict

const dictionaries: Record<Locale, Dict> = { en, pl }

// The backend owns the locale (it lives in config.toml and the tray menu needs
// it too). The frontend only mirrors whatever the backend last sent.
export const locale = writable<Locale>('en')

/** `t('popup.snooze', { minutes: 5 })` -> "5 more min" */
export const t = derived(locale, ($locale) => {
  const dict = dictionaries[$locale] ?? en
  return (key: Key, vars?: Record<string, string | number>): string => {
    const template = dict[key] ?? key
    if (!vars) return template
    return template.replace(/\{(\w+)\}/g, (match, name: string) =>
      name in vars ? String(vars[name]) : match,
    )
  }
})
