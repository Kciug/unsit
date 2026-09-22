//! TOML configuration from `%APPDATA%\Unsit\config.toml`.

use std::time::Duration;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Escalation {
    Toast,
    Popup,
    Overlay,
    Lock,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    pub interval: Duration,
    pub break_length: Duration,
    pub warning: Duration,
    /// Snooze lengths in order. Empty means no snoozing.
    pub snoozes: Vec<Duration>,
    pub max_escalation: Escalation,
}
