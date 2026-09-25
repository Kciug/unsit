mod config;
mod engine;
mod i18n;
mod platform;
mod stats;
mod tray;
mod ui;

use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize};
use tauri::{WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_notification::NotificationExt;

use config::{Config, Locale, Profile};
use engine::{Context, Effect, Event, Machine, Policy, State};
use platform::{ContextSource, WindowsContext};
use ui::{
    FlyoutMode, FlyoutView, Notice, ProfileEdit, SettingsView, StateKind, UiState, FLYOUT_EVENT,
    NOTICE_EVENT, SETTINGS_EVENT, STATE_EVENT,
};

const TICK: Duration = Duration::from_secs(1);
/// How long the notice window stays up before taking itself away.
const NOTICE_SECONDS: u64 = 6;
/// Gap between the popup and the corner of the screen, in logical pixels.
const POPUP_MARGIN: f64 = 24.0;

struct Core {
    machine: Machine,
    config: Config,
    profile: String,
    context_source: WindowsContext,
    /// Whether the shortcut was actually claimed. Another app may already own
    /// it, and in a release build the log saying so goes nowhere.
    hotkey_registered: bool,
    /// When the notice window should take itself away. Kept here rather than
    /// in the window, because timing belongs to the backend.
    notice_until: Option<SystemTime>,
}

impl Core {
    fn profile(&self) -> Option<Profile> {
        self.config.profiles.get(&self.profile).cloned()
    }

    fn menu_model(&self) -> tray::MenuModel {
        tray::MenuModel {
            locale: self.config.general.locale,
            profiles: self.config.profiles.keys().cloned().collect(),
            current: self.profile.clone(),
            paused: matches!(self.machine.state, State::Paused { .. }),
        }
    }
}

struct Shared {
    core: Mutex<Core>,
}

/// What the effect handlers need beyond the effect itself.
///
/// Read off the config while the lock is held, then carried out to the main
/// thread — the handlers must never reach back for it.
struct Render {
    profile: String,
    locale: Locale,
    sound: bool,
    /// A borderless game is in front, so a toast would be discarded and our
    /// own window has to stand in for it. False under exclusive fullscreen,
    /// where no window of ours can be drawn at all.
    use_notice: bool,
}

/// Locks the shared state, recovering from a poisoned mutex.
///
/// A panic in one tick should cost that tick, not brick the app. With a plain
/// `lock()`, every later call returns `Err` forever, so the tray keeps its icon
/// and the popup stays on screen while nothing behind them responds any more —
/// the worst possible failure mode for something that is supposed to nag you.
fn lock(shared: &Shared) -> std::sync::MutexGuard<'_, Core> {
    match shared.core.lock() {
        Ok(core) => core,
        Err(poisoned) => {
            eprintln!("[unsit] shared state was poisoned by an earlier panic; recovering");
            poisoned.into_inner()
        }
    }
}

/// The shared state, if setup has got that far.
///
/// `try_state` rather than `state`, which panics outright when the state is not
/// managed yet. Windows exist and the tray is live before setup finishes, so a
/// click or a shortcut landing in that gap would take the whole app down rather
/// than doing nothing — and in a release build with `panic = "abort"` that is a
/// silent disappearance, with only an event-log entry to show for it.
fn shared<'a>(app: &'a AppHandle, who: &str) -> Option<tauri::State<'a, Shared>> {
    let state = app.try_state::<Shared>();
    if state.is_none() {
        eprintln!("[unsit] {who} ran before the state was ready");
    }
    state
}

/// One turn of the machine: observe, step, publish, act.
///
/// The lock is held only for the engine call. Everything that touches a window
/// runs afterwards on the main thread, because Windows window operations driven
/// from a worker are a reliable way to deadlock.
fn dispatch(app: &AppHandle, event: Event) {
    let Some(shared) = shared(app, "dispatch") else {
        return;
    };
    let now = SystemTime::now();
    let idle = platform::idle_duration();

    #[cfg(debug_assertions)]
    let described = format!("{event:?}");
    #[cfg(debug_assertions)]
    let user_driven = !matches!(event, Event::Tick);

    let (effects, snapshot, render, expire_notice) = {
        let mut core = lock(&shared);
        let Some(profile) = core.profile() else {
            return;
        };

        // The notice takes itself away on our clock, not the window's.
        let expire_notice = match core.notice_until {
            Some(until) if now >= until => {
                core.notice_until = None;
                true
            }
            _ => false,
        };

        let policy = Policy {
            profile: &profile,
            idle_threshold: core.config.general.idle_threshold(),
        };

        let mut effects = Vec::new();

        // Feed the world in first, so the engine sees this tick as it really is.
        let observed = core.context_source.current();
        if observed != core.machine.context {
            let (next, mut produced) = engine::step(
                core.machine.clone(),
                Event::ContextChanged(observed),
                now,
                idle,
                &policy,
            );
            core.machine = next;
            effects.append(&mut produced);
        }

        let (next, mut produced) = engine::step(core.machine.clone(), event, now, idle, &policy);
        core.machine = next;
        effects.append(&mut produced);

        let snapshot = UiState::build(
            &core.machine,
            &core.profile,
            &profile,
            &core.config,
            now,
            idle,
        );

        let render = Render {
            profile: core.profile.clone(),
            locale: core.config.general.locale,
            sound: core.config.general.sound,
            use_notice: core.machine.context == Context::Game,
        };

        (effects, snapshot, render, expire_notice)
    };

    // A dead button and a button the machine deliberately ignores look the
    // same from outside, so say which one it was.
    #[cfg(debug_assertions)]
    if user_driven || !effects.is_empty() {
        eprintln!(
            "[unsit] {described} -> {:?}, effects: {effects:?}",
            snapshot.kind
        );
    }

    let locale = snapshot.locale;
    let _ = app.emit(STATE_EVENT, &snapshot);

    let tip = tooltip(&snapshot, &i18n::profile_name(locale, &render.profile));
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        tray::set_tooltip(&handle, &tip);
        if expire_notice {
            hide_notice(&handle);
        }
        for effect in effects {
            apply(&handle, effect, &render);
        }
    });
}

fn apply(app: &AppHandle, effect: Effect, render: &Render) {
    match effect {
        // Over a borderless game a real notification is discarded by Windows
        // before it reaches the screen, so our own window stands in for it.
        Effect::ShowToast if render.use_notice => show_notice(
            app,
            Notice {
                title: i18n::warning_title(render.locale).to_owned(),
                body: Some(i18n::warning_body(render.locale).to_owned()),
            },
        ),
        Effect::ShowToast => show_toast(app, render.locale),
        Effect::ShowPopup => show_popup(app),
        Effect::ShowOverlay => show_overlays(app),
        Effect::HideAll => hide_all(app),
        Effect::PlaySound => {
            if render.sound {
                platform::play_notification_sound();
            }
        }
        Effect::LockScreen => platform::lock_workstation(),
        Effect::Log(stat) => stats::record(stat, &render.profile),
    }
}

/// One line saying where the machine is.
///
/// Shared by the tray tooltip and the peek notice, so the two can never end up
/// describing the same state differently.
fn status(snapshot: &UiState, mode: &str) -> String {
    let locale = snapshot.locale;

    match snapshot.kind {
        StateKind::Break => {
            if snapshot.counter_held {
                i18n::status_held(locale)
            } else {
                let left = snapshot
                    .required_seconds
                    .saturating_sub(snapshot.earned_seconds);
                i18n::status_break(locale, &clock(left))
            }
        }
        StateKind::Done => i18n::status_done(locale),
        StateKind::Paused | StateKind::Suspended => i18n::status_paused(locale),
        StateKind::Prompt => i18n::status_due(locale, mode),
        _ => match snapshot.seconds_left {
            Some(left) => i18n::status_counting(locale, mode, &clock(left)),
            None => i18n::status_due(locale, mode),
        },
    }
}

fn tooltip(snapshot: &UiState, mode: &str) -> String {
    format!("Unsit — {}", status(snapshot, mode))
}

/// Seconds as a clock, growing an hours field only once there is one.
///
/// A ninety-minute interval read "90:00" before this, which is a duration
/// nobody writes down that way.
fn clock(seconds: u64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let seconds = seconds % 60;

    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

fn show_toast(app: &AppHandle, locale: Locale) {
    let _ = app
        .notification()
        .builder()
        .title(i18n::warning_title(locale))
        .body(i18n::warning_body(locale))
        .show();
}

/// Shows the transient notice, top-right and out of the way.
///
/// Top-right rather than bottom-right on purpose: the prompt popup lives in the
/// bottom corner, and a peek pressed while a prompt is waiting would otherwise
/// land on top of it.
fn show_notice(app: &AppHandle, notice: Notice) {
    let Some(window) = app.get_webview_window("notice") else {
        return;
    };

    let _ = app.emit_to("notice", NOTICE_EVENT, Some(notice));

    // Bottom-right, where every other window of ours appears and where Windows
    // puts its own notifications. It was top-right at first to keep clear of
    // the prompt popup, but the two never share the screen — the warning is
    // two minutes earlier, and the peek gives way to focusing a waiting prompt
    // — so the corner was bought at the price of looking like a stray.
    let placement = match (platform::primary_work_area(), window.outer_size()) {
        (Some((_, _, right, bottom)), Ok(size)) => {
            let scale = window.scale_factor().unwrap_or(1.0);
            let margin = (POPUP_MARGIN * scale) as i32;
            let x = right - size.width as i32 - margin;
            let y = bottom - size.height as i32 - margin;
            let _ = window.set_position(PhysicalPosition::new(x, y));
            Some((x, y, size.width as i32, size.height as i32))
        }
        _ => None,
    };

    let _ = window.show();
    if let (Ok(handle), Some((x, y, width, height))) = (window.hwnd(), placement) {
        platform::raise_above_everything(handle.0 as isize, x, y, width, height);
    }

    let Some(shared) = shared(app, "show_notice") else {
        return;
    };
    let mut core = lock(&shared);
    core.notice_until = SystemTime::now().checked_add(Duration::from_secs(NOTICE_SECONDS));
}

/// The tray flyout: what a left click on the icon opens.
///
/// A second click closes it again, which is what every other tray panel on
/// Windows does and what a hand reaches for without thinking.
fn toggle_flyout(app: &AppHandle) {
    let Some(window) = app.get_webview_window("flyout") else {
        return;
    };

    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
        return;
    }

    let view = {
        let Some(shared) = shared(app, "toggle_flyout") else {
            return;
        };
        let core = lock(&shared);
        let locale = core.config.general.locale;

        FlyoutView {
            modes: core
                .config
                .profiles
                .keys()
                .map(|key| FlyoutMode {
                    key: key.clone(),
                    label: i18n::profile_name(locale, key),
                })
                .collect(),
            active: core.profile.clone(),
            paused: matches!(core.machine.state, State::Paused { .. }),
        }
    };

    let _ = app.emit_to("flyout", FLYOUT_EVENT, view);

    // Bottom-right of the work area, which is where the tray is and where
    // Windows puts its own flyouts.
    if let (Some((_, _, right, bottom)), Ok(size)) =
        (platform::primary_work_area(), window.outer_size())
    {
        let scale = window.scale_factor().unwrap_or(1.0);
        let margin = (POPUP_MARGIN * scale) as i32;
        let x = right - size.width as i32 - margin;
        let y = bottom - size.height as i32 - margin;
        let _ = window.set_position(PhysicalPosition::new(x, y));
    }

    let _ = window.show();
    let _ = window.set_always_on_top(true);
    // Focused on purpose, unlike every other window here: losing focus is how
    // it knows to close.
    let _ = window.set_focus();
}

fn hide_flyout(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("flyout") {
        let _ = window.hide();
    }
}

fn hide_notice(app: &AppHandle) {
    let _ = app.emit_to("notice", NOTICE_EVENT, Option::<Notice>::None);
    if let Some(window) = app.get_webview_window("notice") {
        let _ = window.hide();
    }
}

fn show_popup(app: &AppHandle) {
    let Some(window) = app.get_webview_window("popup") else {
        return;
    };

    // Bottom-right of the *work area*, not of the monitor: the monitor's own
    // rectangle includes the strip the taskbar covers, so measuring from its
    // bottom edge puts the popup underneath the taskbar.
    let placement = match (platform::primary_work_area(), window.outer_size()) {
        (Some((_, _, right, bottom)), Ok(size)) => {
            let scale = window.scale_factor().unwrap_or(1.0);
            let margin = (POPUP_MARGIN * scale) as i32;
            let x = right - size.width as i32 - margin;
            let y = bottom - size.height as i32 - margin;
            let _ = window.set_position(PhysicalPosition::new(x, y));
            Some((x, y, size.width as i32, size.height as i32))
        }
        _ => None,
    };

    let _ = window.show();
    let _ = window.set_always_on_top(true);

    // The same reason the overlay needs it: always-on-top alone loses to a
    // borderless fullscreen game, and a popup behind the game is a popup
    // nobody can press — including the one offering "finish the match".
    if let (Ok(handle), Some((x, y, width, height))) = (window.hwnd(), placement) {
        platform::raise_above_everything(handle.0 as isize, x, y, width, height);
    }
}

/// Creates one hidden overlay window per monitor, for any that are missing.
///
/// Called at startup and again before every break. Building a fullscreen
/// webview is slow enough to be visible, and doing it while handling an effect
/// means doing it from inside the event loop, so the windows are made once up
/// front and only shown later.
fn ensure_overlays(app: &AppHandle) -> tauri::Result<()> {
    let monitors = app.available_monitors()?;

    for (index, monitor) in monitors.iter().enumerate() {
        let label = format!("overlay-{index}");
        if app.get_webview_window(&label).is_some() {
            continue;
        }

        let window =
            WebviewWindowBuilder::new(app, &label, WebviewUrl::App("overlay/index.html".into()))
                .decorations(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .transparent(true)
                // A drop shadow around a transparent fullscreen window renders
                // as a visible border — which is exactly what made the overlay
                // read as a window laid over the desktop rather than a dimmed
                // screen.
                .shadow(false)
                // Without this the edges stay grabbable even with no frame, so
                // the overlay can be resized or shrugged off like any window.
                .resizable(false)
                .maximizable(false)
                .minimizable(false)
                // Taking focus is not needed to be seen, and stealing it from a
                // game is exactly the sort of thing that gets an app deleted.
                .focused(false)
                .visible(false)
                .build()?;

        place_overlay(&window, monitor);

        #[cfg(debug_assertions)]
        {
            let origin = monitor.position();
            let size = monitor.size();
            eprintln!(
                "[unsit] overlay {label} ready at {},{} ({}x{})",
                origin.x, origin.y, size.width, size.height
            );
        }
    }

    Ok(())
}

fn place_overlay(window: &tauri::WebviewWindow, monitor: &tauri::Monitor) {
    let origin = monitor.position();
    let size = monitor.size();
    let _ = window.set_position(PhysicalPosition::new(origin.x, origin.y));
    let _ = window.set_size(PhysicalSize::new(size.width, size.height));
}

/// Covers every monitor. Repositions on each show, so plugging a display in or
/// out between breaks cannot leave a gap.
fn show_overlays(app: &AppHandle) {
    // A monitor may have appeared since startup.
    if let Err(error) = ensure_overlays(app) {
        eprintln!("[unsit] could not create an overlay window: {error}");
    }

    let monitors = match app.available_monitors() {
        Ok(monitors) => monitors,
        Err(error) => {
            eprintln!("[unsit] could not enumerate monitors: {error}");
            return;
        }
    };

    for (index, monitor) in monitors.iter().enumerate() {
        let label = format!("overlay-{index}");
        let Some(window) = app.get_webview_window(&label) else {
            eprintln!("[unsit] overlay {label} is missing, nothing to show");
            continue;
        };

        place_overlay(&window, monitor);
        if let Err(error) = window.show() {
            eprintln!("[unsit] could not show {label}: {error}");
        }
        let _ = window.set_always_on_top(true);

        // Being topmost is not enough to get above the taskbar, which is
        // topmost as well. Re-assert the position by hand on every show.
        if let Ok(handle) = window.hwnd() {
            let origin = monitor.position();
            let size = monitor.size();
            platform::raise_above_everything(
                handle.0 as isize,
                origin.x,
                origin.y,
                size.width as i32,
                size.height as i32,
            );
        }
    }
}

fn hide_all(app: &AppHandle) {
    // The notice goes too: a warning still on screen once the break has
    // actually started is stale by definition.
    hide_notice(app);

    if let Some(popup) = app.get_webview_window("popup") {
        let _ = popup.hide();
    }
    for (label, window) in app.webview_windows() {
        if label.starts_with("overlay-") {
            let _ = window.hide();
        }
    }
}

/// Brings the app's startup registration in line with the config.
///
/// Never in a debug build: it would register the path of the binary under
/// `target/debug`, which stops existing the moment that directory is cleaned,
/// leaving a dead startup entry behind on the user's machine.
fn apply_autostart(app: &AppHandle, enabled: bool) {
    if cfg!(debug_assertions) {
        return;
    }

    let manager = app.autolaunch();
    let result = if enabled {
        manager.enable()
    } else {
        manager.disable()
    };

    if let Err(error) = result {
        eprintln!("[unsit] could not update the autostart entry: {error}");
    }
}

/// The one shortcut, doing whatever the moment calls for.
///
/// Two jobs rather than two keys: a waiting prompt gets the focus so it can be
/// answered, and the rest of the time it says how long is left. Three separate
/// shortcuts in an app this small is three things to remember.
fn on_hotkey(app: &AppHandle) {
    let Some(shared) = shared(app, "on_hotkey") else {
        return;
    };

    let (context, prompt_waiting, line) = {
        let core = lock(&shared);
        let Some(profile) = core.profile() else {
            return;
        };
        let snapshot = UiState::build(
            &core.machine,
            &core.profile,
            &profile,
            &core.config,
            SystemTime::now(),
            platform::idle_duration(),
        );
        let mode = i18n::profile_name(core.config.general.locale, &core.profile);
        (
            core.machine.context,
            matches!(core.machine.state, State::Prompt { .. }),
            status(&snapshot, &mode),
        )
    };

    // Nothing of ours can be drawn over an exclusive-fullscreen game, and
    // taking the foreground there drops the player out of it. Doing nothing is
    // the better answer.
    if context == Context::ExclusiveGame {
        return;
    }

    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if prompt_waiting {
            // The popup is already on screen; what is missing is the cursor,
            // which the game owns. Having just received a hotkey is one of the
            // few things that earns a background process the right to take the
            // foreground, so this is the one moment it can be done honestly.
            if let Some(window) = handle.get_webview_window("popup") {
                let _ = window.set_focus();
            }
            return;
        }

        show_notice(
            &handle,
            Notice {
                title: line,
                body: None,
            },
        );
    });
}

fn show_settings(app: &AppHandle) {
    let Some(window) = app.get_webview_window("settings") else {
        return;
    };

    // Pushed rather than waited for. Every window is created from the config
    // before setup runs, so the settings page mounts — and asks — while the
    // shared state is still being assembled, and gets nothing. Sending it when
    // the window is actually opened removes the race rather than racing better.
    if let Some(state) = shared(app, "show_settings") {
        let core = lock(&state);
        let view = SettingsView::build(&core.config, &core.profile, core.hotkey_registered);
        drop(core);
        let _ = app.emit_to("settings", SETTINGS_EVENT, view);
    }

    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
}

/// Switching mode starts a fresh cycle on the new cadence.
///
/// The alternative — carrying the old deadline over — is worse in both
/// directions: switching to Gaming mid-afternoon would fire a break instantly,
/// and switching to Work would quietly stretch one. It does mean a determined
/// user can push a break away by toggling modes, which is a fair trade for a
/// deliberate action in a tool you point at yourself.
fn set_profile(app: &AppHandle, name: &str) {
    let model = {
        let Some(shared) = shared(app, "set_profile") else {
            return;
        };
        let mut core = lock(&shared);
        if !core.config.profiles.contains_key(name) {
            return;
        }

        core.profile = name.to_owned();
        let Some(profile) = core.profile() else {
            return;
        };
        core.machine = Machine::new(SystemTime::now(), &profile);
        core.menu_model()
    };

    tray::refresh(app, &model);
    hide_all(app);
    dispatch(app, Event::Tick);
}

fn refresh_menu(app: &AppHandle) {
    let model = {
        let Some(shared) = shared(app, "refresh_menu") else {
            return;
        };
        let core = lock(&shared);
        core.menu_model()
    };
    tray::refresh(app, &model);
}

fn on_menu(app: &AppHandle, id: &str) {
    if let Some(name) = id.strip_prefix(tray::MODE_PREFIX) {
        set_profile(app, name);
    } else if id == tray::PAUSE_ID {
        dispatch(app, Event::PauseToggled);
        refresh_menu(app);
    } else if id == tray::SETTINGS_ID {
        show_settings(app);
    } else if id == tray::QUIT_ID {
        app.exit(0);
    }
}

#[tauri::command]
fn accept_break(app: AppHandle) {
    dispatch(&app, Event::Accept);
}

#[tauri::command]
fn snooze(app: AppHandle) {
    dispatch(&app, Event::Snooze);
}

#[tauri::command]
fn extend_for_match(app: AppHandle) {
    dispatch(&app, Event::MatchExtension);
}

#[tauri::command]
fn escape_break(app: AppHandle) {
    dispatch(&app, Event::EmergencyExit);
}

#[tauri::command]
fn toggle_pause(app: AppHandle) {
    dispatch(&app, Event::PauseToggled);
    refresh_menu(&app);
}

#[tauri::command]
fn switch_mode(app: AppHandle, key: String) {
    set_profile(&app, &key);
    hide_flyout(&app);
}

#[tauri::command]
fn close_flyout(app: AppHandle) {
    hide_flyout(&app);
}

#[tauri::command]
fn open_settings(app: AppHandle) {
    hide_flyout(&app);
    show_settings(&app);
}

/// `Option` rather than a made-up default: the settings window already handles
/// a null, and inventing a snapshot would show someone settings that are not
/// theirs.
#[tauri::command]
fn get_settings(app: AppHandle) -> Option<SettingsView> {
    let shared = shared(&app, "get_settings")?;
    let core = lock(&shared);
    Some(SettingsView::build(
        &core.config,
        &core.profile,
        core.hotkey_registered,
    ))
}

/// Bounds for anything the settings window can set.
///
/// Not paranoia: a zero-minute interval would fire a break every tick, and the
/// settings window is the one place a typo turns straight into behaviour.
const MIN_MINUTES: u64 = 1;
const MAX_INTERVAL_MIN: u64 = 720;
const MAX_BREAK_MIN: u64 = 120;
const MAX_SNOOZES: usize = 5;

fn sanitise(edit: ProfileEdit) -> ProfileEdit {
    let interval_min = edit.interval_min.clamp(MIN_MINUTES, MAX_INTERVAL_MIN);

    ProfileEdit {
        interval_min,
        break_min: edit.break_min.clamp(MIN_MINUTES, MAX_BREAK_MIN),
        // A warning longer than half the interval would fire almost as soon as
        // the cycle began, which is not a warning about anything.
        warning_sec: edit.warning_sec.min(interval_min * 30),
        snoozes_min: edit
            .snoozes_min
            .into_iter()
            .filter(|minutes| *minutes > 0)
            .map(|minutes| minutes.min(MAX_BREAK_MIN))
            .take(MAX_SNOOZES)
            .collect(),
        max_escalation: edit.max_escalation,
        match_extension_min: edit
            .match_extension_min
            .map(|minutes| minutes.clamp(MIN_MINUTES, MAX_BREAK_MIN)),
        session_limit_min: edit
            .session_limit_min
            .map(|minutes| minutes.clamp(MIN_MINUTES, MAX_INTERVAL_MIN)),
        session_break_min: edit
            .session_break_min
            .map(|minutes| minutes.clamp(MIN_MINUTES, MAX_BREAK_MIN)),
    }
}

/// Writes an edit onto a profile.
///
/// `prompt_timeout_sec` and `in_game_escalation` are deliberately untouched:
/// they are not in `ProfileEdit`, the settings window never shows them, and
/// overwriting them with a default would quietly undo a hand-edited config.
fn apply_edit(profile: &mut Profile, edit: ProfileEdit) {
    profile.interval_min = edit.interval_min;
    profile.break_min = edit.break_min;
    profile.warning_sec = edit.warning_sec;
    profile.snoozes_min = edit.snoozes_min;
    profile.max_escalation = edit.max_escalation;
    profile.match_extension_min = edit.match_extension_min;
    profile.session_limit_min = edit.session_limit_min;
    profile.session_break_min = edit.session_break_min;
}

#[tauri::command]
fn save_profile(app: AppHandle, profile: String, edit: ProfileEdit) {
    let restarted = {
        let Some(shared) = shared(&app, "save_profile") else {
            return;
        };
        let mut core = lock(&shared);

        let Some(entry) = core.config.profiles.get_mut(&profile) else {
            return;
        };
        apply_edit(entry, sanitise(edit));
        let _ = core.config.save();

        // Editing the mode you are currently in restarts the cycle, the same
        // rule as switching modes. Carrying the old deadline into a new
        // interval is the confusing option in both directions.
        if profile == core.profile {
            if let Some(updated) = core.profile() {
                core.machine = Machine::new(SystemTime::now(), &updated);
            }
            true
        } else {
            false
        }
    };

    if restarted {
        hide_all(&app);
    }
    dispatch(&app, Event::Tick);
}

/// Adds a mode, copying whatever the active one does.
///
/// Copying beats starting from blank defaults: someone adding "Reading" next
/// to "Chill" almost always wants something close to where they already are.
#[tauri::command]
fn add_profile(app: AppHandle, key: String) -> Option<String> {
    let key = key.trim().to_owned();
    if key.is_empty() {
        return None;
    }

    let model = {
        let shared = shared(&app, "add_profile")?;
        let mut core = lock(&shared);

        if core.config.profiles.contains_key(&key) {
            return None;
        }
        let template = core.profile().unwrap_or_else(|| {
            Config::default()
                .profiles
                .remove("work")
                .expect("the defaults always carry work")
        });

        core.config.profiles.insert(key.clone(), template);
        let _ = core.config.save();
        core.menu_model()
    };

    tray::refresh(&app, &model);
    dispatch(&app, Event::Tick);
    Some(key)
}

#[tauri::command]
fn remove_profile(app: AppHandle, key: String) {
    let (model, switched) = {
        let Some(shared) = shared(&app, "remove_profile") else {
            return;
        };
        let mut core = lock(&shared);

        // Never the last one. An app with no modes has nothing to count down.
        if core.config.profiles.len() <= 1 || !core.config.profiles.contains_key(&key) {
            return;
        }

        core.config.profiles.remove(&key);

        // Deleting the mode you are in, or the one that loads at startup,
        // has to land somewhere rather than dangle.
        let fallback = core
            .config
            .profiles
            .keys()
            .next()
            .cloned()
            .expect("checked above that one remains");

        let switched = core.profile == key;
        if switched {
            core.profile = fallback.clone();
            if let Some(profile) = core.profile() {
                core.machine = Machine::new(SystemTime::now(), &profile);
            }
        }
        if core.config.general.default_profile == key {
            core.config.general.default_profile = fallback;
        }

        let _ = core.config.save();
        (core.menu_model(), switched)
    };

    tray::refresh(&app, &model);
    if switched {
        hide_all(&app);
    }
    dispatch(&app, Event::Tick);
}

#[tauri::command]
fn set_sound(app: AppHandle, enabled: bool) {
    {
        let Some(shared) = shared(&app, "set_sound") else {
            return;
        };
        let mut core = lock(&shared);
        core.config.general.sound = enabled;
        let _ = core.config.save();
    }
    dispatch(&app, Event::Tick);
}

#[tauri::command]
fn set_autostart(app: AppHandle, enabled: bool) {
    {
        let Some(shared) = shared(&app, "set_autostart") else {
            return;
        };
        let mut core = lock(&shared);
        core.config.general.autostart = enabled;
        let _ = core.config.save();
    }

    apply_autostart(&app, enabled);
    dispatch(&app, Event::Tick);
}

#[tauri::command]
fn set_locale(app: AppHandle, locale: String) {
    let parsed = match locale.as_str() {
        "pl" => Locale::Pl,
        _ => Locale::En,
    };

    {
        let Some(shared) = shared(&app, "set_locale") else {
            return;
        };
        let mut core = lock(&shared);
        core.config.general.locale = parsed;
        let _ = core.config.save();
    }

    // The tray menu is built in Rust, so it has to be rebuilt by hand.
    refresh_menu(&app);
    dispatch(&app, Event::Tick);
}

pub fn run() {
    tauri::Builder::default()
        // Has to come first, before anything else has a chance to start. Two
        // copies would fight over one config file and stack two overlays on
        // every break.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // Launching again is someone looking for the app, not asking for a
            // second one, so show them where it lives.
            show_settings(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    // Release would fire a second time for one press.
                    if event.state() == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        on_hotkey(app);
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_notification::init())
        .on_window_event(|window, event| {
            // Closing settings must not take the app down with it — the tray is
            // what owns the lifetime here.
            match event {
                WindowEvent::CloseRequested { api, .. } if window.label() == "settings" => {
                    api.prevent_close();
                    let _ = window.hide();
                }
                // The flyout closes the moment attention moves elsewhere,
                // like every other tray panel on Windows.
                WindowEvent::Focused(false) if window.label() == "flyout" => {
                    let _ = window.hide();
                }
                _ => {}
            }
        })
        .setup(|app| {
            // A broken config should not stop the app from nagging you.
            let config = Config::load_or_create().unwrap_or_default();
            let profile_name = config.general.default_profile.clone();
            let profile = config
                .profiles
                .get(&profile_name)
                .cloned()
                .unwrap_or_else(|| Config::default().profiles["work"].clone());

            let context_source = WindowsContext::new(&config.detection.games);
            let machine = Machine::new(SystemTime::now(), &profile);

            let autostart = config.general.autostart;
            let hotkey = config.general.hotkey.clone();
            let core = Core {
                machine,
                config,
                profile: profile_name,
                context_source,
                hotkey_registered: false,
                notice_until: None,
            };
            let model = core.menu_model();
            app.manage(Shared {
                core: Mutex::new(core),
            });

            let handle = app.handle().clone();
            tray::create(&handle, &model, on_menu, toggle_flyout)?;
            apply_autostart(&handle, autostart);

            // A shortcut someone else already owns fails to register, and that
            // is not a reason to refuse to start — it just means this one way
            // in is unavailable.
            let claimed = match hotkey.parse::<tauri_plugin_global_shortcut::Shortcut>() {
                Ok(shortcut) => {
                    match tauri_plugin_global_shortcut::GlobalShortcutExt::global_shortcut(&handle)
                        .register(shortcut)
                    {
                        Ok(()) => true,
                        Err(error) => {
                            eprintln!("[unsit] could not register {hotkey}: {error}");
                            false
                        }
                    }
                }
                Err(error) => {
                    eprintln!("[unsit] {hotkey} is not a valid shortcut: {error}");
                    false
                }
            };

            // Recorded rather than only logged: in a release build nobody ever
            // sees stderr, and a shortcut that silently does nothing is worse
            // than one that says it is unavailable.
            if let Some(state) = shared(&handle, "setup") {
                lock(&state).hotkey_registered = claimed;
            }

            // Built now rather than when a break starts: a fullscreen webview
            // takes long enough to create that doing it on demand shows.
            if let Err(error) = ensure_overlays(&handle) {
                eprintln!("[unsit] could not create overlay windows: {error}");
            }

            // The whole app is this loop. Everything else reacts to it.
            let ticking = app.handle().clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(TICK);
                dispatch(&ticking, Event::Tick);
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            accept_break,
            snooze,
            extend_for_match,
            escape_break,
            toggle_pause,
            set_locale,
            set_autostart,
            get_settings,
            switch_mode,
            close_flyout,
            open_settings,
            save_profile,
            add_profile,
            remove_profile,
            set_sound
        ])
        .run(tauri::generate_context!())
        .expect("error while running Unsit");
}
