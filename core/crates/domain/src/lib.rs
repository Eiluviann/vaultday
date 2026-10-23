//! Domain rules of Vaultday: tasks, the day plan, timer sessions and estimate
//! maths.
//!
//! The crate is pure. It does no IO, parses no vault and knows no platform
//! type: vault-side task fields arrive as plain values ([`task::VaultTask`]),
//! the clock and the notifier are ports ([`ports`]) and every rule is a
//! function over values. Billing models, rates and rounding are not here: they
//! belong to the Toggl and billing feature set after v1, and sessions are
//! stored raw so that feature can round them later.
//!
//! The three time types stay apart as distinct newtypes in [`estimate`]:
//! [`estimate::SpentTime`] (real private time from sessions),
//! [`estimate::LoggedTime`] (time allocated to a story per day) and
//! [`estimate::Estimate`] (the budget). They cannot be added to each other.
//!
//! [`scheduler`] lays unlocked time boxes into the free slots of a working day,
//! [`calendar_day`] lays out the side panel's day grid from events and boxes,
//! [`quick_add`] turns one typed line into a [`quick_add::ParsedEntry`] for the
//! confirmation popover, and [`registry`] is the feature registry with nothing
//! plugged in: the base version has no feature set, so it only defines the
//! manifest, the slots and the enablement rules later feature sets use.
//! [`week`] holds the close-out's periods and their working days, and
//! [`allocation`] spreads a day's logged hours over the stories worked.
//! [`ticket`] is the generic ticket provider every workspace gets from its
//! settings. [`today_list`] decides which tasks a day's list holds and in
//! what order. [`browse`] reads the typed query line Browse filters tasks with.

// The workspace lints forbid `unwrap`/`expect` in shipped code; tests may use them.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

// Declares a string newtype used as an identifier. Textually before the modules so they all see it.
macro_rules! id_type {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            /// Wraps the identifier text.
            pub fn new(id: impl Into<String>) -> Self {
                Self(id.into())
            }

            /// The identifier text.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl ::core::fmt::Display for $name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl From<&str> for $name {
            fn from(id: &str) -> Self {
                Self::new(id)
            }
        }

        impl From<String> for $name {
            fn from(id: String) -> Self {
                Self(id)
            }
        }
    };
}

pub mod allocation;
pub mod billing;
pub mod browse;
pub mod calendar_day;
pub mod day_plan;
pub mod error;
pub mod estimate;
pub mod ports;
pub mod quick_add;
pub mod registry;
pub mod scheduler;
pub mod session;
pub mod task;
pub mod ticket;
pub mod today_list;
pub mod week;
pub mod workspace;

pub use error::{DomainError, Result};
