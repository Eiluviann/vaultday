//! The Toggl and billing feature set (`toggl-billing`).
//!
//! Each workspace declares a billing model in `settings.json` (hourly, fixed
//! price per task, FTE). With this feature on for the app and the workspace,
//! a stopped session of an hourly or fixed-price workspace becomes a Toggl
//! Track time entry through an offline queue kept in the workspace document,
//! and an FTE workspace sends one flat entry per working day instead.
//!
//! Every operation hangs off [`TogglFeature`], which only
//! [`TogglFeature::for_workspace`] hands out, and only while the registry says
//! the feature is on: with the feature off, no code of this crate runs.
//!
//! The HTTP client ([`HttpTogglApi`]) reads the token from the secret store at
//! call time and never keeps it. It reaches the live service only through
//! [`HttpTogglApi::live`], which refuses unless the feature is on for the
//! workspace, a token is stored and the app-level `allow_live` switch is on
//! (off by default); everything else talks to a loopback mock server.

// The workspace lints forbid `unwrap`/`expect` in shipped code; tests may use them.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod api;
pub mod billing;
pub mod entry;
pub mod error;
pub mod feature;
pub mod flat;
pub mod http;
pub mod mapping;
pub mod queue;
pub mod settings;
pub mod transport;
pub mod week;

pub use api::{NewTimeEntry, TimeEntry, TogglApi, TogglMe, TogglProject, TogglWorkspace};
pub use billing::{BilledPart, BillingPolicy, billed_parts};
pub use entry::EntryDetails;
pub use error::TogglError;
pub use feature::{
    FEATURE_ID, TogglFeature, contributions, feature_id, manifest, register, settings_schema,
};
pub use flat::{FlatDay, FlatDayStatus, FlatJobInput, FlatRunReport, flat_job_range};
pub use http::{HttpTogglApi, LIVE_BASE_URL, RetryPolicy, has_token, token_id};
pub use mapping::{TogglMapping, allow_live, set_allow_live};
pub use queue::{DrainReport, EnqueueOutcome, QueueItem, QueueState};
pub use settings::TogglSettings;
pub use week::{TogglWeekDay, WeekDayStatus};
