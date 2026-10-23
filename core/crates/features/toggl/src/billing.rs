//! The three billing models as arithmetic over a session's real duration.
//!
//! - Hourly: the session is billable time, rounded up to the workspace's
//!   increment; a task can be marked not billable.
//! - Fixed price per task: the estimate is the quote and the cap. What fits
//!   under the cap is billable; the rest is an unpaid overrun, sent as a
//!   separate entry Toggl marks not billable.
//! - FTE: sessions stay private and send nothing; the flat week does.

use std::time::Duration;

use vault_base::{BillingModel, BillingSettings};

/// How one session of a workspace is billed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BillingPolicy {
    /// Hourly billing.
    Hourly {
        /// The increment billable time rounds up to, at least one minute.
        rounding_minutes: u32,
        /// Whether the task is billable.
        is_billable: bool,
    },
    /// Fixed price per task.
    Fixed {
        /// The task's estimate: the quote and the cap; `None` quotes nothing.
        estimate: Option<Duration>,
    },
    /// Full-time equivalent: sessions are never sent.
    Fte,
}

/// One entry a session becomes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BilledPart {
    /// Added to the session id to key the entry: `None` for the main part,
    /// `Some("overrun")` for a fixed-price overrun.
    pub suffix: Option<&'static str>,
    /// How far after the session's start the part starts.
    pub offset: Duration,
    /// How long it is.
    pub duration: Duration,
    /// Whether Toggl marks it billable.
    pub is_billable: bool,
}

impl BillingPolicy {
    /// The policy for a session of a workspace with `billing`: `None` when the
    /// workspace has no billing model. `task_billable` is the task's own flag
    /// (hourly), `estimate` the task's estimate (fixed price).
    #[must_use]
    pub fn for_workspace(
        billing: Option<&BillingSettings>,
        task_billable: Option<bool>,
        estimate: Option<Duration>,
    ) -> Option<BillingPolicy> {
        let billing = billing?;
        Some(match billing.model {
            BillingModel::Hourly => BillingPolicy::Hourly {
                rounding_minutes: billing.rounding_minutes_or_default(),
                is_billable: task_billable.unwrap_or(billing.billable_default_or_default()),
            },
            BillingModel::Fixed => BillingPolicy::Fixed { estimate },
            BillingModel::Fte => BillingPolicy::Fte,
        })
    }

    /// Whether sessions under this policy become entries.
    #[must_use]
    pub fn sends_sessions(&self) -> bool {
        !matches!(self, BillingPolicy::Fte)
    }
}

/// The entries a session of `actual` length becomes. For a fixed-price task
/// `already_billed` is what earlier sessions of the task billed under its cap.
#[must_use]
pub fn billed_parts(
    policy: &BillingPolicy,
    actual: Duration,
    already_billed: Duration,
) -> Vec<BilledPart> {
    let actual = Duration::from_secs(actual.as_secs());
    if actual.is_zero() {
        return Vec::new();
    }
    match *policy {
        BillingPolicy::Hourly {
            rounding_minutes,
            is_billable,
        } => vec![BilledPart {
            suffix: None,
            offset: Duration::ZERO,
            duration: round_up(actual, rounding_minutes),
            is_billable,
        }],
        BillingPolicy::Fixed { estimate } => {
            let remaining = estimate
                .unwrap_or(Duration::ZERO)
                .saturating_sub(already_billed);
            let within = actual.min(remaining);
            let overrun = actual.saturating_sub(within);
            let mut parts = Vec::new();
            if !within.is_zero() {
                parts.push(BilledPart {
                    suffix: None,
                    offset: Duration::ZERO,
                    duration: within,
                    is_billable: true,
                });
            }
            if !overrun.is_zero() {
                parts.push(BilledPart {
                    suffix: Some("overrun"),
                    offset: within,
                    duration: overrun,
                    is_billable: false,
                });
            }
            parts
        }
        BillingPolicy::Fte => Vec::new(),
    }
}

/// `duration` rounded up to a whole number of `minutes` increments.
fn round_up(duration: Duration, minutes: u32) -> Duration {
    let step = u64::from(minutes.max(1)) * 60;
    let seconds = duration.as_secs();
    Duration::from_secs(seconds.div_ceil(step).saturating_mul(step))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINUTE: Duration = Duration::from_secs(60);

    fn hourly(rounding_minutes: u32) -> BillingPolicy {
        BillingPolicy::Hourly {
            rounding_minutes,
            is_billable: true,
        }
    }

    #[test]
    fn hourly_rounds_up_to_the_increment() {
        for (actual, rounding, billed) in [
            (1, 15, 15 * 60),
            (15 * 60, 15, 15 * 60),
            (15 * 60 + 1, 15, 30 * 60),
            (61, 1, 120),
            (7 * 60, 6, 12 * 60),
        ] {
            let parts = billed_parts(
                &hourly(rounding),
                Duration::from_secs(actual),
                Duration::ZERO,
            );
            assert_eq!(parts.len(), 1);
            assert_eq!(
                parts[0].duration,
                Duration::from_secs(billed),
                "{actual}s by {rounding}"
            );
        }
        assert!(billed_parts(&hourly(15), Duration::ZERO, Duration::ZERO).is_empty());
        let not_billable = BillingPolicy::Hourly {
            rounding_minutes: 15,
            is_billable: false,
        };
        assert!(!billed_parts(&not_billable, MINUTE, Duration::ZERO)[0].is_billable);
    }

    #[test]
    fn fixed_bills_up_to_the_estimate_and_sends_the_overrun_unpaid() {
        let policy = BillingPolicy::Fixed {
            estimate: Some(60 * MINUTE),
        };
        let under = billed_parts(&policy, 40 * MINUTE, Duration::ZERO);
        assert_eq!(under.len(), 1);
        assert_eq!(
            (under[0].duration, under[0].is_billable),
            (40 * MINUTE, true)
        );

        let across = billed_parts(&policy, 40 * MINUTE, 40 * MINUTE);
        assert_eq!(
            across,
            [
                BilledPart {
                    suffix: None,
                    offset: Duration::ZERO,
                    duration: 20 * MINUTE,
                    is_billable: true,
                },
                BilledPart {
                    suffix: Some("overrun"),
                    offset: 20 * MINUTE,
                    duration: 20 * MINUTE,
                    is_billable: false,
                },
            ]
        );
        let over = billed_parts(&policy, 10 * MINUTE, 60 * MINUTE);
        assert_eq!(over.len(), 1);
        assert_eq!(
            (over[0].suffix, over[0].is_billable),
            (Some("overrun"), false)
        );
        let no_quote = billed_parts(
            &BillingPolicy::Fixed { estimate: None },
            MINUTE,
            Duration::ZERO,
        );
        assert!(!no_quote[0].is_billable);
    }

    #[test]
    fn fte_sends_no_session_and_no_block_means_no_policy() {
        assert!(billed_parts(&BillingPolicy::Fte, 60 * MINUTE, Duration::ZERO).is_empty());
        assert!(!BillingPolicy::Fte.sends_sessions());
        assert_eq!(BillingPolicy::for_workspace(None, None, None), None);
        let mut block = BillingSettings::new(BillingModel::Hourly);
        block.billable_default = Some(false);
        assert_eq!(
            BillingPolicy::for_workspace(Some(&block), None, None),
            Some(BillingPolicy::Hourly {
                rounding_minutes: 15,
                is_billable: false
            })
        );
        assert_eq!(
            BillingPolicy::for_workspace(Some(&block), Some(true), None),
            Some(BillingPolicy::Hourly {
                rounding_minutes: 15,
                is_billable: true
            })
        );
    }
}
