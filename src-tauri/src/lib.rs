mod config;
mod engine;
mod platform;
mod stats;

// Command stubs. They exist so the frontend's `invoke` calls resolve against
// something; the engine behind them is the next piece of work. Every one of
// them is a request — the backend stays the one deciding what happens.

#[tauri::command]
fn accept_break() {}

#[tauri::command]
fn snooze() {}

#[tauri::command]
fn extend_for_match() {}

#[tauri::command]
fn escape_break() {}

#[tauri::command]
fn toggle_pause() {}

#[tauri::command]
fn set_locale(_locale: String) {}

pub fn run() {
    // `settings` and `popup` are declared in tauri.conf.json. Overlay windows
    // are not: there is one per monitor, so they get created at runtime and
    // rebuilt when a monitor is plugged in or removed.
    tauri::Builder::default()
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
