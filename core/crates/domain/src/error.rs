//! The crate's error type and `Result` alias.
//!
//! Every variant is an expected outcome of valid use (a second timer started
//! while one runs, a task missing from a plan). Contract violations panic
//! instead and are documented on the function that panics.

use jiff::Timestamp;
use jiff::civil::{Date, DateTime};
use thiserror::Error;

use crate::registry::FeatureId;
use crate::session::SessionId;
use crate::task::{TaskId, TimeBoxId};

/// Expected failures of the domain operations.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DomainError {
    /// A session is running or paused; only one session runs per app.
    #[error("session {running} is already running; stop it before starting another")]
    SessionAlreadyRunning {
        /// The session that is already active.
        running: SessionId,
    },
    /// No session is running or paused.
    #[error("no session is running")]
    NoRunningSession,
    /// The session is paused, so it cannot be paused again.
    #[error("session {id} is not running")]
    SessionNotRunning {
        /// The session the operation targeted.
        id: SessionId,
    },
    /// The session is running or stopped, so it cannot be resumed.
    #[error("session {id} is not paused")]
    SessionNotPaused {
        /// The session the operation targeted.
        id: SessionId,
    },
    /// The session has already stopped.
    #[error("session {id} has already stopped")]
    SessionAlreadyStopped {
        /// The session the operation targeted.
        id: SessionId,
    },
    /// A session with this id is already in the book.
    #[error("session {id} already exists")]
    DuplicateSessionId {
        /// The id used twice.
        id: SessionId,
    },
    /// A stored session's instants do not describe a session this crate could have produced.
    #[error("stored session {id} is malformed: {reason}")]
    MalformedStoredSession {
        /// The session being rebuilt.
        id: SessionId,
        /// What is wrong with it.
        reason: &'static str,
    },
    /// Stored sessions hold more than one running or paused session.
    #[error("stored sessions hold {count} active sessions; at most one is allowed")]
    MultipleActiveSessions {
        /// How many were active.
        count: usize,
    },
    /// A backdated start would count time an earlier session already counted.
    #[error("the start overlaps session {previous}, which ended at {ended_at}")]
    OverlapsPreviousSession {
        /// The session that ended after the requested start.
        previous: SessionId,
        /// When it ended.
        ended_at: Timestamp,
    },
    /// "Continue latest" found no earlier session to continue.
    #[error("no earlier session to continue")]
    NoSessionToContinue,
    /// A time range or a timeline step runs backwards.
    #[error("{end} is before {start}")]
    EndBeforeStart {
        /// The earlier boundary.
        start: Timestamp,
        /// The boundary that should come after it.
        end: Timestamp,
    },
    /// A timestamp or date left the representable range.
    #[error("the time or date is out of range")]
    TimeOutOfRange,
    /// The task is not in the plan.
    #[error("task {id} is not in the plan for {date}")]
    TaskNotInPlan {
        /// The task that was looked up.
        id: TaskId,
        /// The plan's date.
        date: Date,
    },
    /// The task is already in the plan.
    #[error("task {id} is already in the plan for {date}")]
    TaskAlreadyInPlan {
        /// The task that was added twice.
        id: TaskId,
        /// The plan's date.
        date: Date,
    },
    /// A list position past the end of the plan.
    #[error("position {position} is out of range for a plan of {len} tasks")]
    PositionOutOfRange {
        /// The requested position.
        position: usize,
        /// The number of tasks in the plan.
        len: usize,
    },
    /// The task is done or cancelled, so it does not roll over.
    #[error("task {id} is finished and does not roll over")]
    TaskFinished {
        /// The finished task.
        id: TaskId,
    },
    /// The time box starts on another day than the plan.
    #[error("time box {id} starts on {start}, not on {date}")]
    TimeBoxOutsideDay {
        /// The box.
        id: TimeBoxId,
        /// The day the box starts on.
        start: Date,
        /// The plan's date.
        date: Date,
    },
    /// Another task in the plan already holds a box with this id.
    #[error("time box {id} is already held by task {holder}")]
    DuplicateTimeBoxId {
        /// The box id used twice.
        id: TimeBoxId,
        /// The task that holds it.
        holder: TaskId,
    },
    /// The task has no time box to lock, unlock or clear.
    #[error("task {id} has no time box")]
    NoTimeBox {
        /// The task without a time box.
        id: TaskId,
    },
    /// A box would be locked over another locked box; two pins never share time.
    #[error("Another locked block already holds that time")]
    LockedTimeTaken {
        /// The task whose locked box holds the time.
        holder: TaskId,
    },
    /// A span of local time ends before it starts, or a working day has no length.
    #[error("the span from {start} to {end} runs backwards or is empty")]
    ReversedSpan {
        /// The start.
        start: DateTime,
        /// The end, which is not after the start.
        end: DateTime,
    },
    /// A quick-add line holds no text once its tokens are taken out.
    #[error("nothing to add: the line holds no text")]
    EmptyEntry,
    /// A workspace's `ticket_pattern` is not a valid regular expression.
    #[error("the ticket pattern `{pattern}` is not a valid regular expression: {reason}")]
    InvalidTicketPattern {
        /// The pattern as configured.
        pattern: String,
        /// What the regex engine refused.
        reason: String,
    },
    /// A `ticket_url` template names a `{field}` the story's frontmatter lacks.
    #[error("the ticket URL needs the field `{field}`, which the story does not have")]
    MissingTicketField {
        /// The field named between braces.
        field: String,
    },
    /// A feature with this id is already registered.
    #[error("feature {id} is already registered")]
    DuplicateFeature {
        /// The id registered twice.
        id: FeatureId,
    },
    /// A contribution enhances a feature its manifest does not list under `enhances`.
    #[error("feature {feature} contributes to {enhances} without declaring it in `enhances`")]
    UndeclaredEnhancement {
        /// The contributing feature.
        feature: FeatureId,
        /// The feature the contribution claimed to enhance.
        enhances: FeatureId,
    },
    /// Two features claim the same frontmatter key.
    #[error("feature {feature} claims the frontmatter key `{key}`, which {holder} already owns")]
    OwnedKeyClaimed {
        /// The feature being registered.
        feature: FeatureId,
        /// The key both want.
        key: String,
        /// The feature that owns it.
        holder: FeatureId,
    },
}

/// The crate's result type, with [`DomainError`] as the default error.
pub type Result<T, E = DomainError> = core::result::Result<T, E>;
