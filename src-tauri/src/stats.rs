//! Append-only JSONL event log. SQLite only if queries ever start to hurt.
//!
//! Append-only matters: a crash mid-write costs one line, never the file, and
//! a day's history stays greppable with no tooling at all.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::engine::StatEvent;

#[derive(Debug, Serialize)]
struct Record {
    /// Unix seconds, so a line stays readable without a date library.
    at: u64,
    event: &'static str,
    profile: String,
}

fn name(event: StatEvent) -> &'static str {
    match event {
        StatEvent::BreakTaken => "break_taken",
        StatEvent::BreakSkipped => "break_skipped",
        StatEvent::NaturalBreak => "natural_break",
        StatEvent::EscapeHatchUsed => "escape_hatch_used",
    }
}

pub fn path() -> Option<PathBuf> {
    let appdata = std::env::var("APPDATA").ok()?;
    Some(PathBuf::from(appdata).join("Unsit").join("stats.jsonl"))
}

/// Appends one event. Failures are swallowed on purpose: losing a statistic is
/// never a reason to interrupt what the user is doing.
pub fn record(event: StatEvent, profile: &str) {
    let Some(path) = path() else { return };

    let at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or(0);

    let record = Record {
        at,
        event: name(event),
        profile: profile.to_owned(),
    };

    let Ok(mut line) = serde_json::to_string(&record) else {
        return;
    };
    line.push('\n');

    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = file.write_all(line.as_bytes());
    }
}
