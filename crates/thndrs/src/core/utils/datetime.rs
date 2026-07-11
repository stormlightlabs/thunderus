/// Get the current time as an ISO 8601 UTC string.
///
/// Format: `YYYY-MM-DDTHH:MM:SSZ`.
pub fn now_iso8601() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let days = secs / 86_400;
    let remainder = secs % 86_400;
    let hour = remainder / 3600;
    let minute = (remainder % 3600) / 60;
    let second = remainder % 60;

    format!("{}T{hour:02}:{minute:02}:{second:02}Z", date_from_days(days))
}

/// Convert days since Unix epoch to a YYYY-MM-DD string.
///
/// Uses the Howard Hinnant algorithm for date calculation.
pub fn date_from_days(days: u64) -> String {
    let z = days as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

/// Get the current date rounded to the day (YYYY-MM-DD) for cache stability.
///
/// The exact timestamp stays in session JSONL metadata when needed for audit.
pub fn rounded_date() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    date_from_days(now / 86_400)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_from_days_known_values() {
        assert_eq!(date_from_days(0), "1970-01-01");
        let d = date_from_days(20_745);
        assert!(d.starts_with("2026"), "day 20745 should be in 2026, got {d}");
    }
}
