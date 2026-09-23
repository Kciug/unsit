# Unsit

A break reminder for Windows that knows whether you are working or gaming — and only counts a break if you actually got up.

> **Status: it works, but there is no release yet.** Everything described below is built and running. The installer just is not published anywhere permanent.

## Why

I sit at the computer for hours without noticing — three at work, five if I am gaming. Every reminder I tried was either dismissed with one click and forgotten, or strict enough that I disabled it within a week. None of them knew whether I was in a code editor or halfway through a ranked match, so they behaved identically in both, and a fullscreen overlay landing mid-match is the fastest way to get an app uninstalled.

If you have used Stretchly, Workrave or BreakTimer and wished one of them would notice you are mid-game, this is meant to be that.

## What it does

It sits in the tray, counts down to your next break and nudges you when one is due.

**It knows roughly what you are doing.** Three modes — Work, Gaming and Chill — each with its own interval and break length, switched from the tray menu.

**It nags gradually.** A quiet notification a couple of minutes ahead, then a small popup you can snooze, and only then an overlay across every monitor. In Gaming mode, while a game is actually in the foreground, it stays at the notification and waits.

**A break only counts if you actually take it.** The countdown runs only while you are away from the keyboard. Touch the mouse and it stops until you leave again, so sitting through a break does not get you anywhere. Leaving early is possible, but it costs holding a button for five seconds rather than clicking one.

When the break is up, the overlay waits for you instead of clearing itself — so the next interval starts when you are back at the desk, not when the timer ran out.

## Install

There is no release yet. You can build it yourself, or take the installer that the Release workflow leaves on its run under the Actions tab.

Either way the build is unsigned, so Windows SmartScreen will warn the first time: **More info**, then **Run anyway**.

## Building from source

You need Node, Rust with the MSVC toolchain, and Visual Studio Build Tools with the "Desktop development with C++" workload. WebView2 ships with Windows 11.

```bash
npm install
npm run tauri dev
```

`npm run tauri build` produces the installer.

## Known limitations

**Windows only.** Most of the app would port, but the parts that make it interesting — knowing whether you are there, whether a game is running, and drawing over everything — are exactly the parts that differ most between systems, and some of them are not available to an ordinary app on Wayland at all.

**Gamepads do not count as activity.** Windows does not report controller input the way it reports the keyboard and mouse, so a session played entirely on a pad looks like an empty desk. In practice that means a break can be credited while you are in the middle of playing. Known, and not solved yet.

## License

MIT — see [LICENSE](LICENSE).

---

Built mostly for my own use, public in case someone else wants it too.
