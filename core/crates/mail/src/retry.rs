//! Bounded retries with exponential backoff for transient failures.

use std::time::Duration;

use crate::error::MailError;

/// How often and how patiently a transient failure is retried.
#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    /// Attempts in all, the first included; at least one.
    pub attempts: u32,
    /// The wait before the first retry; it doubles each time.
    pub base_delay: Duration,
    /// The longest wait, a server's `Retry-After` included.
    pub max_delay: Duration,
    /// How to wait; tests pass a function that returns at once.
    pub sleep: fn(Duration),
}

impl Default for RetryPolicy {
    fn default() -> Self {
        RetryPolicy {
            attempts: 3,
            base_delay: Duration::from_secs(2),
            max_delay: Duration::from_secs(60),
            sleep: std::thread::sleep,
        }
    }
}

impl RetryPolicy {
    /// Runs `operation` until it succeeds, fails with a non-transient error,
    /// or has used every attempt; the last error is returned.
    ///
    /// # Errors
    /// The last error `operation` returned.
    pub fn run<T>(
        &self,
        mut operation: impl FnMut() -> Result<T, MailError>,
    ) -> Result<T, MailError> {
        let attempts = self.attempts.max(1);
        let mut attempt = 1;
        loop {
            match operation() {
                Err(error) if error.is_transient() && attempt < attempts => {
                    (self.sleep)(self.delay(attempt, error.retry_after()));
                    attempt += 1;
                }
                outcome => return outcome,
            }
        }
    }

    fn delay(&self, attempt: u32, asked: Option<Duration>) -> Duration {
        let backoff = self
            .base_delay
            .saturating_mul(2_u32.saturating_pow(attempt.saturating_sub(1)));
        asked.unwrap_or(backoff).min(self.max_delay)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn instant() -> RetryPolicy {
        RetryPolicy {
            sleep: |_| {},
            ..RetryPolicy::default()
        }
    }

    #[test]
    fn transient_failures_are_retried_until_the_attempts_run_out() {
        let calls = Cell::new(0);
        let outcome: Result<(), MailError> = instant().run(|| {
            calls.set(calls.get() + 1);
            Err(MailError::network("reset"))
        });
        assert!(outcome.is_err());
        assert_eq!(calls.get(), 3);

        let calls = Cell::new(0);
        let outcome = instant().run(|| {
            calls.set(calls.get() + 1);
            if calls.get() < 2 {
                Err(MailError::network("reset"))
            } else {
                Ok(7)
            }
        });
        assert_eq!(outcome.unwrap(), 7);
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn other_failures_are_not_retried() {
        let calls = Cell::new(0);
        let outcome: Result<(), MailError> = instant().run(|| {
            calls.set(calls.get() + 1);
            Err(MailError::Auth("no".to_owned()))
        });
        assert!(matches!(outcome, Err(MailError::Auth(_))));
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn delays_double_and_respect_the_server_and_the_cap() {
        let policy = RetryPolicy::default();
        assert_eq!(policy.delay(1, None), Duration::from_secs(2));
        assert_eq!(policy.delay(2, None), Duration::from_secs(4));
        assert_eq!(
            policy.delay(1, Some(Duration::from_secs(9))),
            Duration::from_secs(9)
        );
        assert_eq!(
            policy.delay(1, Some(Duration::from_secs(600))),
            Duration::from_secs(60)
        );
    }
}
