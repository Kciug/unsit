//! The cases the plan calls out, driven by a fake clock and a fake idle source.
//!
//! Time is advanced one second at a time rather than jumped, because a jump is
//! itself meaningful to the machine — a gap longer than a break is how sleep is
//! detected, so leaping forward would trip that instead of whatever is under
//! test.

use super::*;
use crate::config::Config;

const SECOND: Duration = Duration::from_secs(1);

fn minutes(count: u64) -> Duration {
    Duration::from_secs(count * 60)
}

fn t0() -> Timestamp {
    plus(SystemTime::UNIX_EPOCH, Duration::from_secs(1_700_000_000))
}

fn profile(name: &str) -> Profile {
    Config::default()
        .profiles
        .remove(name)
        .expect("profile exists in the defaults")
}

fn policy(profile: &Profile) -> Policy<'_> {
    Policy {
        profile,
        idle_threshold: Duration::from_secs(5),
    }
}

/// Ticks once a second across `span`, collecting everything that came out.
fn advance(
    mut machine: Machine,
    from: Timestamp,
    span: Duration,
    idle: Duration,
    policy: &Policy,
) -> (Machine, Vec<Effect>, Timestamp) {
    let mut collected = Vec::new();
    let mut now = from;
    let end = plus(from, span);

    while now < end {
        now = plus(now, SECOND);
        let (next, mut produced) = step(machine, Event::Tick, now, idle, policy);
        machine = next;
        collected.append(&mut produced);
    }

    (machine, collected, now)
}

/// Runs the clock out to the first popup.
fn to_prompt(policy: &Policy) -> (Machine, Timestamp) {
    let start = t0();
    let machine = Machine::new(start, policy.profile);
    let span = policy.profile.interval() + SECOND;
    // Idle zero throughout: the user is right there, so nothing counts as a
    // natural break on the way.
    let (machine, _, now) = advance(machine, start, span, Duration::ZERO, policy);
    assert!(
        matches!(machine.state, State::Prompt { .. }),
        "expected a prompt, got {:?}",
        machine.state
    );
    (machine, now)
}

fn earned(machine: &Machine) -> Duration {
    match machine.state {
        State::Break { earned, .. } => earned,
        ref other => panic!("expected a break, got {other:?}"),
    }
}

fn required(machine: &Machine) -> Duration {
    match machine.state {
        State::Break { required, .. } => required,
        ref other => panic!("expected a break, got {other:?}"),
    }
}

#[test]
fn input_during_a_break_holds_the_counter_and_letting_go_resumes_it() {
    let profile = profile("work");
    let policy = policy(&profile);
    let (machine, now) = to_prompt(&policy);

    let (machine, _) = step(machine, Event::Accept, now, Duration::ZERO, &policy);

    // A minute away from the keyboard: the counter moves.
    let (machine, _, now) = advance(machine, now, minutes(1), Duration::from_secs(10), &policy);
    assert_eq!(earned(&machine), minutes(1));

    // A minute of typing: it does not.
    let (machine, _, now) = advance(machine, now, minutes(1), Duration::ZERO, &policy);
    assert_eq!(earned(&machine), minutes(1), "input must hold the counter");

    // Step away again and it picks up where it stopped.
    let (machine, _, _) = advance(machine, now, minutes(1), Duration::from_secs(10), &policy);
    assert_eq!(earned(&machine), minutes(2));
}

#[test]
fn a_break_ends_once_enough_time_is_earned() {
    let profile = profile("work");
    let policy = policy(&profile);
    let (machine, now) = to_prompt(&policy);

    let (machine, _) = step(machine, Event::Accept, now, Duration::ZERO, &policy);
    let span = profile.break_length() + SECOND;
    let (machine, effects, _) = advance(machine, now, span, Duration::from_secs(10), &policy);

    assert!(effects.contains(&Effect::Log(StatEvent::BreakTaken)));
    assert!(matches!(machine.state, State::Working { .. }));
}

#[test]
fn walking_away_while_working_counts_as_a_break_and_resets_the_cycle() {
    let profile = profile("work");
    let policy = policy(&profile);
    let start = t0();

    let mut machine = Machine::new(start, &profile);
    machine.cycle.snoozes_used = 1;

    // Idle for a whole break length while the schedule still has time to run.
    let (machine, effects, _) = advance(machine, start, SECOND * 2, profile.break_length(), &policy);

    assert!(effects.contains(&Effect::Log(StatEvent::NaturalBreak)));
    assert!(matches!(machine.state, State::Working { .. }));
    assert_eq!(machine.cycle.snoozes_used, 0, "the cycle should have reset");
}

#[test]
fn the_warning_toast_lands_before_the_popup() {
    let profile = profile("work");
    let policy = policy(&profile);
    let start = t0();
    let machine = Machine::new(start, &profile);

    let span = profile.interval() - profile.warning() + SECOND;
    let (machine, effects, _) = advance(machine, start, span, Duration::ZERO, &policy);

    assert!(effects.contains(&Effect::ShowToast));
    assert!(!effects.contains(&Effect::ShowPopup), "too early for the popup");
    assert!(matches!(machine.state, State::Warning { .. }));
}

#[test]
fn an_ignored_popup_becomes_an_overlay_once_the_snoozes_are_gone() {
    let profile = profile("work");
    let policy = policy(&profile);
    let (machine, now) = to_prompt(&policy);

    // Burn the first snooze, wait it out, burn the second.
    let (machine, _) = step(machine, Event::Snooze, now, Duration::ZERO, &policy);
    let (machine, _, now) = advance(machine, now, minutes(5) + SECOND, Duration::ZERO, &policy);
    let (machine, _) = step(machine, Event::Snooze, now, Duration::ZERO, &policy);
    let (machine, _, now) = advance(machine, now, minutes(3) + SECOND, Duration::ZERO, &policy);

    assert!(matches!(machine.state, State::Prompt { .. }));
    assert_eq!(machine.snoozes_left(&profile), 0);

    // A third request is simply not honoured.
    let (machine, effects) = step(machine, Event::Snooze, now, Duration::ZERO, &policy);
    assert!(effects.is_empty());
    assert!(matches!(machine.state, State::Prompt { .. }));

    // Ignore it long enough and the overlay takes over by itself.
    let span = profile.prompt_timeout() + SECOND;
    let (machine, effects, _) = advance(machine, now, span, Duration::ZERO, &policy);
    assert!(effects.contains(&Effect::ShowOverlay));
    assert!(matches!(machine.state, State::Break { .. }));

    // And the popup goes with it. Leaving it on screen strands a window whose
    // buttons the machine will refuse, which looks exactly like a broken app.
    let hide = effects.iter().position(|effect| *effect == Effect::HideAll);
    let overlay = effects.iter().position(|effect| *effect == Effect::ShowOverlay);
    assert!(hide.is_some(), "the popup must be dismissed: {effects:?}");
    assert!(hide < overlay, "hide before showing, or the overlay flickers");
}

#[test]
fn waking_after_two_hours_asleep_counts_as_a_break() {
    let profile = profile("work");
    let policy = policy(&profile);
    let start = t0();

    // One tick so the machine knows when it last saw the world.
    let machine = Machine::new(start, &profile);
    let (machine, _, now) = advance(machine, start, SECOND, Duration::ZERO, &policy);

    let woken = plus(now, minutes(120));
    let (machine, effects) = step(machine, Event::SystemResumed, woken, Duration::ZERO, &policy);

    assert!(effects.contains(&Effect::Log(StatEvent::NaturalBreak)));
    assert!(matches!(machine.state, State::Working { .. }));
}

#[test]
fn a_call_during_the_prompt_suspends_it_and_gives_it_back_afterwards() {
    let profile = profile("work");
    let policy = policy(&profile);
    let (machine, now) = to_prompt(&policy);

    let (machine, effects) = step(
        machine,
        Event::ContextChanged(Context::Call),
        now,
        Duration::ZERO,
        &policy,
    );
    assert!(matches!(machine.state, State::Suspended { .. }));
    assert!(effects.contains(&Effect::HideAll), "no windows during a call");

    // The call runs long; nothing escalates while it does.
    let (machine, effects, now) = advance(machine, now, minutes(10), Duration::ZERO, &policy);
    assert!(effects.is_empty(), "a call must not be interrupted");
    assert!(matches!(machine.state, State::Suspended { .. }));

    let (machine, effects) = step(
        machine,
        Event::ContextChanged(Context::Free),
        now,
        Duration::ZERO,
        &policy,
    );
    assert!(matches!(machine.state, State::Prompt { .. }));
    assert!(effects.contains(&Effect::ShowPopup));
}

#[test]
fn finishing_the_match_is_offered_exactly_once_per_cycle() {
    let profile = profile("gaming");
    let policy = policy(&profile);
    let (machine, now) = to_prompt(&policy);

    assert!(machine.match_extension_available(&profile));
    let (machine, effects) = step(machine, Event::MatchExtension, now, Duration::ZERO, &policy);
    assert!(effects.contains(&Effect::HideAll));
    assert!(matches!(machine.state, State::Working { .. }));
    assert!(!machine.match_extension_available(&profile));

    // Fifteen minutes later the popup is back.
    let (machine, _, now) = advance(machine, now, minutes(15) + SECOND, Duration::ZERO, &policy);
    assert!(matches!(machine.state, State::Prompt { .. }));

    // And this time the extension is not on the table.
    let (machine, effects) = step(machine, Event::MatchExtension, now, Duration::ZERO, &policy);
    assert!(effects.is_empty());
    assert!(matches!(machine.state, State::Prompt { .. }));
}

#[test]
fn the_session_cap_forces_a_long_break_that_cannot_be_snoozed() {
    let profile = profile("gaming");
    let policy = policy(&profile);
    let start = t0();

    // Three hours of gaming already on the clock.
    let mut machine = Machine::new(start, &profile);
    machine.cycle.session_used = minutes(180);

    let (machine, effects, _) = advance(machine, start, SECOND * 2, Duration::ZERO, &policy);

    assert!(effects.contains(&Effect::ShowOverlay));
    assert_eq!(
        required(&machine),
        minutes(15),
        "the cap owes a long break, not the usual short one"
    );

    // Snoozing out of it is not on offer, cap or no cap.
    let (machine, effects) = step(machine, Event::Snooze, start, Duration::ZERO, &policy);
    assert!(effects.is_empty());
    assert!(matches!(machine.state, State::Break { .. }));
}

#[test]
fn the_escape_hatch_logs_a_skip_and_does_not_buy_back_session_time() {
    let profile = profile("gaming");
    let policy = policy(&profile);
    let (machine, now) = to_prompt(&policy);

    let (mut machine, _) = step(machine, Event::Accept, now, Duration::ZERO, &policy);
    machine.cycle.session_used = minutes(100);

    let (machine, effects) = step(machine, Event::EmergencyExit, now, Duration::ZERO, &policy);

    assert!(effects.contains(&Effect::Log(StatEvent::BreakSkipped)));
    assert!(effects.contains(&Effect::Log(StatEvent::EscapeHatchUsed)));
    assert!(matches!(machine.state, State::Working { .. }));
    assert_eq!(
        machine.cycle.session_used,
        minutes(100),
        "skipping a break must not reset the session cap, or the cap means nothing"
    );
}

#[test]
fn a_taken_break_does_buy_back_session_time() {
    let profile = profile("gaming");
    let policy = policy(&profile);
    let (machine, now) = to_prompt(&policy);

    let (mut machine, _) = step(machine, Event::Accept, now, Duration::ZERO, &policy);
    machine.cycle.session_used = minutes(100);

    // Exactly the break length, so the run ends on the tick that completes it.
    // A second more and the machine would already be working again, putting a
    // second back on the session clock.
    let (machine, _, _) = advance(machine, now, profile.break_length(), Duration::from_secs(10), &policy);

    assert!(matches!(machine.state, State::Working { .. }));
    assert_eq!(machine.cycle.session_used, Duration::ZERO);
}

#[test]
fn a_game_in_the_foreground_keeps_the_popup_down_to_a_toast() {
    let profile = profile("gaming");
    let policy = policy(&profile);
    let start = t0();

    let machine = Machine::new(start, &profile);
    let (machine, _) = step(
        machine,
        Event::ContextChanged(Context::Game),
        start,
        Duration::ZERO,
        &policy,
    );

    let span = profile.interval() + SECOND;
    let (machine, effects, _) = advance(machine, start, span, Duration::ZERO, &policy);

    assert!(matches!(machine.state, State::Prompt { .. }));
    assert!(
        !effects.contains(&Effect::ShowPopup),
        "no window over a running game"
    );
    assert!(effects.contains(&Effect::ShowToast));
}
