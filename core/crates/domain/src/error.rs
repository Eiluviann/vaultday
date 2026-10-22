//! The crate's error type and `Result` alias.
//!
//! Every variant is an expected outcome of valid use (a second timer started
//! while one runs, a task missing from a plan). Contract violations panic
//! instead and are documented on the function that panics.

use jiff::Timestamp;
use jiff::civil::Date;
use thiserror::Error;

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
}

/// The crate's result type, with [`DomainError`] as the default error.
pub type Result<T, E = DomainError> = core::result::Result<T, E>;
