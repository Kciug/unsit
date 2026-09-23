//! Strings the backend owns.
//!
//! Toasts and the tray menu are built in Rust, so the locale cannot live in the
//! frontend alone. This is the Rust half of the same dictionary; the frontend
//! mirrors whatever locale arrives in the state event.

use crate::config::Locale;

pub fn warning_title(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "Break coming up",
        Locale::Pl => "Zbliża się przerwa",
    }
}

pub fn warning_body(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "Time to wrap up what you are doing.",
        Locale::Pl => "Czas domknąć to, co robisz.",
    }
}

pub fn menu_mode(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "Mode",
        Locale::Pl => "Tryb",
    }
}

pub fn menu_pause(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "Pause",
        Locale::Pl => "Pauza",
    }
}

pub fn menu_resume(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "Resume",
        Locale::Pl => "Wznów",
    }
}

pub fn menu_settings(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "Settings",
        Locale::Pl => "Ustawienia",
    }
}

pub fn menu_quit(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "Quit",
        Locale::Pl => "Zakończ",
    }
}

/// Display name for a profile key from the config.
///
/// Falls back to the raw key, so a profile someone added by hand still shows up
/// in the menu under whatever they called it.
pub fn profile_name(locale: Locale, key: &str) -> String {
    match (locale, key) {
        (Locale::En, "work") => "Work".into(),
        (Locale::En, "gaming") => "Gaming".into(),
        (Locale::En, "chill") => "Chill".into(),
        (Locale::Pl, "work") => "Praca".into(),
        (Locale::Pl, "gaming") => "Granie".into(),
        (Locale::Pl, "chill") => "Luz".into(),
        _ => key.to_owned(),
    }
}

pub fn tooltip_break(locale: Locale, remaining: &str) -> String {
    match locale {
        Locale::En => format!("Unsit — break, {remaining} left"),
        Locale::Pl => format!("Unsit — przerwa, zostało {remaining}"),
    }
}

pub fn tooltip_held(locale: Locale) -> String {
    match locale {
        Locale::En => "Unsit — counter stopped, step away".into(),
        Locale::Pl => "Unsit — licznik stoi, odejdź od klawiatury".into(),
    }
}

pub fn tooltip_due(locale: Locale, mode: &str) -> String {
    match locale {
        Locale::En => format!("Unsit — {mode}, break due"),
        Locale::Pl => format!("Unsit — {mode}, przerwa się należy"),
    }
}

pub fn tooltip_counting(locale: Locale, mode: &str, remaining: &str) -> String {
    match locale {
        Locale::En => format!("Unsit — {mode}, break in {remaining}"),
        Locale::Pl => format!("Unsit — {mode}, przerwa za {remaining}"),
    }
}

pub fn tooltip_paused(locale: Locale) -> String {
    match locale {
        Locale::En => "Unsit — paused".into(),
        Locale::Pl => "Unsit — zapauzowane".into(),
    }
}
