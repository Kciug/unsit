# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

Unsit is a lightweight Windows tray app that nags you into taking breaks. What separates it from Stretchly/Workrave/BreakTimer: it has context-aware **modes** (Work, Gaming, Chill) with separate break policies, it escalates gradually instead of blocking outright, and a break only counts when you actually left the machine.

## Repo status

Working and in daily use. It lives in the tray, a left click opens a flyout and a right click the menu, modes are editable and can be added or removed, the tick loop drives `engine::step` once a second off real `GetLastInputInfo`, and a single shortcut pulls a waiting prompt to the front or peeks at the countdown. Popups, per-monitor overlays and a transient notice window all draw over the taskbar and over a borderless game; breaks count only while you are away; a finished break waits to be dismissed; leaving early costs a five-second hold. Stats land in `%APPDATA%\Unsit\stats.jsonl`. `npm run check`, `npm run build`, `cargo test` (20) and `cargo clippy` are all clean.

Work is tracked in Sync (project Unsit, key prefix `UNS`), not in this file.

Deliberately absent: the `type` escape method (falls back to holding, so no button promises what it will not do), automatic profile switching on game detection (detection works and already caps escalation mid-game, but switching profiles mid-cycle raises a deadline question worth settling on its own), microphone-based call detection, and renaming a mode — the key is the name, so a rename has to move the active mode and the startup default with it.

**Exclusive fullscreen gets sound and nothing else.** No window of ours can be drawn over it — one placed there alt-tabs the game instead of covering it — and Windows discards notifications while it runs. Discord manages it by injecting into the game process, which this project will not do, so audio is the whole channel. Do not "fix" this with a window.

To watch a full cycle without waiting fifty minutes, set `interval_min = 1` and `break_min = 1` in `%APPDATA%\Unsit\config.toml`.

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

Single test: `cargo test --manifest-path src-tauri/Cargo.toml <test_name>`.

CI runs on every PR and on pushes to main, on `windows-latest` because the `windows` crate builds nowhere else. It is strict, so run these before pushing or it will fail on you:

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml
```

```bash
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

`-D warnings` means dead code fails the build. Code written ahead of the feature that will use it — `Effect::LockScreen`, `Event::SystemResumed`, `Context::Call`, `foreground_covers_monitor` — carries `#[allow(dead_code)]` with a note saying which version needs it. Add the note, not just the attribute.

The Release workflow builds the NSIS installer through `tauri-action` — see **Versioning** below for how to run it.

## Layout

```
src/                  # UI (Svelte 5 + TypeScript, one HTML entry per window)
  lib/i18n/           # en.ts / pl.ts dictionaries, locale store, t()
  lib/state.ts        # UiState view model + the backend state event
  lib/commands.ts     # invoke wrappers
  lib/DurationField.svelte
  settings/ popup/ overlay/ notice/ flyout/
src-tauri/src/
  lib.rs              # Tauri builder, tick loop, commands, effect handling
  engine/             # state machine — pure logic, zero Win32
  platform/           # Win32: idle, fullscreen, monitors, microphone, lock
  ui.rs               # UiState, the view model pushed to the windows
  i18n.rs             # backend-owned strings (toasts, later the tray)
  config.rs           # TOML in %APPDATA%\Unsit\config.toml
  stats.rs            # append-only JSONL events
```

`dispatch` in `lib.rs` is the one turn of the crank: observe context, step the engine, emit the new `UiState`, then run the effects. The lock is held only across the engine call, and effects are run through `run_on_main_thread` — Windows window operations driven from a worker thread are a reliable way to deadlock.

Vite runs with `root: 'src'`, so dev URLs are `/popup/`, `/overlay/`, `/settings/` and match the paths in `tauri.conf.json`. That is also why the Svelte plugin is handed an explicit `configFile` — it would otherwise look for `svelte.config.js` inside `src/`.

`settings` and `popup` are declared in `tauri.conf.json`. Overlay windows are not: there is one per monitor, created at runtime and rebuilt when monitors change.

## Architecture invariants

Breaking any of these breaks the project at its foundation:

- **The Rust backend is the single source of truth.** State machine, clock, idle and context polling, config, stats. The TS frontend only renders: it receives state via Tauri events and sends actions via commands. **No time logic in JS** — the WebView throttles timers in hidden windows, so a JS-side countdown will silently lie.
- **The frontend gets a view model, not the state machine.** `UiState` in `src/lib/state.ts` is built for rendering — seconds already resolved, no deadline timestamps — so the engine can change shape without dragging the UI along. Widen `UiState` deliberately; do not mirror the Rust enum into TypeScript.
- **`engine/` is pure.** Its core is `step(machine, event, now, idle, policy) -> (Machine, Vec<Effect>)` — no Win32, no I/O, no reading the clock inside. Effects are returned as data and performed by the caller. All Win32 lives in `platform/`. The engine never reaches for the world: `now` and `idle` arrive as arguments, which is why the tests can drive a whole day through it a second at a time. Only the observed context comes through a trait, `ContextSource`, because the caller polls it rather than being handed it.
- **`Machine` carries what `State` cannot.** `step` takes a `Machine` (state plus a `Cycle`) rather than a bare `State`, because "the match extension is offered once per cycle" and "three hours in total forces a long break" are bookkeeping that outlives any one state. `Policy` bundles the profile with the idle threshold, which lives in `[general]` because it describes the user rather than the profile.
- **Only a break that happened buys back session time.** `Cycle::session_used` resets on `BreakTaken` and `NaturalBreak` but survives `BreakSkipped` — reset it on a skip and the session cap is defeated by always skipping.
- **A tick gap is a signal, not noise.** A gap between ticks longer than a whole break means the machine slept, and is credited as a natural break. This is why tests advance a second at a time instead of jumping: a jump trips sleep detection.
- **Deadlines are timestamps, never ticking counters.** Otherwise sleep/resume and system clock changes desync the state. Deadlines are recomputed on wake, and a sleep longer than the break length counts as a break taken. `Timestamp` is wall clock on purpose: a monotonic clock that stops during sleep would silently postpone every break by however long the machine was off.
- **The break timer only advances while there is no input** (`GetLastInputInfo`, 5s threshold by default). Mouse movement pauses the counter rather than cancelling the break. Gamepads (XInput) are invisible to `GetLastInputInfo` — a known and deliberately accepted gap.
- **Never touch game processes.** Game detection uses only the process list, `SHQueryUserNotificationState` and foreground-window geometry. No injection, no hooking — anti-cheat safety.
- **Friction, not lockout.** An escape hatch (hold a button / retype a sentence) always exists and always lands in stats as a skipped break. Escalation is gradual — toast, popup, overlay, `LockWorkStation` (opt-in) — capped per profile by `max_escalation`.
- **Autostart never registers from a debug build.** `apply_autostart` returns early under `debug_assertions`, because the entry it would write points into `target/debug` — a path that stops existing the moment that directory is cleaned, leaving a dead startup entry on the user's machine. The setting is still honoured in release builds.
- **The single-instance plugin is registered first.** Two copies would fight over one config file and stack two overlays on every break. A second launch shows the settings window of the copy already running and exits.
- **The backend owns the locale.** It lives in `config.toml` because the tray menu and toasts are built in Rust; the frontend mirrors whatever arrives in the state event. Never set the locale store directly as the source of truth.
- **`config.toml` is written for a human, not for serde.** Durations are integers named by their unit (`interval_min`, `warning_sec`), never `std::time::Duration` — deriving `Serialize` on `Duration` turns every field into a `{ secs, nanos }` table, and this file is hand-edited until the settings UI lands in v0.3. `Profile` exposes `interval()`, `break_length()` and `snooze(used)` so the engine still works in `Duration`.

`Suspended` (call detected) and `Paused` (manual) wrap the previous state in `resume_to` and are reachable from anywhere except `Break`. On exit: if the break is now overdue go to `Prompt`, otherwise restore the prior state.

Five window kinds. `settings` is an ordinary window that hides rather than closes. `popup` is frameless and bottom-right. `notice` is the transient one, top-right, standing in for a Windows notification while a game is running. `overlay` is **one per monitor**, transparent and fullscreen, and is created at startup rather than when a break begins. `flyout` is the tray panel.

**Windows exist before the backend does.** Tauri creates every window declared in `tauri.conf.json` before `setup` runs, hidden or not, and their webviews load and start calling commands straight away. A page that fetches on mount is therefore asking while the shared state is still being assembled — which crashed v0.2.0 outright and left v0.2.1 with an empty settings window. So state is **pushed when a window is shown**, not pulled when it mounts: `show_settings` and `toggle_flyout` both send their payload before revealing the window. Every `state::<Shared>()` access also goes through `shared()`, which uses `try_state` and gives up quietly rather than panicking, naming itself in the log.

Two rules they share. Anything meant to appear over a borderless game must be re-raised with `platform::raise_above_everything` on every show — `always_on_top` alone loses to both the taskbar and a fullscreen game, and this has already been the cause of two bugs. And **only the flyout takes focus**: every other window is focus-free so it can never pull someone out of a game, while the flyout needs focus because losing it is how a tray panel knows to close.

Config is TOML at `%APPDATA%\Unsit\config.toml`; stats are append-only JSONL, with SQLite deferred until queries actually hurt.

## Testing

State logic is unit-tested against a fake clock and fake idle source. Cases that matter: input during a break pauses the counter, idle ≥ break length while working resets the cycle and logs a natural break, exhausted snoozes promote the popup to an overlay with no snooze option, waking after 2h of sleep counts as a break, a call during `Prompt` suspends and returns to `Prompt`, the "finish the match" extension is available exactly once per cycle, and the gaming session limit forces a long break regardless of snoozes.

What cannot be simulated belongs on a manual checklist, not in automated tests: exclusive fullscreen vs borderless, two monitors at different DPI scaling, sleeping mid-break, unplugging a monitor while the overlay is up, and a Teams/Discord call arriving during `Warning` and `Prompt`.

## Versioning

`src-tauri/Cargo.toml` is the single source of truth. `tauri.conf.json` has no `version` field on purpose — Tauri falls back to the crate version, so the installer can never disagree with the tag. `package.json` carries a copy that nothing reads; the release workflow keeps it in step.

Do not bump by hand. Run the **Build** workflow and pick a `bump`:

| `bump` | what happens |
| --- | --- |
| `none` | builds the installer and uploads it as a workflow artifact. Nothing is tagged or published — this is how you test a real build |
| `current` | releases the version already in `Cargo.toml`, tagging this commit |
| `patch` / `minor` / `major` | raises the version in `Cargo.toml` and `package.json`, refreshes `Cargo.lock`, commits, tags `v<version>`, then releases |

Every release is a **draft**, so nothing is public until you say so. Pushing a `v*` tag by hand also builds a release, and then the tag is trusted to match `Cargo.toml`.

The workflow is called Build, not Release, because it only sometimes releases — a name that promises one every run is how you end up wondering where the release went.
