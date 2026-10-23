//! Vaultday's app state: everything that is not in the vault, encrypted at
//! rest before any real data accumulates.
//!
//! - [`WorkspaceState`]: one Automerge document per workspace (day order,
//!   time boxes, sessions, rollover history, ritual answers, notification
//!   state, id counters), synced through [`WorkspaceSync`] as encrypted,
//!   batched change files under `<workspace>/.vaultday/sync/<device-id>/`.
//! - [`AppState`]: the app-level document (connections, sources, feature
//!   enablement, settings, security fields), stored by [`AppStateStore`] as
//!   one signed, encrypted snapshot per device.
//! - The key hierarchy in [`keys`]: a data key per workspace and one for the
//!   app document, both synchronisable, and a device-only Ed25519 signing key
//!   per device whose public half is published. Every key lives behind the
//!   [`SecretStore`] port; [`InMemorySecretStore`] serves tests and CI.
//! - Files go through the [`SyncFiles`] port; volatile state lives where the
//!   caller says, outside every workspace ([`VolatileStateDir`]).
//!
//! `SECURITY.md` beside this crate says what v1 protects and what it does not.

// The workspace lints forbid `unwrap`/`expect` in shipped code; tests may use them.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod app_doc;
pub mod device;
mod doc;
mod envelope;
pub mod error;
pub mod ids;
pub mod keys;
mod random;
pub mod secret_store;
pub mod settings_guard;
pub mod signed;
pub mod sync_files;
pub mod sync_folder;
pub mod volatile;
pub mod workspace_state;

pub use app_doc::{
    AccountPart, AppState, Connection, ConnectionId, ConnectionKind, EnablementScope,
    FeatureScopeChoice, FeatureScopeMode, FolderGrant, KeyboardScheme, ONBOARDING_STEP_COUNT,
    OnboardingProgress, SecurityMode, Source, SyncChoice, SyncHome, SyncService, WorkspaceSecurity,
};
pub use device::DeviceId;
pub use error::AppStateError;
pub use ids::{IdBlock, IdKind};
pub use keys::{
    DataKey, DeviceIdentity, TrustedDevices, app_key, create_app_key, create_workspace_key,
    delete_workspace, workspace_key,
};
pub use secret_store::{
    Availability, InMemorySecretStore, SecretBytes, SecretId, SecretItem, SecretKind, SecretStore,
    SecretStoreError,
};
pub use signed::{AppStateLoad, AppStateStore, RefusedSnapshot, SnapshotRefusal};
pub use sync_files::{PlainSyncFiles, SyncEntry, SyncFiles};
pub use sync_folder::{
    Compaction, CompactionOutcome, NotArrived, PendingFile, PullOutcome, SyncConfig,
    WORKSPACE_ID_FILE, WorkspaceSync, claim_workspace_id, holds_app_state, read_workspace_id,
    sync_folder,
};
pub use volatile::{GITIGNORE_SNIPPET, VolatileStateDir};
pub use workspace_state::{PlannedTimeBox, RolloverEntry, WorkspaceState};
