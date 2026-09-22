//! The break state machine.
//!
//! Pure logic on purpose: no Win32, no I/O, no reading the clock. `step` takes
//! everything it needs as arguments and hands back effects as data for the
//! caller to perform. That is what makes the whole thing testable against a
//! fake clock and a fake idle source.

use std::time::{Duration, SystemTime};

/// Wall-clock instant a deadline is compared against.
///
/// Deliberately wall clock rather than `Instant`: deadlines have to survive
/// sleep/resume, and a monotonic clock that stops during sleep would silently
/// postpone every break by however long the machine was off.
pub type Timestamp = SystemTime;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    Working { due: Timestamp },
    Warning { due: Timestamp },
    Prompt { snoozes_left: u8 },
    Snoozed { until: Timestamp, snoozes_left: u8 },
    Break { required: Duration, earned: Duration },
    Suspended { resume_to: Box<State> },
    Paused { resume_to: Box<State> },
}

/// What the machine can observe about the outside world.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Context {
    Free,
    Game,
    Call,
    Presentation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Tick,
    Accept,
    Snooze,
    MatchExtension,
    EmergencyExit,
    ContextChanged(Context),
    SystemResumed,
    PauseToggled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatEvent {
    BreakTaken,
    BreakSkipped,
    NaturalBreak,
    EscapeHatchUsed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    ShowToast,
    ShowPopup,
    ShowOverlay,
    HideAll,
    PlaySound,
    LockScreen,
    Log(StatEvent),
}

/// The one function that matters. Everything else in this crate serves it.
pub fn step(
    _state: State,
    _event: Event,
    _now: Timestamp,
    _idle: Duration,
    _profile: &crate::config::Profile,
) -> (State, Vec<Effect>) {
    todo!("state machine — first real piece of v0.1")
}
