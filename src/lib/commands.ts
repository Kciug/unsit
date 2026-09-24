import type { ProfileEdit, SettingsView } from './state'
import { invoke } from '@tauri-apps/api/core'

// Every one of these is a request, not a state change. The backend decides what
// actually happens and answers with a state event.

/**
 * A rejected `invoke` is otherwise completely silent, which makes a genuinely
 * broken button impossible to tell from one the machine chose to ignore.
 */
async function call(command: string, args?: Record<string, unknown>): Promise<void> {
  try {
    await invoke<void>(command, args)
  } catch (error) {
    console.error(`[unsit] ${command} failed`, error)
  }
}

export const acceptBreak = () => call('accept_break')
export const snooze = () => call('snooze')
export const extendForMatch = () => call('extend_for_match')
export const escapeBreak = () => call('escape_break')
export const togglePause = () => call('toggle_pause')
export const setLocale = (locale: string) => call('set_locale', { locale })
export const setAutostart = (enabled: boolean) => call('set_autostart', { enabled })

/** Reads a value back, rather than firing and forgetting like the rest. */
async function read<T>(command: string, args?: Record<string, unknown>): Promise<T | null> {
  try {
    return await invoke<T>(command, args)
  } catch (error) {
    console.error(`[unsit] ${command} failed`, error)
    return null
  }
}

export const getSettings = () => read<SettingsView>('get_settings')

export const setSound = (enabled: boolean) => call('set_sound', { enabled })

export const saveProfile = (profile: string, edit: ProfileEdit) =>
  call('save_profile', { profile, edit })

/** Returns the key that was actually taken, or null if the name was refused. */
export const addProfile = (key: string) => read<string | null>('add_profile', { key })

export const removeProfile = (key: string) => call('remove_profile', { key })
