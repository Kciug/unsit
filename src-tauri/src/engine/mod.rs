//! The break state machine.
//!
//! Pure logic on purpose: no Win32, no I/O, no reading the clock. `step` takes
//! everything it needs as arguments and hands back effects as data for the
//! caller to perform. That is what makes the whole thing testable against a
//! fake clock and a fake idle source.

#[cfg(test)]
mod tests;

use std::time::{Duration, SystemTime};

use crate::config::{Escalation, Profile};

/// Wall-clock instant a deadline is compared against.
///
/// Deliberately wall clock rather than `Instant`: deadlines have to survive
/// sleep/resume, and a monotonic clock that stops while the machine is off
/// would silently postpone every break by however long it slept.
pub type Timestamp = SystemTime;

/// What the machine can observe about the outside world.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Context {
    Free,
    Game,
    /// Detected from the microphone. Not wired up yet — call detection is v0.3+
    /// — but the machine already knows how to hold escalation for one.
    #[allow(dead_code)]
    Call,
    Presentation,
}

impl Context {
    /// A call or a presentation is not a moment to be nagged.
    fn holds_escalation(self) -> bool {
        matches!(self, Context::Call | Context::Presentation)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    Working {
        due: Timestamp,
    },
    Warning {
        due: Timestamp,
    },
    /// `since` is when the popup went up — an ignored popup escalates.
    Prompt {
        since: Timestamp,
    },
    Snoozed {
        until: Timestamp,
    },
    Break {
        required: Duration,
        earned: Duration,
    },
    /// The break is served, and the overlay is waiting to be dismissed.
    ///
    /// A separate state rather than going straight back to work: ending the
    /// break on its own means someone who actually left has no way of knowing
    /// it happened, and starts the next interval counting while they are still
    /// away from the desk.
    Done {
        since: Timestamp,
    },
    Suspended {
        resume_to: Box<State>,
    },
    Paused {
        resume_to: Box<State>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Tick,
    Accept,
    Snooze,
    MatchExtension,
    EmergencyExit,
    ContextChanged(Context),
    /// Nothing emits this yet: a sleep is already caught by the gap between
    /// ticks. Kept because Windows can tell us directly, and that is strictly
    /// better than inferring it.
    #[allow(dead_code)]
    SystemResumed,
    PauseToggled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    /// Hard mode, the last rung. The ladder stops at the overlay for now, so
    /// nothing produces this — the handler and the Win32 call are both ready.
    #[allow(dead_code)]
    LockScreen,
    Log(StatEvent),
}

/// Everything `step` needs from the config.
///
/// A bundle rather than a bare `&Profile` because the idle threshold lives in
/// `[general]` — it describes the user, not the profile.
#[derive(Debug, Clone, Copy)]
pub struct Policy<'a> {
    pub profile: &'a Profile,
    pub idle_threshold: Duration,
}

/// Bookkeeping that outlives any single state.
///
/// This is why `step` takes a `Machine` and not a bare `State`: "once per
/// cycle" and "three hours in total" cannot be expressed by the state alone.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Cycle {
    pub snoozes_used: u8,
    pub match_extension_used: bool,
    /// Working time since the last break that actually happened. Skipping a
    /// break deliberately does *not* reset this, or the session cap would be
    /// trivially defeated by always skipping.
    pub session_used: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Machine {
    pub state: State,
    pub cycle: Cycle,
    pub context: Context,
    /// When `step` last saw a tick. A large gap means the machine slept.
    pub last_tick: Option<Timestamp>,
}

impl Machine {
    pub fn new(now: Timestamp, profile: &Profile) -> Self {
        Self {
            state: State::Working {
                due: plus(now, profile.interval()),
            },
            cycle: Cycle::default(),
            context: Context::Free,
            last_tick: None,
        }
    }

    pub fn snoozes_left(&self, profile: &Profile) -> u8 {
        profile
            .snooze_count()
            .saturating_sub(self.cycle.snoozes_used)
    }

    pub fn match_extension_available(&self, profile: &Profile) -> bool {
        profile.match_extension_min.is_some() && !self.cycle.match_extension_used
    }
}

/// The one function that matters. Everything else in this module serves it.
pub fn step(
    mut machine: Machine,
    event: Event,
    now: Timestamp,
    idle: Duration,
    policy: &Policy,
) -> (Machine, Vec<Effect>) {
    let mut effects = Vec::new();

    match event {
        Event::Tick => on_tick(&mut machine, now, idle, policy, &mut effects),
        Event::Accept => on_accept(&mut machine, now, policy, &mut effects),
        Event::Snooze => on_snooze(&mut machine, now, policy, &mut effects),
        Event::MatchExtension => on_match_extension(&mut machine, now, policy, &mut effects),
        Event::EmergencyExit => on_emergency_exit(&mut machine, now, policy, &mut effects),
        Event::ContextChanged(context) => {
            on_context_changed(&mut machine, context, now, policy, &mut effects)
        }
        Event::SystemResumed => on_system_resumed(&mut machine, now, policy, &mut effects),
        Event::PauseToggled => on_pause_toggled(&mut machine, now, policy, &mut effects),
    }

    (machine, effects)
}

fn on_tick(
    machine: &mut Machine,
    now: Timestamp,
    idle: Duration,
    policy: &Policy,
    effects: &mut Vec<Effect>,
) {
    let gap = machine
        .last_tick
        .map(|last| since(now, last))
        .unwrap_or_default();
    machine.last_tick = Some(now);

    // A gap between ticks longer than a whole break means nobody was here —
    // the machine slept, or the process was starved. Either way it counts.
    if gap >= policy.profile.break_length() && !matches!(machine.state, State::Break { .. }) {
        finish_cycle(machine, now, policy, StatEvent::NaturalBreak, effects);
        return;
    }

    match machine.state.clone() {
        State::Working { due } | State::Warning { due } => {
            machine.cycle.session_used = machine.cycle.session_used.saturating_add(gap);

            // Walking away for a whole break length is a break, whatever the
            // schedule says.
            if idle >= policy.profile.break_length() {
                finish_cycle(machine, now, policy, StatEvent::NaturalBreak, effects);
                return;
            }

            if let Some(required) = forced_session_break(machine, policy) {
                begin_break(machine, required, effects);
                return;
            }

            if now >= due {
                enter_prompt(machine, now, policy, effects);
            } else if matches!(machine.state, State::Working { .. })
                && now >= minus(due, policy.profile.warning())
            {
                machine.state = State::Warning { due };
                effects.push(Effect::ShowToast);
            }
        }

        State::Prompt { since: shown } => {
            if since(now, shown) >= policy.profile.prompt_timeout()
                && ceiling(machine, policy).rank() >= Escalation::Overlay.rank()
            {
                let required = forced_session_break(machine, policy)
                    .unwrap_or_else(|| policy.profile.break_length());
                begin_break(machine, required, effects);
            }
        }

        State::Snoozed { until } => {
            if now >= until {
                enter_prompt(machine, now, policy, effects);
            }
        }

        State::Break { required, earned } => {
            // The counter only moves while there is no input. This one rule is
            // the reason the whole app exists.
            if idle >= policy.idle_threshold {
                let earned = earned.saturating_add(gap);
                if earned >= required {
                    complete_break(machine, now, effects);
                } else {
                    machine.state = State::Break { required, earned };
                }
            }
        }

        // Waiting on the user to come back and say so.
        State::Done { .. } => {}

        // Held on purpose: a call or a manual pause stops the clock entirely.
        State::Suspended { .. } | State::Paused { .. } => {}
    }
}

fn on_accept(machine: &mut Machine, now: Timestamp, policy: &Policy, effects: &mut Vec<Effect>) {
    // Coming back from a finished break. The next interval starts here, when
    // the user is actually at the desk again — not when the timer ran out.
    if matches!(machine.state, State::Done { .. }) {
        machine.state = State::Working {
            due: plus(now, policy.profile.interval()),
        };
        effects.push(Effect::HideAll);
        return;
    }

    if !matches!(
        machine.state,
        State::Working { .. }
            | State::Warning { .. }
            | State::Prompt { .. }
            | State::Snoozed { .. }
    ) {
        return;
    }

    // Asking for a break always gets you the overlay, even under a profile that
    // would never have escalated there on its own: `max_escalation` caps what
    // the app does *to* you, not what you ask for.
    let required =
        forced_session_break(machine, policy).unwrap_or_else(|| policy.profile.break_length());
    begin_break(machine, required, effects);
}

fn on_snooze(machine: &mut Machine, now: Timestamp, policy: &Policy, effects: &mut Vec<Effect>) {
    if !matches!(machine.state, State::Prompt { .. }) {
        return;
    }
    // A forced session break is not snoozable — that is the point of it.
    if forced_session_break(machine, policy).is_some() {
        return;
    }
    let Some(duration) = policy.profile.snooze(machine.cycle.snoozes_used as usize) else {
        return;
    };

    machine.cycle.snoozes_used = machine.cycle.snoozes_used.saturating_add(1);
    machine.state = State::Snoozed {
        until: plus(now, duration),
    };
    effects.push(Effect::HideAll);
}

fn on_match_extension(
    machine: &mut Machine,
    now: Timestamp,
    policy: &Policy,
    effects: &mut Vec<Effect>,
) {
    if !matches!(machine.state, State::Prompt { .. } | State::Warning { .. }) {
        return;
    }
    if machine.cycle.match_extension_used {
        return;
    }
    if forced_session_break(machine, policy).is_some() {
        return;
    }
    let Some(minutes) = policy.profile.match_extension_min else {
        return;
    };

    machine.cycle.match_extension_used = true;
    machine.state = State::Working {
        due: plus(now, Duration::from_secs(minutes * 60)),
    };
    effects.push(Effect::HideAll);
}

fn on_emergency_exit(
    machine: &mut Machine,
    now: Timestamp,
    policy: &Policy,
    effects: &mut Vec<Effect>,
) {
    if !matches!(machine.state, State::Break { .. }) {
        return;
    }
    finish_cycle(machine, now, policy, StatEvent::BreakSkipped, effects);
    effects.push(Effect::Log(StatEvent::EscapeHatchUsed));
}

fn on_context_changed(
    machine: &mut Machine,
    context: Context,
    now: Timestamp,
    policy: &Policy,
    effects: &mut Vec<Effect>,
) {
    machine.context = context;

    if context.holds_escalation() {
        // A break already under way, or one waiting to be dismissed, is left
        // alone; everything else is held.
        if matches!(
            machine.state,
            State::Break { .. }
                | State::Done { .. }
                | State::Suspended { .. }
                | State::Paused { .. }
        ) {
            return;
        }
        let previous = machine.state.clone();
        machine.state = State::Suspended {
            resume_to: Box::new(previous),
        };
        effects.push(Effect::HideAll);
    } else if let State::Suspended { resume_to } = machine.state.clone() {
        resume(machine, *resume_to, now, policy, effects);
    }
}

fn on_system_resumed(
    machine: &mut Machine,
    now: Timestamp,
    policy: &Policy,
    effects: &mut Vec<Effect>,
) {
    let slept = machine
        .last_tick
        .map(|last| since(now, last))
        .unwrap_or_default();
    machine.last_tick = Some(now);

    if slept >= policy.profile.break_length() {
        finish_cycle(machine, now, policy, StatEvent::NaturalBreak, effects);
    }
}

fn on_pause_toggled(
    machine: &mut Machine,
    now: Timestamp,
    policy: &Policy,
    effects: &mut Vec<Effect>,
) {
    match machine.state.clone() {
        State::Paused { resume_to } => resume(machine, *resume_to, now, policy, effects),
        // Pausing out of a break would be an escape hatch without the friction,
        // and a finished one is waiting on a click, not on the clock.
        State::Break { .. } | State::Done { .. } => {}
        previous => {
            machine.state = State::Paused {
                resume_to: Box::new(previous),
            };
            effects.push(Effect::HideAll);
        }
    }
}

/// Puts back a state that was held by a call or a manual pause.
fn resume(
    machine: &mut Machine,
    previous: State,
    now: Timestamp,
    policy: &Policy,
    effects: &mut Vec<Effect>,
) {
    let overdue = match &previous {
        State::Working { due } | State::Warning { due } => now >= *due,
        State::Snoozed { until } => now >= *until,
        State::Prompt { .. } => true,
        _ => false,
    };

    if overdue {
        enter_prompt(machine, now, policy, effects);
    } else {
        machine.state = previous;
    }
}

fn enter_prompt(machine: &mut Machine, now: Timestamp, policy: &Policy, effects: &mut Vec<Effect>) {
    machine.state = State::Prompt { since: now };

    // A profile capped at toast never opens a window at all.
    if ceiling(machine, policy).rank() >= Escalation::Popup.rank() {
        effects.push(Effect::ShowPopup);
        effects.push(Effect::PlaySound);
    } else {
        effects.push(Effect::ShowToast);
    }
}

/// The break has been served. The overlay stays up until it is dismissed.
fn complete_break(machine: &mut Machine, now: Timestamp, effects: &mut Vec<Effect>) {
    machine.cycle.snoozes_used = 0;
    machine.cycle.match_extension_used = false;
    machine.cycle.session_used = Duration::ZERO;

    machine.state = State::Done { since: now };
    // Deliberately no HideAll: the overlay is how the user finds out.
    effects.push(Effect::Log(StatEvent::BreakTaken));
}

fn begin_break(machine: &mut Machine, required: Duration, effects: &mut Vec<Effect>) {
    machine.state = State::Break {
        required,
        earned: Duration::ZERO,
    };
    // Clear the popup first. Leaving it up behind the overlay strands a window
    // whose buttons the machine will refuse, which reads as the app being
    // broken rather than as having moved on.
    effects.push(Effect::HideAll);
    effects.push(Effect::ShowOverlay);
}

fn finish_cycle(
    machine: &mut Machine,
    now: Timestamp,
    policy: &Policy,
    stat: StatEvent,
    effects: &mut Vec<Effect>,
) {
    machine.cycle.snoozes_used = 0;
    machine.cycle.match_extension_used = false;

    // Only a break that actually happened buys back session time.
    if matches!(stat, StatEvent::BreakTaken | StatEvent::NaturalBreak) {
        machine.cycle.session_used = Duration::ZERO;
    }

    machine.state = State::Working {
        due: plus(now, policy.profile.interval()),
    };
    effects.push(Effect::HideAll);
    effects.push(Effect::Log(stat));
}

/// The long break the session cap owes us, if it is due.
fn forced_session_break(machine: &Machine, policy: &Policy) -> Option<Duration> {
    let limit = policy.profile.session_limit_min?;
    let length = policy.profile.session_break_min?;
    if machine.cycle.session_used >= Duration::from_secs(limit * 60) {
        Some(Duration::from_secs(length * 60))
    } else {
        None
    }
}

/// How far escalation may go right now.
fn ceiling(machine: &Machine, policy: &Policy) -> Escalation {
    if machine.context == Context::Game {
        if let Some(in_game) = policy.profile.in_game_escalation {
            return in_game;
        }
    }
    policy.profile.max_escalation
}

fn since(now: Timestamp, earlier: Timestamp) -> Duration {
    now.duration_since(earlier).unwrap_or_default()
}

fn plus(instant: Timestamp, duration: Duration) -> Timestamp {
    instant.checked_add(duration).unwrap_or(instant)
}

fn minus(instant: Timestamp, duration: Duration) -> Timestamp {
    instant.checked_sub(duration).unwrap_or(instant)
}
