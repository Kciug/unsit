//! Append-only JSONL event log. SQLite only if queries ever start to hurt.

use crate::engine::StatEvent;

pub fn record(_event: StatEvent) {
    todo!("stats logging")
}
