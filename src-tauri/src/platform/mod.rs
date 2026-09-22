//! Everything that touches Win32 lives here, behind traits the engine can fake.
//!
//! Game detection is limited to the process list, `SHQueryUserNotificationState`
//! and foreground-window geometry. No injection and no hooking, ever — that is
//! what keeps anti-cheat out of the picture.

use std::time::Duration;

use crate::engine::{Context, Timestamp};

pub trait Clock {
    fn now(&self) -> Timestamp;
}

pub trait IdleSource {
    /// Time since the last input, via `GetLastInputInfo`.
    ///
    /// Known gap: gamepads are invisible to it, so controller input does not
    /// register as activity.
    fn idle_for(&self) -> Duration;
}

pub trait ContextSource {
    fn current(&self) -> Context;
}
