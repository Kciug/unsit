//! The tray icon: the only way to drive the app until the settings UI exists.
//!
//! The menu is rebuilt rather than mutated whenever the mode, the pause state
//! or the locale changes. Rebuilding is cheap, happens only on a user action,
//! and keeps the check marks honest without tracking a pile of item handles.

use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Wry};

use crate::config::Locale;
use crate::i18n;

pub const TRAY_ID: &str = "main";

pub const MODE_PREFIX: &str = "mode:";
pub const PAUSE_ID: &str = "pause";
pub const SETTINGS_ID: &str = "settings";
pub const QUIT_ID: &str = "quit";

pub struct MenuModel {
    pub locale: Locale,
    pub profiles: Vec<String>,
    pub current: String,
    pub paused: bool,
}

pub fn build_menu(app: &AppHandle, model: &MenuModel) -> tauri::Result<Menu<Wry>> {
    let locale = model.locale;

    let modes: Vec<CheckMenuItem<Wry>> = model
        .profiles
        .iter()
        .map(|key| {
            CheckMenuItem::with_id(
                app,
                format!("{MODE_PREFIX}{key}"),
                i18n::profile_name(locale, key),
                true,
                *key == model.current,
                None::<&str>,
            )
        })
        .collect::<tauri::Result<_>>()?;

    let mode_refs: Vec<&dyn IsMenuItem<Wry>> = modes
        .iter()
        .map(|item| item as &dyn IsMenuItem<Wry>)
        .collect();
    let mode_menu = Submenu::with_items(app, i18n::menu_mode(locale), true, &mode_refs)?;

    let pause_label = if model.paused {
        i18n::menu_resume(locale)
    } else {
        i18n::menu_pause(locale)
    };
    let pause = MenuItem::with_id(app, PAUSE_ID, pause_label, true, None::<&str>)?;
    let settings = MenuItem::with_id(
        app,
        SETTINGS_ID,
        i18n::menu_settings(locale),
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, QUIT_ID, i18n::menu_quit(locale), true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;

    Menu::with_items(
        app,
        &[&mode_menu, &separator, &pause, &settings, &separator, &quit],
    )
}

pub fn create<F, G>(
    app: &AppHandle,
    model: &MenuModel,
    on_menu: F,
    on_click: G,
) -> tauri::Result<()>
where
    F: Fn(&AppHandle, &str) + Send + Sync + 'static,
    G: Fn(&AppHandle) + Send + Sync + 'static,
{
    let menu = build_menu(app, model)?;
    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or_else(|| tauri::Error::AssetNotFound("default window icon".into()))?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .menu(&menu)
        .tooltip("Unsit")
        // Left click belongs to the flyout; the menu stays on right click.
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| on_menu(app, event.id.as_ref()))
        .on_tray_icon_event(move |tray, event| {
            // On the release, not the press: acting on the press leaves the
            // click hitting whatever the flyout puts under the cursor.
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                on_click(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}

pub fn refresh(app: &AppHandle, model: &MenuModel) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    if let Ok(menu) = build_menu(app, model) {
        let _ = tray.set_menu(Some(menu));
    }
}

pub fn set_tooltip(app: &AppHandle, text: &str) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_tooltip(Some(text));
    }
}
