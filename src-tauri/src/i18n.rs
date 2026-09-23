//! Strings the backend owns.
//!
//! Toasts and (soon) the tray menu are built in Rust, so the locale cannot live
//! in the frontend alone. This is the Rust half of the same dictionary; the
//! frontend mirrors whatever locale arrives in the state event.

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

pub fn break_due_title(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "Time for a break",
        Locale::Pl => "Czas na przerwę",
    }
}
