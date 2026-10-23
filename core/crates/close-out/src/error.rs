//! Why a close-out operation failed.

use thiserror::Error;

/// A close-out failure.
#[derive(Debug, Error)]
pub enum CloseOutError {
    /// The input breaks a rule: a bad date, an allocation that invents work.
    #[error("{0}")]
    InvalidInput(String),
    /// The period cannot close yet; the list says what is missing.
    #[error("the close-out still misses: {}", .0.join(", "))]
    NotReady(Vec<String>),
    /// The workspace has no `log_deadline`, so it has no close-out.
    #[error("this workspace has no log_deadline, so it has no close-out")]
    NoDeadline,
    /// A date rule failed.
    #[error(transparent)]
    Domain(#[from] domain::DomainError),
    /// The Toggl feature refused.
    #[error(transparent)]
    Toggl(#[from] vaultday_feature_toggl::TogglError),
    /// The workspace document refused an edit.
    #[error(transparent)]
    AppState(#[from] app_state::AppStateError),
    /// The day log could not be written.
    #[error(transparent)]
    Write(#[from] vault_base::writer::WriteError),
}
