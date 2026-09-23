//! TOML configuration from `%APPDATA%\Unsit\config.toml`.
//!
//! Until the settings UI lands in v0.3 this file is edited by hand, so the
//! representation is chosen for a human with a text editor rather than for
//! serde's convenience: durations are plain integers named by their unit.
//! Deriving `Serialize` on `std::time::Duration` instead would turn every
//! single field into a `{ secs, nanos }` table.

use std::collections::BTreeMap;
use std::io;
use std::path::PathBuf;
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

impl Escalation {
    /// Position on the ladder, so a profile ceiling can be compared against it.
    pub fn rank(self) -> u8 {
        match self {
            Escalation::Toast => 0,
            Escalation::Popup => 1,
            Escalation::Overlay => 2,
            Escalation::Lock => 3,
        }
    }
}

/// How much it costs to walk out of a break early.
///
/// `Type` — retyping a random sentence — is not built yet and currently falls
/// back to holding, so the way out always exists and the button never promises
/// something it will not do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EscapeMethod {
    Hold,
    Type,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Locale {
    En,
    Pl,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    pub interval_min: u64,
    pub break_min: u64,
    #[serde(default = "default_warning_sec")]
    pub warning_sec: u64,
    /// How long an ignored popup waits before the overlay takes over.
    #[serde(default = "default_prompt_timeout_sec")]
    pub prompt_timeout_sec: u64,
    /// Snooze lengths in order. Empty means no snoozing.
    #[serde(default)]
    pub snoozes_min: Vec<u64>,
    pub max_escalation: Escalation,

    /// "Finish the match", once per cycle. `None` outside Gaming.
    #[serde(default)]
    pub match_extension_min: Option<u64>,
    /// Escalation ceiling while a game is actually in the foreground.
    #[serde(default)]
    pub in_game_escalation: Option<Escalation>,
    /// Total time before a long break is forced regardless of snoozes.
    #[serde(default)]
    pub session_limit_min: Option<u64>,
    #[serde(default)]
    pub session_break_min: Option<u64>,
}

impl Profile {
    pub fn interval(&self) -> Duration {
        Duration::from_secs(self.interval_min * 60)
    }

    pub fn break_length(&self) -> Duration {
        Duration::from_secs(self.break_min * 60)
    }

    pub fn warning(&self) -> Duration {
        Duration::from_secs(self.warning_sec)
    }

    pub fn prompt_timeout(&self) -> Duration {
        Duration::from_secs(self.prompt_timeout_sec)
    }

    /// Snooze the user would get with `used` snoozes already spent.
    pub fn snooze(&self, used: usize) -> Option<Duration> {
        self.snoozes_min
            .get(used)
            .map(|minutes| Duration::from_secs(minutes * 60))
    }

    pub fn snooze_count(&self) -> u8 {
        self.snoozes_min.len() as u8
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct General {
    pub default_profile: String,
    #[serde(default = "default_idle_threshold_sec")]
    pub idle_threshold_sec: u64,
    #[serde(default = "default_locale")]
    pub locale: Locale,
}

impl General {
    /// How long without input before a break is considered to be happening.
    pub fn idle_threshold(&self) -> Duration {
        Duration::from_secs(self.idle_threshold_sec)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Detection {
    #[serde(default)]
    pub games: Vec<String>,
    /// Filter only — a running process is not a call, the microphone is.
    #[serde(default)]
    pub call_apps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Escape {
    pub method: EscapeMethod,
    #[serde(default = "default_hold_sec")]
    pub hold_sec: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    pub general: General,
    pub profiles: BTreeMap<String, Profile>,
    #[serde(default)]
    pub detection: Detection,
    pub escape: Escape,
}

fn default_warning_sec() -> u64 {
    120
}

fn default_prompt_timeout_sec() -> u64 {
    60
}

fn default_idle_threshold_sec() -> u64 {
    5
}

fn default_hold_sec() -> u64 {
    5
}

fn default_locale() -> Locale {
    Locale::En
}

impl Default for Config {
    fn default() -> Self {
        let mut profiles = BTreeMap::new();

        profiles.insert(
            "work".to_owned(),
            Profile {
                interval_min: 50,
                break_min: 10,
                warning_sec: 120,
                prompt_timeout_sec: 60,
                snoozes_min: vec![5, 3],
                max_escalation: Escalation::Overlay,
                match_extension_min: None,
                in_game_escalation: None,
                session_limit_min: None,
                session_break_min: None,
            },
        );

        profiles.insert(
            "gaming".to_owned(),
            Profile {
                interval_min: 60,
                break_min: 5,
                warning_sec: 120,
                prompt_timeout_sec: 60,
                // No snoozing here on purpose — "finish the match" replaces it.
                snoozes_min: vec![],
                max_escalation: Escalation::Overlay,
                match_extension_min: Some(15),
                in_game_escalation: Some(Escalation::Toast),
                session_limit_min: Some(180),
                session_break_min: Some(15),
            },
        );

        profiles.insert(
            "chill".to_owned(),
            Profile {
                interval_min: 90,
                break_min: 5,
                warning_sec: 120,
                prompt_timeout_sec: 60,
                snoozes_min: vec![10],
                max_escalation: Escalation::Popup,
                match_extension_min: None,
                in_game_escalation: None,
                session_limit_min: None,
                session_break_min: None,
            },
        );

        Self {
            general: General {
                default_profile: "work".to_owned(),
                idle_threshold_sec: 5,
                locale: Locale::En,
            },
            profiles,
            detection: Detection::default(),
            escape: Escape {
                method: EscapeMethod::Hold,
                hold_sec: 5,
            },
        }
    }
}

impl Config {
    pub fn path() -> io::Result<PathBuf> {
        let appdata = std::env::var("APPDATA")
            .map_err(|_| io::Error::new(io::ErrorKind::NotFound, "APPDATA is not set"))?;
        Ok(PathBuf::from(appdata).join("Unsit").join("config.toml"))
    }

    /// Reads the config, writing out the defaults first if there is no file yet.
    pub fn load_or_create() -> io::Result<Self> {
        let path = Self::path()?;
        if !path.exists() {
            let config = Self::default();
            config.save()?;
            return Ok(config);
        }
        let text = std::fs::read_to_string(&path)?;
        toml::from_str(&text).map_err(io::Error::other)
    }

    pub fn save(&self) -> io::Result<()> {
        let path = Self::path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(self).map_err(io::Error::other)?;
        std::fs::write(&path, text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_round_trip_through_toml() {
        let config = Config::default();
        let text = toml::to_string_pretty(&config).expect("serialise");
        let parsed: Config = toml::from_str(&text).expect("parse");
        assert_eq!(config, parsed);
    }

    #[test]
    fn durations_are_written_as_plain_minutes() {
        // The whole point of the integer fields: this file gets hand-edited.
        let text = toml::to_string_pretty(&Config::default()).expect("serialise");
        assert!(text.contains("interval_min = 50"), "got:\n{text}");
        assert!(
            !text.contains("nanos"),
            "durations leaked into the file:\n{text}"
        );
    }

    #[test]
    fn snoozes_run_out_in_order() {
        let config = Config::default();
        let work = &config.profiles["work"];
        assert_eq!(work.snooze(0), Some(Duration::from_secs(5 * 60)));
        assert_eq!(work.snooze(1), Some(Duration::from_secs(3 * 60)));
        assert_eq!(work.snooze(2), None);
    }

    #[test]
    fn gaming_swaps_snoozes_for_the_match_extension() {
        let config = Config::default();
        let gaming = &config.profiles["gaming"];
        assert_eq!(gaming.snooze_count(), 0);
        assert_eq!(gaming.match_extension_min, Some(15));
    }
}
