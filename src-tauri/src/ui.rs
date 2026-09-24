//! The view model pushed to every window.
//!
//! Deliberately not a mirror of `engine::State`: seconds are already resolved,
//! there are no deadline timestamps, and nothing here requires the UI to know
//! how the machine works. The engine can change shape without the frontend
//! noticing.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::config::{Config, Escalation, EscapeMethod, Locale, Profile};
use crate::engine::{Machine, State, Timestamp};

pub const STATE_EVENT: &str = "unsit://state";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum StateKind {
    Working,
    Warning,
    Prompt,
    Snoozed,
    Break,
    Done,
    Suspended,
    Paused,
}

impl From<&State> for StateKind {
    fn from(state: &State) -> Self {
        match state {
            State::Working { .. } => StateKind::Working,
            State::Warning { .. } => StateKind::Warning,
            State::Prompt { .. } => StateKind::Prompt,
            State::Snoozed { .. } => StateKind::Snoozed,
            State::Break { .. } => StateKind::Break,
            State::Done { .. } => StateKind::Done,
            State::Suspended { .. } => StateKind::Suspended,
            State::Paused { .. } => StateKind::Paused,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UiState {
    pub kind: StateKind,
    pub mode: String,
    pub locale: Locale,
    /// Seconds until the next transition, or null when nothing counts down.
    pub seconds_left: Option<u64>,
    pub snoozes_left: u8,
    pub next_snooze_minutes: Option<u64>,
    pub match_extension_available: bool,
    pub match_extension_minutes: u64,
    pub earned_seconds: u64,
    pub required_seconds: u64,
    /// True while the break counter is held because there is input.
    pub counter_held: bool,
    pub escape_method: EscapeMethod,
    pub escape_hold_seconds: u64,
    pub autostart: bool,
}

impl UiState {
    pub fn build(
        machine: &Machine,
        mode: &str,
        profile: &Profile,
        config: &Config,
        now: Timestamp,
        idle: Duration,
    ) -> Self {
        let seconds_left = match &machine.state {
            State::Working { due } | State::Warning { due } => Some(remaining(*due, now)),
            State::Snoozed { until } => Some(remaining(*until, now)),
            // A prompt waits on the user, not on the clock.
            _ => None,
        };

        let (earned, required, counter_held) = match machine.state {
            State::Break { required, earned } => (
                earned.as_secs(),
                required.as_secs(),
                idle < config.general.idle_threshold(),
            ),
            _ => (0, 0, false),
        };

        Self {
            kind: StateKind::from(&machine.state),
            mode: mode.to_owned(),
            locale: config.general.locale,
            seconds_left,
            snoozes_left: machine.snoozes_left(profile),
            next_snooze_minutes: profile
                .snoozes_min
                .get(machine.cycle.snoozes_used as usize)
                .copied(),
            match_extension_available: machine.match_extension_available(profile),
            match_extension_minutes: profile.match_extension_min.unwrap_or(0),
            earned_seconds: earned,
            required_seconds: required,
            counter_held,
            escape_method: config.escape.method,
            escape_hold_seconds: config.escape.hold_sec,
            autostart: config.general.autostart,
        }
    }
}

/// The editable half of a mode, travelling in both directions.
///
/// Everything a mode has that a person would reasonably want to change, so the
/// settings window hands back the shape it was given rather than growing a pile
/// of single-field commands.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileEdit {
    pub interval_min: u64,
    pub break_min: u64,
    pub warning_sec: u64,
    /// Lengths in order, so a mode can taper: five minutes, then three.
    pub snoozes_min: Vec<u64>,
    pub max_escalation: Escalation,
    /// All three are meaningless outside a gaming-shaped mode, and `None` is
    /// how a mode says it does not do this at all.
    pub match_extension_min: Option<u64>,
    pub session_limit_min: Option<u64>,
    pub session_break_min: Option<u64>,
}

impl From<&Profile> for ProfileEdit {
    fn from(profile: &Profile) -> Self {
        Self {
            interval_min: profile.interval_min,
            break_min: profile.break_min,
            warning_sec: profile.warning_sec,
            snoozes_min: profile.snoozes_min.clone(),
            max_escalation: profile.max_escalation,
            match_extension_min: profile.match_extension_min,
            session_limit_min: profile.session_limit_min,
            session_break_min: profile.session_break_min,
        }
    }
}

/// One editable mode, as the settings window sees it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileView {
    pub key: String,
    #[serde(flatten)]
    pub edit: ProfileEdit,
}

/// A snapshot for the settings window.
///
/// Fetched once when that window opens rather than ridden along on the state
/// event: the profiles change when someone edits them, not sixty times a
/// minute, and `UiState` goes out on every tick to every window.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    pub locale: Locale,
    pub autostart: bool,
    pub sound: bool,
    pub hotkey: String,
    /// False when something else already owns the shortcut, so the settings
    /// window can say so instead of leaving a key that quietly does nothing.
    pub hotkey_registered: bool,
    pub active_profile: String,
    pub profiles: Vec<ProfileView>,
}

impl SettingsView {
    pub fn build(config: &Config, active: &str, hotkey_registered: bool) -> Self {
        Self {
            locale: config.general.locale,
            autostart: config.general.autostart,
            sound: config.general.sound,
            hotkey: config.general.hotkey.clone(),
            hotkey_registered,
            active_profile: active.to_owned(),
            profiles: config
                .profiles
                .iter()
                .map(|(key, profile)| ProfileView {
                    key: key.clone(),
                    edit: ProfileEdit::from(profile),
                })
                .collect(),
        }
    }
}

fn remaining(deadline: Timestamp, now: Timestamp) -> u64 {
    deadline
        .duration_since(now)
        .map(|left| left.as_secs())
        .unwrap_or(0)
}

pub const NOTICE_EVENT: &str = "unsit://notice";

pub const FLYOUT_EVENT: &str = "unsit://flyout";

/// One selectable mode in the flyout, already named in the right language.
#[derive(Debug, Clone, Serialize)]
pub struct FlyoutMode {
    pub key: String,
    pub label: String,
}

/// What the tray flyout shows.
///
/// Pushed when it opens rather than polled: it is on screen for a few seconds
/// at a time, and the countdown it shows comes from the state event like
/// everywhere else.
#[derive(Debug, Clone, Serialize)]
pub struct FlyoutView {
    pub modes: Vec<FlyoutMode>,
    pub active: String,
    pub paused: bool,
}

/// Text for the transient notice window.
///
/// Already translated: that window stands in for a Windows notification while
/// a game is running, and the backend owns those strings because it owns the
/// locale — the same reason the tray menu is built in Rust.
#[derive(Debug, Clone, Serialize)]
pub struct Notice {
    pub title: String,
    pub body: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    /// The settings window reads these names literally, and `flatten` plus
    /// `rename_all` is exactly the sort of thing that changes shape quietly.
    #[test]
    fn the_settings_view_keeps_the_shape_typescript_expects() {
        let view = SettingsView::build(&Config::default(), "work", true);
        let json = serde_json::to_string(&view).expect("serialise");

        for field in [
            "hotkeyRegistered",
            "activeProfile",
            "intervalMin",
            "breakMin",
            "warningSec",
            "snoozesMin",
            "maxEscalation",
            "matchExtensionMin",
            "sessionLimitMin",
            "sessionBreakMin",
        ] {
            assert!(
                json.contains(&format!("\"{field}\"")),
                "missing {field}: {json}"
            );
        }

        // Flattened onto the profile, not nested under an "edit" key.
        assert!(
            !json.contains("\"edit\""),
            "profile fields got nested: {json}"
        );
    }
}
