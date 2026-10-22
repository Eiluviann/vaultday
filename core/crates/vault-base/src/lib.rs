//! Vault base: the one markdown parser, writer model and workspace model of
//! Vaultday, shared by every surface through the FFI and Node bindings.
//!
//! - [`Document`] scans a note line by line and renders it back byte for byte.
//! - [`Story`], [`ProjectHub`] and [`DayLog`] are typed views over a note.
//! - [`WorkspaceSettings`] is the `.vaultday/settings.json` schema.
//! - [`Workspace`] and [`WorkspaceSet`] resolve paths to workspaces.
//! - [`FileIndex`] caches modification times; [`Vault`] loads notes and
//!   answers the queries the CLI and the app need.
//! - [`FileAccess`] is the port to the file system; [`PlainFileAccess`] is the
//!   plain-path implementation for tests and desktop.

// The workspace lints forbid `unwrap`/`expect` in shipped code; tests may use them.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod date;
pub mod file_access;
pub mod frontmatter;
pub mod index;
mod line;
pub mod note;
pub mod scan;
pub mod settings;
pub mod vault;
pub mod workspace;

pub use date::{Date, Time};
pub use file_access::{
    DirEntry, EntryKind, FileAccess, FileAccessError, PlainFileAccess, WatchCallback, WatchHandle,
};
pub use frontmatter::Frontmatter;
pub use index::{FileIndex, FileStamp, IndexDelta, IndexError, IndexScope};
pub use line::LineEnding;
pub use note::{DayLog, LogKind, LogLine, NoteType, ProjectHub, Story, StoryStatus};
pub use scan::{
    Document, Heading, InlineField, Line, LineKind, ListItem, Marker, MarkerKind, ProvenanceUnit,
    Section, Task, TaskStatus,
};
pub use settings::{SETTINGS_SCHEMA_VERSION, SettingsError, WorkspaceKind, WorkspaceSettings};
pub use vault::{
    LogEntry, Note, StoryEntry, TaskRef, UnreadableNote, Vault, VaultError, WaitingOn,
};
pub use workspace::{
    WORKSPACE_DIR_NAME, Workspace, WorkspaceError, WorkspaceSet, find_vault_root,
    find_workspace_root, normalize_path,
};
