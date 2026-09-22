import { readable } from 'svelte/store'
import { listen } from '@tauri-apps/api/event'
import { locale, type Locale } from './i18n'

export type StateKind =
  | 'working'
  | 'warning'
  | 'prompt'
  | 'snoozed'
  | 'break'
  | 'suspended'
  | 'paused'

export type Mode = 'work' | 'gaming' | 'chill'
export type EscapeMethod = 'hold' | 'type'

/**
 * What the backend pushes to every window.
 *
 * Deliberately a view model, not a mirror of the Rust `State` enum: the engine
 * can change shape without dragging the UI along, and the UI never has to know
 * what a deadline timestamp is. Everything here is already resolved for
 * rendering.
 *
 * Named `uiState` rather than `state` on purpose — `$state` is a Svelte 5 rune,
 * so a store by that name would collide inside components.
 */
export interface UiState {
  kind: StateKind
  mode: Mode
  locale: Locale
  /** Seconds until the next transition, or null when nothing is counting down. */
  secondsLeft: number | null
  snoozesLeft: number
  /** Length of the snooze the user would get by asking now. */
  nextSnoozeMinutes: number | null
  matchExtensionAvailable: boolean
  matchExtensionMinutes: number
  /** Break progress. Only meaningful while `kind === 'break'`. */
  earnedSeconds: number
  requiredSeconds: number
  /** True while the break counter is held because there is input. */
  counterHeld: boolean
  escapeMethod: EscapeMethod
  escapeHoldSeconds: number
}

export const initialState: UiState = {
  kind: 'working',
  mode: 'work',
  locale: 'en',
  secondsLeft: null,
  snoozesLeft: 0,
  nextSnoozeMinutes: null,
  matchExtensionAvailable: false,
  matchExtensionMinutes: 0,
  earnedSeconds: 0,
  requiredSeconds: 0,
  counterHeld: false,
  escapeMethod: 'hold',
  escapeHoldSeconds: 5,
}

export const STATE_EVENT = 'unsit://state'

/** Live backend state. Needs the Tauri runtime — in a plain browser it stays at the initial value. */
export const uiState = readable<UiState>(initialState, (set) => {
  const pending = listen<UiState>(STATE_EVENT, (event) => {
    set(event.payload)
    locale.set(event.payload.locale)
  })
  return () => {
    void pending.then((unlisten) => unlisten()).catch(() => {})
  }
})

/** Seconds as mm:ss. */
export function formatDuration(totalSeconds: number): string {
  const safe = Math.max(0, Math.floor(totalSeconds))
  const minutes = Math.floor(safe / 60)
  const seconds = safe % 60
  return `${minutes}:${String(seconds).padStart(2, '0')}`
}
