//! Vault base: the one markdown parser, writer model and workspace model of
//! Vaultday, shared by every surface through the FFI and Node bindings.
//!
//! - [`Document`] scans a note line by line and renders it back byte for byte.
//! - [`Story`], [`ProjectHub`] and [`DayLog`] are typed views over a note.
//! - [`WorkspaceSettings`] is the `.vaultday/settings.json` schema.
//! - [`Workspace`] and [`WorkspaceSet`] resolve paths to workspaces.
//! - [`FileIndex`] caches modification times; [`Vault`] loads notes and
//!   answers the queries the CLI and the app need; [`TodayView`] is the
//!   one the Today surfaces copy out.
//! - [`ops`] holds the operations the CLI, the MCP server and the apps share,
//!   as plain records the bindings copy.
//! - [`FileAccess`] is the port to the file system; [`PlainFileAccess`] is the
//!   plain-path implementation for tests and desktop.
//! - [`VaultWriter`] is the one write path: line-scoped, atomic, re-read
//!   before every write; [`WriteQueue`] debounces bursts; [`log`] formats day-log
//!   lines; [`conflict`] merges sync conflict copies back into their note.
//! - [`VaultWatcher`] turns file events into debounced batches that update the
//!   [`FileIndex`], and [`IndexStore`] persists that index per device.

// The workspace lints forbid `unwrap`/`expect` in shipped code; tests may use them.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod buffer;
pub mod conflict;
pub mod date;
pub mod file_access;
pub mod frontmatter;
pub mod index;
pub mod index_store;
mod line;
pub mod log;
pub mod note;
pub mod ops;
pub mod scan;
pub mod settings;
pub mod today;
pub mod vault;
pub mod watcher;
pub mod workspace;
pub mod writer;

pub use conflict::{
    ConflictCopy, MergeConflict, MergeReport, conflict_copies_in_index, conflict_copy_original,
    find_conflict_copies,
};
pub use date::{Date, Time};
pub use file_access::{
    DirEntry, EntryKind, FileAccess, FileAccessError, PlainFileAccess, WatchCallback, WatchHandle,
};
pub use frontmatter::Frontmatter;
pub use index::{FileIndex, FileStamp, IndexDelta, IndexError, IndexScope};
pub use index_store::{INDEX_STORE_SCHEMA_VERSION, IndexStore, IndexStoreError};
pub use line::LineEnding;
pub use log::{LogAppend, LogFormatError, day_log_path, format_log_line, new_day_log_header};
pub use note::{DayLog, LogKind, LogLine, NoteType, ProjectHub, Story, StoryStatus};
pub use ops::{OpError, resolve_workspace};
pub use scan::{
    Document, Heading, InlineField, Line, LineKind, ListItem, Marker, MarkerKind, ProvenanceUnit,
    Section, Task, TaskStatus,
};
pub use settings::{SETTINGS_SCHEMA_VERSION, SettingsError, WorkspaceKind, WorkspaceSettings};
pub use today::{TodayStory, TodayView};
pub use vault::{
    LogEntry, Note, StoryEntry, TaskRef, UnreadableNote, Vault, VaultError, WaitingOn,
};
pub use watcher::{DEFAULT_WATCH_DEBOUNCE, VaultWatcher, WatchBatch, is_ignored};
pub use workspace::{
    WORKSPACE_DIR_NAME, Workspace, WorkspaceError, WorkspaceSet, find_vault_root,
    find_workspace_root, normalize_path,
};
pub use writer::{
    DEFAULT_DEBOUNCE, DEFAULT_STORY_TEMPLATE, NewStory, NewTask, QueueOutcome, TaskEdit,
    TaskSection, TaskTarget, VaultWriter, WriteError, WriteQueue, WriteReceipt, task_line,
};
