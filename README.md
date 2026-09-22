# Przerwnik

A Windows tray app that reminds you to take breaks — and notices whether you are working or gaming.

*(Working name, from the Polish "przerwa" — a break.)*

> **Status: early days.** There is no release yet and nothing to install.

## What it does

It sits in the tray, counts down to your next break and nudges you when one is due. Three things it tries to get right:

**It knows roughly what you are doing.** Separate profiles for working, gaming and watching something, each with its own interval and break length, switched automatically rather than by hand.

**It nags gradually.** A quiet notification first, then a small popup you can snooze, and only then something harder to ignore. It does not open by taking your screen away.

**A break only counts if you actually take it.** The countdown runs only while you are away from the keyboard, so sitting through it does not get you anywhere.

It also stays quiet during a game or a call, and picks up where it left off once you are done.

## Install

Nothing to download yet. Once there is something worth releasing, it will show up under Releases as a normal Windows installer.

## Building from source

Nothing to build yet either. It is a Tauri 2 app, so eventually the usual Rust and Node toolchain will be all you need.

## License

MIT — see [LICENSE](LICENSE).

---

Built mostly for my own use, public in case someone else wants it too.
