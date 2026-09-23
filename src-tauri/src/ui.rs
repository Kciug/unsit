//! The view model pushed to every window.
//!
//! Deliberately not a mirror of `engine::State`: seconds are already resolved,
//! there are no deadline timestamps, and nothing here requires the UI to know
//! how the machine works. The engine can change shape without the frontend
//! noticing.

use std::time::Duration;

use serde::Serialize;

use crate::config::{Config, EscapeMethod, Locale, Profile};
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

fn remaining(deadline: Timestamp, now: Timestamp) -> u64 {
    deadline
        .duration_since(now)
        .map(|left| left.as_secs())
        .unwrap_or(0)
}
