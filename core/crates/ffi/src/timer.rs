//! The timer surfaces over the bridge: "Start minutes ago", "Continue
//! latest", a stop that writes the session's day-log line, and the
//! long-timer check through the `Notifier` port, which the Mac app
//! implements with a user notification.

use std::sync::Arc;
use std::time::{Duration, Instant};

use domain::estimate::{Estimate, SpentTime};
use domain::ports::{Notification, Notifier, RecordingNotifier};
use domain::session::{Session, SessionBook, SessionId, SessionRecord, SessionSource};
use domain::task::{StoryNumber, TaskId};
use jiff::Timestamp;
use jiff::tz::TimeZone;
use vault_base::ops::{self, SessionLogRequest};
use vault_base::{Date, PlainFileAccess, Time};

use crate::app_state::{SessionView, WorkspaceStateHandle, lock, session_view};
use crate::home::settled_session_book;
use crate::{CoreError, WorkspaceHandle};

/// Where the app's notifications go: implemented by the platform. The core
/// decides when to notify, the platform how.
#[uniffi::export(with_foreign)]
pub trait NotifierPort: Send + Sync {
    /// A session has been running for the long-timer limit or longer; the
    /// platform asks "still working on this?".
    fn long_timer(&self, session_id: String, elapsed_seconds: u64);

    /// Reported time on a task reached the alert threshold of its estimate.
    fn threshold_crossed(
        &self,
        task: String,
        story: Option<String>,
        reported_seconds: u64,
        estimate_seconds: u64,
    );

    /// A mail fetch brought `added` new signals into `workspace`; `waiting`
    /// signals are new there now. Told once per fetch, never per signal, and
    /// shown quietly: no sound, no banner that stays.
    fn new_signals(&self, workspace: String, added: u32, waiting: u32);
}

/// A stopped session and what its stop wrote to the day log.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct StoppedSessionView {
    /// The session, now stopped.
    pub session: SessionView,
    /// The `session` line appended to the day log; `None` when the day log
    /// held the session already or the line could not be written.
    pub log_line: Option<String>,
    /// Why the line could not be written; the session stays stopped.
    pub log_error: Option<String>,
}

/// What writing a stopped session's day-log line did.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SessionLogView {
    /// The `session` line appended to the day log; `None` when the day log
    /// held the session already or the line could not be written.
    pub log_line: Option<String>,
    /// Why the line could not be written; the session stays stopped.
    pub log_error: Option<String>,
}

#[uniffi::export]
impl WorkspaceStateHandle {
    /// "Start minutes ago": starts the timer on a task or a story as if it had
    /// started `minutes` before now. The start may not reach into a recorded
    /// session.
    ///
    /// # Errors
    /// [`CoreError::Conflict`] while another session is active,
    /// [`CoreError::InvalidInput`] when the start overlaps the last session,
    /// or the document refused the edit.
    pub fn start_session_minutes_ago(
        &self,
        task: Option<String>,
        story: Option<String>,
        minutes: u32,
    ) -> Result<SessionView, CoreError> {
        let target = domain::session::SessionTarget {
            task: task.map(TaskId::new),
            story: story.map(Into::into),
        };
        self.start_with(|book, id, now| {
            book.start_minutes_ago(id, target, now, minutes)
                .map(Session::to_record)
        })
    }

    /// "Continue latest": a new session on the target of the timer session
    /// recorded last. Meetings are not continued.
    ///
    /// # Errors
    /// [`CoreError::Conflict`] while a session is active,
    /// [`CoreError::InvalidInput`] when there is no timer session to continue,
    /// or the document refused the edit.
    pub fn continue_latest_session(&self) -> Result<SessionView, CoreError> {
        self.start_with(|book, id, now| book.continue_latest(id, now).map(Session::to_record))
    }

    /// Appends the day-log line of the stopped session `session_id`, once:
    /// a day that holds the session's id already is left alone. The vault
    /// IO this does is why it is apart from the stop itself, so the app can
    /// stop on the main thread and write the line off it.
    ///
    /// # Errors
    /// [`CoreError::NotFound`] when no stopped session has that id. A failed
    /// write is reported in [`SessionLogView::log_error`], not as an error.
    pub fn log_stopped_session(
        &self,
        workspace: &WorkspaceHandle,
        session_id: String,
    ) -> Result<SessionLogView, CoreError> {
        let record = lock(&self.sync)
            .state()
            .sessions()
            .into_iter()
            .find(|record| record.id.as_str() == session_id && record.stopped_at.is_some())
            .ok_or_else(|| CoreError::NotFound(format!("no stopped session {session_id}")))?;
        Ok(log_session(workspace, &record))
    }

    /// Stops the running or paused session and appends its `session` line to
    /// the day log of its local start day, through the core's day-log
    /// writer. A session is logged once: a day log that already holds its id
    /// is left alone, whichever surface stopped it.
    ///
    /// # Errors
    /// [`CoreError::Conflict`] when no session is active, or the document
    /// refused the edit. A failed log write is reported in
    /// [`StoppedSessionView::log_error`], not as an error.
    pub fn stop_and_log_session(
        &self,
        workspace: &WorkspaceHandle,
    ) -> Result<StoppedSessionView, CoreError> {
        let record = {
            let mut sync = lock(&self.sync);
            let mut book = settled_session_book(&mut sync)?;
            let record = book.stop(Timestamp::now())?.to_record();
            sync.edit(Instant::now(), |state| state.put_session(&record))?;
            record
        };
        let SessionLogView {
            log_line,
            log_error,
        } = log_session(workspace, &record);
        Ok(StoppedSessionView {
            session: session_view(record),
            log_line,
            log_error,
        })
    }

    /// Asks, once per session, whether the active session has counted
    /// `limit_minutes` or longer; when it has, tells `notifier` and records
    /// that it did, on every device. Returns whether it notified this time.
    ///
    /// # Errors
    /// [`CoreError::InvalidInput`] when the stored sessions break their
    /// rules, or the document refused the edit.
    pub fn check_long_timer(
        &self,
        limit_minutes: u32,
        notifier: Arc<dyn NotifierPort>,
    ) -> Result<bool, CoreError> {
        let recorder = RecordingNotifier::new();
        {
            let mut sync = lock(&self.sync);
            let mut book = settled_session_book(&mut sync)?;
            let limit = Duration::from_secs(u64::from(limit_minutes) * 60);
            if !book.notify_long_timer(Timestamp::now(), limit, &recorder) {
                return Ok(false);
            }
            if let Some(record) = book.active().map(Session::to_record) {
                sync.edit(Instant::now(), |state| state.put_session(&record))?;
            }
        }
        // The platform is told once the document lock is released, so it may
        // read the sessions back from its callback.
        let port = ForeignNotifier(notifier.as_ref());
        for notification in recorder.sent() {
            forward(&port, &notification);
        }
        Ok(true)
    }
}

impl WorkspaceStateHandle {
    /// Mints a session id and runs one start on the book.
    fn start_with(
        &self,
        start: impl FnOnce(&mut SessionBook, SessionId, Timestamp) -> domain::Result<SessionRecord>,
    ) -> Result<SessionView, CoreError> {
        let mut sync = lock(&self.sync);
        let mut book = settled_session_book(&mut sync)?;
        let id = sync.edit(Instant::now(), |state| state.mint_session_id())?;
        let record = start(&mut book, id, Timestamp::now())?;
        sync.edit(Instant::now(), |state| state.put_session(&record))?;
        Ok(session_view(record))
    }
}

/// [`Notifier`] over a platform [`NotifierPort`].
struct ForeignNotifier<'port>(&'port dyn NotifierPort);

impl Notifier for ForeignNotifier<'_> {
    fn threshold_crossed(
        &self,
        task: &TaskId,
        story: Option<&StoryNumber>,
        reported: SpentTime,
        estimate: Estimate,
    ) {
        self.0.threshold_crossed(
            task.as_str().to_owned(),
            story.map(|story| story.as_str().to_owned()),
            reported.as_secs(),
            estimate.as_duration().as_secs(),
        );
    }

    fn long_timer(&self, session: &SessionId, elapsed: SpentTime) {
        self.0
            .long_timer(session.as_str().to_owned(), elapsed.as_secs());
    }
}

fn forward(notifier: &dyn Notifier, notification: &Notification) {
    match notification {
        Notification::ThresholdCrossed {
            task,
            story,
            reported,
            estimate,
        } => notifier.threshold_crossed(task, story.as_ref(), *reported, *estimate),
        Notification::LongTimer { session, elapsed } => notifier.long_timer(session, *elapsed),
    }
}

/// The day-log request for a stopped session: its local start day and time,
/// and the minutes it counted.
fn log_session(workspace: &WorkspaceHandle, record: &SessionRecord) -> SessionLogView {
    match session_log_request(record).and_then(|request| {
        Ok(ops::append_session_log(
            &workspace.workspace,
            &PlainFileAccess,
            &request,
        )?)
    }) {
        Ok(change) => SessionLogView {
            log_line: change.map(|change| change.new_line),
            log_error: None,
        },
        Err(error) => SessionLogView {
            log_line: None,
            log_error: Some(error.to_string()),
        },
    }
}

fn session_log_request(record: &SessionRecord) -> Result<SessionLogRequest, CoreError> {
    let session = Session::from_record(record.clone())?;
    let stopped_at = record.stopped_at.unwrap_or_else(Timestamp::now);
    let (date, time) = local_date_and_time(record.started_at)?;
    let fallback_text = match &record.source {
        SessionSource::Meeting { title } => title.clone(),
        SessionSource::Timer => record
            .target
            .story
            .as_ref()
            .map(|story| story.as_str().to_owned())
            .or_else(|| {
                record
                    .target
                    .task
                    .as_ref()
                    .map(|task| task.as_str().to_owned())
            })
            .unwrap_or_else(|| "Timer".to_owned()),
    };
    Ok(SessionLogRequest {
        date,
        time,
        session: record.id.as_str().to_owned(),
        task: record
            .target
            .task
            .as_ref()
            .map(|task| task.as_str().to_owned()),
        story: record
            .target
            .story
            .as_ref()
            .map(|story| story.as_str().to_owned()),
        fallback_text,
        minutes: session.elapsed(stopped_at).as_secs() / 60,
    })
}

fn local_date_and_time(at: Timestamp) -> Result<(Date, Time), CoreError> {
    let zoned = at.to_zoned(TimeZone::system());
    let date = Date::new(
        i32::from(zoned.year()),
        zoned.month().unsigned_abs(),
        zoned.day().unsigned_abs(),
    );
    let time = Time::new(zoned.hour().unsigned_abs(), zoned.minute().unsigned_abs());
    date.zip(time)
        .ok_or_else(|| CoreError::InvalidDate(format!("{at} is outside the day log's range")))
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::home::tests::{open, sandbox, state};

    #[derive(Default)]
    struct Recorded {
        long_timers: Mutex<Vec<(String, u64)>>,
    }

    impl NotifierPort for Recorded {
        fn long_timer(&self, session_id: String, elapsed_seconds: u64) {
            self.long_timers
                .lock()
                .unwrap()
                .push((session_id, elapsed_seconds));
        }

        fn threshold_crossed(&self, _: String, _: Option<String>, _: u64, _: u64) {}

        fn new_signals(&self, _: String, _: u32, _: u32) {}
    }

    #[test]
    fn a_session_paused_and_resumed_stops_once_with_one_log_line() {
        let sandbox = sandbox();
        let workspace = open(sandbox.path());
        let state = state(sandbox.path());
        let started = state
            .start_session(Some("task-5".to_owned()), Some("WEB-142".to_owned()))
            .unwrap();
        state.pause_session().unwrap();
        state.resume_session().unwrap();
        let stopped = state.stop_and_log_session(&workspace).unwrap();
        assert_eq!(stopped.session.id, started.id);
        assert_eq!(
            state.sessions().len(),
            1,
            "pause and resume stay one session"
        );
        let line = stopped.log_line.unwrap();
        assert!(
            line.contains(" session · Write the end-to-end checkout test · 0m "),
            "{line}"
        );
        assert!(line.contains(&format!("[id:: {}]", started.id)), "{line}");
        assert_eq!(stopped.log_error, None);
        let again = state.stop_and_log_session(&workspace).unwrap_err();
        assert!(
            matches!(again, CoreError::Conflict(_)),
            "nothing left to stop"
        );
    }

    #[test]
    fn a_stop_then_a_separate_log_write_logs_once() {
        let sandbox = sandbox();
        let workspace = open(sandbox.path());
        let state = state(sandbox.path());
        let started = state
            .start_session(Some("task-5".to_owned()), None)
            .unwrap();
        assert!(matches!(
            state
                .log_stopped_session(&workspace, started.id.clone())
                .unwrap_err(),
            CoreError::NotFound(_)
        ));
        state.stop_session().unwrap();
        let logged = state
            .log_stopped_session(&workspace, started.id.clone())
            .unwrap();
        let line = logged.log_line.unwrap();
        assert!(line.contains(&format!("[id:: {}]", started.id)), "{line}");
        let again = state.log_stopped_session(&workspace, started.id).unwrap();
        assert_eq!(again.log_line, None, "the day holds the session already");
        assert_eq!(again.log_error, None);
    }

    #[test]
    fn continue_latest_restarts_the_last_target_and_minutes_ago_backdates() {
        let sandbox = sandbox();
        let workspace = open(sandbox.path());
        let state = state(sandbox.path());
        assert!(matches!(
            state.continue_latest_session().unwrap_err(),
            CoreError::InvalidInput(_)
        ));
        let started = state
            .start_session_minutes_ago(Some("task-2".to_owned()), None, 10)
            .unwrap();
        assert!(
            started.elapsed_seconds >= 600,
            "{}",
            started.elapsed_seconds
        );
        assert!(matches!(
            state.continue_latest_session().unwrap_err(),
            CoreError::Conflict(_)
        ));
        let stopped = state.stop_and_log_session(&workspace).unwrap();
        assert!(stopped.log_line.unwrap().contains(" · 10m "));
        let continued = state.continue_latest_session().unwrap();
        assert_ne!(continued.id, started.id);
        assert_eq!(continued.task.as_deref(), Some("task-2"));
        let overlapping = state
            .stop_session()
            .and_then(|_| state.start_session_minutes_ago(Some("task-2".to_owned()), None, 60));
        assert!(matches!(
            overlapping.unwrap_err(),
            CoreError::InvalidInput(_)
        ));
    }

    #[test]
    fn the_long_timer_notifies_once_per_session_and_never_while_under_the_limit() {
        let sandbox = sandbox();
        let state = state(sandbox.path());
        let notifier = Arc::new(Recorded::default());
        assert!(
            !state.check_long_timer(1, notifier.clone()).unwrap(),
            "no session"
        );
        let started = state
            .start_session_minutes_ago(Some("task-2".to_owned()), None, 5)
            .unwrap();
        assert!(!state.check_long_timer(60, notifier.clone()).unwrap());
        assert!(state.check_long_timer(5, notifier.clone()).unwrap());
        assert!(
            !state.check_long_timer(5, notifier.clone()).unwrap(),
            "once per session"
        );
        let sent = notifier.long_timers.lock().unwrap().clone();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, started.id);
        assert!(sent[0].1 >= 300);
    }
}
