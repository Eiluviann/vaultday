//! The billing maths every billed figure is computed with: the hourly
//! rounding, a task's `[billable:: …]` flag and the duration a quote or a
//! logged session is written as. Toggl's entries and the report bundle's
//! hours both call these, so the two cannot disagree.

use std::time::Duration;

use crate::quick_add::parse_estimate;

/// `duration` rounded up to a whole number of `increment_minutes`; an
/// increment of zero counts as one minute.
#[must_use]
pub fn round_up_to_increment(duration: Duration, increment_minutes: u32) -> Duration {
    let step = u64::from(increment_minutes.max(1)) * 60;
    Duration::from_secs(duration.as_secs().div_ceil(step).saturating_mul(step))
}

/// Whole minutes rounded up to a whole number of `increment_minutes`.
#[must_use]
pub fn round_up_minutes(minutes: u64, increment_minutes: u32) -> u64 {
    round_up_to_increment(
        Duration::from_secs(minutes.saturating_mul(60)),
        increment_minutes,
    )
    .as_secs()
        / 60
}

/// A task's `[billable:: …]` value: `true`/`yes` or `false`/`no`, any case;
/// `None` for anything else, which leaves the workspace default in force.
#[must_use]
pub fn parse_billable_flag(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "yes" => Some(true),
        "false" | "no" => Some(false),
        _ => None,
    }
}

/// Minutes in a quote or a logged duration: `45m`, `2h`, `1h 5m`, `1h30`,
/// `1.5h`, spaces ignored. `None` for anything else.
#[must_use]
pub fn parse_billing_minutes(text: &str) -> Option<u64> {
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    parse_estimate(&compact).map(|estimate| estimate.as_duration().as_secs() / 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounding_goes_up_to_the_increment_in_seconds_and_in_minutes() {
        assert_eq!(
            round_up_to_increment(Duration::from_secs(1), 15),
            Duration::from_secs(900)
        );
        assert_eq!(
            round_up_to_increment(Duration::from_secs(901), 15),
            Duration::from_secs(1800)
        );
        assert_eq!(round_up_to_increment(Duration::ZERO, 15), Duration::ZERO);
        assert_eq!(
            round_up_to_increment(Duration::from_secs(61), 0),
            Duration::from_secs(120)
        );
        assert_eq!(round_up_minutes(25, 30), 30);
        assert_eq!(round_up_minutes(30, 30), 30);
        assert_eq!(round_up_minutes(50, 30), 60);
        assert_eq!(round_up_minutes(7, 6), 12);
        assert_eq!(round_up_minutes(u64::MAX, 15) % 15, 0);
    }

    #[test]
    fn the_billable_flag_reads_true_yes_false_no_and_nothing_else() {
        for (text, flag) in [
            ("true", Some(true)),
            (" Yes ", Some(true)),
            ("FALSE", Some(false)),
            ("no", Some(false)),
            ("maybe", None),
            ("", None),
        ] {
            assert_eq!(parse_billable_flag(text), flag, "{text:?}");
        }
    }

    #[test]
    fn billing_minutes_read_quotes_and_logged_durations() {
        for (text, minutes) in [
            ("45m", Some(45)),
            ("45min", Some(45)),
            ("2h", Some(120)),
            ("1h 5m", Some(65)),
            ("1h30", Some(90)),
            ("1.5h", Some(90)),
            ("1h 75m", None),
            ("soon", None),
            ("", None),
        ] {
            assert_eq!(parse_billing_minutes(text), minutes, "{text:?}");
        }
    }
}
