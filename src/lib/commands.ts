import { invoke } from '@tauri-apps/api/core'

// Every one of these is a request, not a state change. The backend decides what
// actually happens and answers with a state event.

export const acceptBreak = () => invoke<void>('accept_break')
export const snooze = () => invoke<void>('snooze')
export const extendForMatch = () => invoke<void>('extend_for_match')
export const escapeBreak = () => invoke<void>('escape_break')
export const togglePause = () => invoke<void>('toggle_pause')
export const setLocale = (locale: string) => invoke<void>('set_locale', { locale })
