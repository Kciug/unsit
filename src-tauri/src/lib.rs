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

/// One turn of the machine: observe, step, publish, act.
///
/// The lock is held only for the engine call. Everything that touches a window
/// runs afterwards on the main thread, because Windows window operations driven
/// from a worker are a reliable way to deadlock.
fn dispatch(app: &AppHandle, event: Event) {
    let shared = app.state::<Shared>();
    let now = SystemTime::now();
    let idle = platform::idle_duration();

    let (effects, snapshot, profile_name) = {
        let Ok(mut core) = shared.core.lock() else {
            return;
        };
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

    // Bottom-right of the primary monitor, out of the way of whatever is open.
    if let Ok(Some(monitor)) = window.primary_monitor() {
        if let Ok(size) = window.outer_size() {
            let scale = monitor.scale_factor();
            let margin = (POPUP_MARGIN * scale) as i32;
            let area = monitor.size();
            let origin = monitor.position();
            let x = origin.x + area.width as i32 - size.width as i32 - margin;
            let y = origin.y + area.height as i32 - size.height as i32 - margin;
            let _ = window.set_position(PhysicalPosition::new(x, y));
        }
    }

    let _ = window.show();
    let _ = window.set_always_on_top(true);
}

/// One overlay per monitor, rebuilt on every show so that plugging a display in
/// or out between breaks cannot leave a gap.
fn show_overlays(app: &AppHandle) {
    let Ok(monitors) = app.available_monitors() else {
        return;
    };

    for (index, monitor) in monitors.iter().enumerate() {
        let label = format!("overlay-{index}");

        let window = match app.get_webview_window(&label) {
            Some(existing) => existing,
            None => {
                let built = WebviewWindowBuilder::new(
                    app,
                    &label,
                    WebviewUrl::App("overlay/index.html".into()),
                )
                .decorations(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .transparent(true)
                .shadow(false)
                // Taking focus is not needed to be seen, and stealing it from a
                // game is exactly the sort of thing that gets an app deleted.
                .focused(false)
                .visible(false)
                .build();

                match built {
                    Ok(window) => window,
                    Err(_) => continue,
                }
            }
        };

        let origin = monitor.position();
        let size = monitor.size();
        let _ = window.set_position(PhysicalPosition::new(origin.x, origin.y));
        let _ = window.set_size(PhysicalSize::new(size.width, size.height));
        let _ = window.show();
        let _ = window.set_always_on_top(true);
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
        let Ok(mut core) = shared.core.lock() else {
            return;
        };
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
        let Ok(core) = shared.core.lock() else {
            return;
        };
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
        let Ok(mut core) = shared.core.lock() else {
            return;
        };
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
