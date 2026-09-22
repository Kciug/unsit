# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

Unsit is a lightweight Windows tray app that nags you into taking breaks. What separates it from Stretchly/Workrave/BreakTimer: it has context-aware **modes** (Work, Gaming, Chill) with separate break policies, it escalates gradually instead of blocking outright, and a break only counts when you actually left the machine.

## Repo status

Scaffolded, not yet working. The frontend builds and typechecks clean. The Rust side is a skeleton whose core functions are `todo!()`, and **it has never been compiled** — the toolchain was not installed when it was written, so expect it to need fixing on the first `cargo check`.

The design doc lives at `docs/unsit-plan.md`, which is **gitignored and local-only** (Polish, personal working notes). Do not assume it is present; this file is the self-contained reference. When it is present, record design decisions there rather than creating new docs — it has a dated "Decyzje" log and an open-questions checklist.

Windows-only. Building needs Rust (MSVC toolchain) plus Visual Studio 2022 Build Tools with the C++ workload; WebView2 ships with Windows 11.

## Commands

```bash
npm install
```

```bash
npm run tauri dev
```

`npm run dev` runs Vite alone, which is useful for styling windows but leaves every `invoke` and every state event dead, so the UI sits at its initial state.

```bash
npm run check
```

```bash
cargo test --manifest-path src-tauri/Cargo.toml
```

Single test: `cargo test --manifest-path src-tauri/Cargo.toml <test_name>`. CI should run `cargo test`, `cargo clippy` and `npm run check` on every PR, and build the installer via `tauri-action` on tags.

## Layout

```
src/                  # UI (Svelte 5 + TypeScript, one HTML entry per window)
  lib/i18n/           # en.ts / pl.ts dictionaries, locale store, t()
  lib/state.ts        # UiState view model + the backend state event
  lib/commands.ts     # invoke wrappers
  settings/ popup/ overlay/
src-tauri/src/
  lib.rs              # Tauri builder, commands, windows
  engine/             # state machine — pure logic, zero Win32
  platform/           # Win32: idle, fullscreen, monitors, microphone, lock
  config.rs           # TOML in %APPDATA%\Unsit\config.toml
  stats.rs            # append-only JSONL events
```

Vite runs with `root: 'src'`, so dev URLs are `/popup/`, `/overlay/`, `/settings/` and match the paths in `tauri.conf.json`. That is also why the Svelte plugin is handed an explicit `configFile` — it would otherwise look for `svelte.config.js` inside `src/`.

`settings` and `popup` are declared in `tauri.conf.json`. Overlay windows are not: there is one per monitor, created at runtime and rebuilt when monitors change.

## Architecture invariants

Breaking any of these breaks the project at its foundation:

- **The Rust backend is the single source of truth.** State machine, clock, idle and context polling, config, stats. The TS frontend only renders: it receives state via Tauri events and sends actions via commands. **No time logic in JS** — the WebView throttles timers in hidden windows, so a JS-side countdown will silently lie.
- **The frontend gets a view model, not the state machine.** `UiState` in `src/lib/state.ts` is built for rendering — seconds already resolved, no deadline timestamps — so the engine can change shape without dragging the UI along. Widen `UiState` deliberately; do not mirror the Rust enum into TypeScript.
- **`engine/` is pure.** Its core is `step(state, event, now, idle, profile) -> (State, Vec<Effect>)` — no Win32, no I/O, no reading the clock inside. Effects are returned as data and performed by the caller. All Win32 lives in `platform/`. The engine sees the system only through `Clock`, `IdleSource` and `ContextSource` traits, swapped for fakes in tests.
- **Deadlines are timestamps, never ticking counters.** Otherwise sleep/resume and system clock changes desync the state. Deadlines are recomputed on wake, and a sleep longer than the break length counts as a break taken. `Timestamp` is wall clock on purpose: a monotonic clock that stops during sleep would silently postpone every break by however long the machine was off.
- **The break timer only advances while there is no input** (`GetLastInputInfo`, 5s threshold by default). Mouse movement pauses the counter rather than cancelling the break. Gamepads (XInput) are invisible to `GetLastInputInfo` — a known and deliberately accepted gap.
- **Never touch game processes.** Game detection uses only the process list, `SHQueryUserNotificationState` and foreground-window geometry. No injection, no hooking — anti-cheat safety.
- **Friction, not lockout.** An escape hatch (hold a button / retype a sentence) always exists and always lands in stats as a skipped break. Escalation is gradual — toast, popup, overlay, `LockWorkStation` (opt-in) — capped per profile by `max_escalation`.
- **The backend owns the locale.** It lives in `config.toml` because the tray menu and toasts are built in Rust; the frontend mirrors whatever arrives in the state event. Never set the locale store directly as the source of truth.

`Suspended` (call detected) and `Paused` (manual) wrap the previous state in `resume_to` and are reachable from anywhere except `Break`. On exit: if the break is now overdue go to `Prompt`, otherwise restore the prior state.

Three window kinds with different requirements: `settings` (ordinary), `popup` (frameless, topmost, bottom-right), `overlay` (**one per monitor**, transparent, fullscreen, topmost, off the taskbar). The overlay does not need to steal focus to work, and it cannot paint over an exclusive-fullscreen game — which is why Gaming mode only fires a toast while a game is running.

Config is TOML at `%APPDATA%\Unsit\config.toml`; stats are append-only JSONL, with SQLite deferred until queries actually hurt.

## Testing

State logic is unit-tested against a fake clock and fake idle source. Cases that matter: input during a break pauses the counter, idle ≥ break length while working resets the cycle and logs a natural break, exhausted snoozes promote the popup to an overlay with no snooze option, waking after 2h of sleep counts as a break, a call during `Prompt` suspends and returns to `Prompt`, the "finish the match" extension is available exactly once per cycle, and the gaming session limit forces a long break regardless of snoozes.

What cannot be simulated belongs on a manual checklist, not in automated tests: exclusive fullscreen vs borderless, two monitors at different DPI scaling, sleeping mid-break, unplugging a monitor while the overlay is up, and a Teams/Discord call arriving during `Warning` and `Prompt`.
