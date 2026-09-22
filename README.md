# Przerwnik

A lightweight Windows tray app that makes you take breaks — and actually knows whether you are working or gaming.

*(Working name, from Polish "przerwa" — a break.)*

> **Status: planning.** There is no working code yet, just a design and a roadmap. Nothing here is installable.

## Why another break reminder

I sit at the computer too long: three hours straight while working, five while gaming. Existing reminders are either dismissed with one click, or so strict that you disable them within a week. None of them know whether you are in a code editor or in the middle of a ranked match, so they behave identically in both.

|                          | Stretchly, Workrave, BreakTimer, EyeLeo | Przerwnik                                  |
| ------------------------ | --------------------------------------- | ------------------------------------------ |
| Context-aware modes      | one schedule, manual pause              | profiles with automatic switching          |
| Behaviour in games       | unaware                                 | detects the game, "finish the match", session cap |
| When a break counts      | when the timer runs out                 | only while there is no input                |
| Skipping a break         | one click, or not at all (strict mode)  | friction: hold a button or retype a sentence |
| Footprint                | Electron (Stretchly, BreakTimer)        | Tauri, a few MB                            |

## Design principles

1. **Friction, not lockout.** There is always a way out, but it costs effort.
2. **Gradual escalation.** A whisper first, a shout later.
3. **Context before schedule.** A match or a call is not the moment for a fullscreen overlay.
4. **Stay light.** It runs in the background while you game, so it cannot eat resources.
5. **Never touch game processes.** No injection, no hooking — anti-cheat safety. Separate windows only.

## How it works

### Modes

|                    | Work    | Gaming                                | Chill   |
| ------------------ | ------- | ------------------------------------- | ------- |
| Interval           | 50 min  | 60 min                                | 90 min  |
| Break length       | 10 min  | 5 min                                 | 5 min   |
| Snoozes            | 2 (5, 3 min) | none — "finish the match" instead | 1 (10 min) |
| "Finish the match" | —       | +15 min, once per cycle               | —       |
| While a game is up | —       | toast and sound only                  | —       |
| Session cap        | —       | forced 15 min break after 3h total    | —       |
| Max escalation     | overlay | overlay                               | popup   |

Chill is for films and series, where an overlay would be overkill.

### Escalation ladder

| Stage           | When                                              | What you see                                        |
| --------------- | ------------------------------------------------- | --------------------------------------------------- |
| 0. Warning      | T − 2 min                                         | a discreet system toast                             |
| 1. Popup        | T                                                 | small bottom-right window: **Start** / **X more min** |
| 2. Overlay      | snoozes exhausted, or popup ignored for N seconds | translucent overlay on **every** monitor, with a countdown |
| 3. Hard mode    | overlay ignored for N seconds (opt-in)            | `LockWorkStation`                                   |

### A break only counts if you actually left

The break countdown advances **only** once there has been no input for at least a few seconds. Touch the mouse and the counter stops, with the overlay saying as much. Conversely, if you are idle for longer than a break while working, the cycle resets on its own and the break is credited — the same goes for waking the machine after a long sleep.

Known gap: gamepads are invisible to the Windows idle API, so controller input does not currently keep the counter paused.

### Context detection

| Context      | Detected via                                                                        | Reaction                     |
| ------------ | ----------------------------------------------------------------------------------- | ---------------------------- |
| Game         | editable process list, `SHQueryUserNotificationState`, fullscreen foreground window   | switch to Gaming             |
| Call         | microphone in use, filtered by an app list                                            | hold escalation at stage 0   |
| Presentation | `QUNS_PRESENTATION_MODE`                                                              | same as a call               |

A running process is not a call — Discord is always running, the microphone is what counts.

## Stack

Tauri 2 and Rust (the `windows` crate for Win32), TypeScript and Vite for the UI. Config is TOML in `%APPDATA%\Przerwnik\config.toml`; stats are append-only JSONL.

The Rust backend owns all state, timing and detection; the frontend only renders.

## Roadmap

**v0.1 — MVP.** Tray with a countdown and menu, state machine with unit tests, Work and Gaming profiles from TOML with manual switching, warning toast, popup with snoozes, multi-monitor overlay that only counts down while you are away, escape hatch, natural breaks, sleep/resume handling, autostart, single instance.

**v0.2 — context.** Game detection and automatic mode switching, "finish the match" and the session cap, call detection, stats (breaks taken, skipped, escape hatches, day streak), a global pause hotkey.

**v0.3 and later.** A settings UI instead of hand-edited TOML, stretch suggestions on the overlay, hard mode, 20-20-20 eye micro-breaks, Chill mode, an updater and a winget manifest, PL/EN localisation.

## Building

Nothing to build yet. Once the project is scaffolded:

```bash
npm install
npm run tauri dev
```

## License

MIT — see [LICENSE](LICENSE).
