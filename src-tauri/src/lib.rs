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
use tauri_plugin_notification::NotificationExt;

use config::{Config, Locale, Profile};
use engine::{Effect, Event, Machine, Policy, State};
use platform::{ContextSource, WindowsContext};
use ui::{StateKind, UiState, STATE_EVENT};

const TICK: Duration = Duration::from_secs(1);
/// Gap between the popup and the corner of the screen, in logical pixels.
const POPUP_MARGIN: f64 = 24.0;

struct Core {
    machine: Machine,
    config: Config,
    profile: String,
    context_source: WindowsContext,
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

/// One turn of the machine: observe, step, publish, act.
///
/// The lock is held only for the engine call. Everything that touches a window
/// runs afterwards on the main thread, because Windows window operations driven
/// from a worker are a reliable way to deadlock.
fn dispatch(app: &AppHandle, event: Event) {
    let shared = app.state::<Shared>();
    let now = SystemTime::now();
    let idle = platform::idle_duration();

    #[cfg(debug_assertions)]
    let described = format!("{event:?}");
    #[cfg(debug_assertions)]
    let user_driven = !matches!(event, Event::Tick);

    let (effects, snapshot, profile_name) = {
        let mut core = lock(&shared);
        let Some(profile) = core.profile() else {
            return;
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

        (effects, snapshot, core.profile.clone())
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

    let tip = tooltip(&snapshot, &i18n::profile_name(locale, &profile_name));
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        tray::set_tooltip(&handle, &tip);
        for effect in effects {
            apply(&handle, effect, &profile_name, locale);
        }
    });
}

fn apply(app: &AppHandle, effect: Effect, profile: &str, locale: Locale) {
    match effect {
        Effect::ShowToast => show_toast(app, locale),
        Effect::ShowPopup => show_popup(app),
        Effect::ShowOverlay => show_overlays(app),
        Effect::HideAll => hide_all(app),
        // Sound is still unwired; silence beats a placeholder chime.
        Effect::PlaySound => {}
        Effect::LockScreen => platform::lock_workstation(),
        Effect::Log(stat) => stats::record(stat, profile),
    }
}

fn tooltip(snapshot: &UiState, mode: &str) -> String {
    let locale = snapshot.locale;

    match snapshot.kind {
        StateKind::Break => {
            if snapshot.counter_held {
                i18n::tooltip_held(locale)
            } else {
                let left = snapshot
                    .required_seconds
                    .saturating_sub(snapshot.earned_seconds);
                i18n::tooltip_break(locale, &clock(left))
            }
        }
        StateKind::Done => i18n::tooltip_done(locale),
        StateKind::Paused | StateKind::Suspended => i18n::tooltip_paused(locale),
        StateKind::Prompt => i18n::tooltip_due(locale, mode),
        _ => match snapshot.seconds_left {
            Some(left) => i18n::tooltip_counting(locale, mode, &clock(left)),
            None => i18n::tooltip_due(locale, mode),
        },
    }
}

fn clock(seconds: u64) -> String {
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

fn show_toast(app: &AppHandle, locale: Locale) {
    let _ = app
        .notification()
        .builder()
        .title(i18n::warning_title(locale))
        .body(i18n::warning_body(locale))
        .show();
}

fn show_popup(app: &AppHandle) {
    let Some(window) = app.get_webview_window("popup") else {
        return;
    };

    // Bottom-right of the *work area*, not of the monitor: the monitor's own
    // rectangle includes the strip the taskbar covers, so measuring from its
    // bottom edge puts the popup underneath the taskbar.
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
    if let Some(popup) = app.get_webview_window("popup") {
        let _ = popup.hide();
    }
    for (label, window) in app.webview_windows() {
        if label.starts_with("overlay-") {
            let _ = window.hide();
        }
    }
}

fn show_settings(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
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
        let shared = app.state::<Shared>();
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
        let shared = app.state::<Shared>();
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
fn set_locale(app: AppHandle, locale: String) {
    let parsed = match locale.as_str() {
        "pl" => Locale::Pl,
        _ => Locale::En,
    };

    {
        let shared = app.state::<Shared>();
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
        .plugin(tauri_plugin_notification::init())
        .on_window_event(|window, event| {
            // Closing settings must not take the app down with it — the tray is
            // what owns the lifetime here.
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "settings" {
                    api.prevent_close();
                    let _ = window.hide();
                }
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

            let core = Core {
                machine,
                config,
                profile: profile_name,
                context_source,
            };
            let model = core.menu_model();
            app.manage(Shared {
                core: Mutex::new(core),
            });

            let handle = app.handle().clone();
            tray::create(&handle, &model, on_menu)?;

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
            set_locale
        ])
        .run(tauri::generate_context!())
        .expect("error while running Unsit");
}
