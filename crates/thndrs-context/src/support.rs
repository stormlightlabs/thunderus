//! Small standard-library helpers shared by the context and memory modules.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

/// Compute a stable hash for context and memory content.
pub(crate) fn hash_content(content: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    hasher.finish()
}

/// Compute `floor(value * ratio)` for whole-token budget calculations.
pub(crate) fn ratio_of(value: u64, ratio: f64) -> u64 {
    if value == 0 { 0 } else { ((value as f64) * ratio).floor() as u64 }
}

/// Return the number of path segments in a context scope.
pub(crate) fn scope_depth(scope: &str) -> usize {
    if scope == "." || scope.is_empty() { 0 } else { scope.matches('/').count() + 1 }
}

/// Return the current user's home directory from standard platform variables.
pub(crate) fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
}

/// Return the current UTC time in ISO-8601 form without an external clock dependency.
#[cfg(feature = "memory")]
pub(crate) fn now_iso8601() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = secs / 86_400;
    let remainder = secs % 86_400;
    let hour = remainder / 3_600;
    let minute = (remainder % 3_600) / 60;
    let second = remainder % 60;

    format!("{}T{hour:02}:{minute:02}:{second:02}Z", date_from_days(days))
}

#[cfg(feature = "memory")]
fn date_from_days(days: u64) -> String {
    let z = days as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = year + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}
