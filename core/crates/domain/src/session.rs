//! Timer sessions: a run of the timer against a task or story, and the book
//! that allows one running session per app.
//!
//! Sessions are stored raw: exact start, pause and stop instants, never
//! rounded. A pause and its resume stay inside one session; the paused span
//! is cut out of the elapsed time.

use std::time::Duration;

use jiff::Timestamp;

use crate::error::{DomainError, Result};
use crate::estimate::{LoggedTime, SpentTime};
use crate::ports::Notifier;
use crate::task::{StoryNumber, TaskId};

id_type! {
    /// Identifies a session, such as `ses-41`.
    SessionId
}

/// What a session was started for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionTarget {
    /// The task the time is spent on, when the timer ran on a task.
    pub task: Option<TaskId>,
    /// The story the time belongs to, when known.
    pub story: Option<StoryNumber>,
}

impl SessionTarget {
    /// A session on a task without a known story.
    pub fn task(task: impl Into<TaskId>) -> Self {
        Self {
            task: Some(task.into()),
            story: None,
        }
    }

    /// A session on a story without a task.
    pub fn story(story: impl Into<StoryNumber>) -> Self {
        Self {
            task: None,
            story: Some(story.into()),
        }
    }

    /// A session on a task inside a story.
    pub fn task_in_story(task: impl Into<TaskId>, story: impl Into<StoryNumber>) -> Self {
        Self {
            task: Some(task.into()),
            story: Some(story.into()),
        }
    }
}

/// How the session came to exist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionSource {
    /// The user ran the timer.
    Timer,
    /// Created from a calendar event; the event title is kept.
    Meeting {
        /// The calendar event's title.
        title: String,
    },
}

/// A calendar event, as the calendar port hands it over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalendarEvent {
    /// The event title.
    pub title: String,
    /// When the event starts.
    pub start: Timestamp,
    /// When the event ends.
    pub end: Timestamp,
}

/// A span cut out of a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pause {
    started_at: Timestamp,
    ended_at: Option<Timestamp>,
}

impl Pause {
    /// A pause that began at `started_at` and ended at `ended_at`, or is still open.
    /// Order is checked when the session is rebuilt with [`Session::from_record`].
    pub const fn new(started_at: Timestamp, ended_at: Option<Timestamp>) -> Self {
        Self {
            started_at,
            ended_at,
        }
    }

    /// When the pause began.
    pub const fn started_at(&self) -> Timestamp {
        self.started_at
    }

    /// When the session resumed; `None` while still paused.
    pub const fn ended_at(&self) -> Option<Timestamp> {
        self.ended_at
    }

    fn length_until(&self, end: Timestamp) -> Duration {
        let pause_end = self.ended_at.map_or(end, |ended| ended.min(end));
        non_negative(pause_end.duration_since(self.started_at))
    }
}

/// Where a session is in its life.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// Counting time.
    Running,
    /// Started, not stopped, not counting.
    Paused,
    /// Finished; its elapsed time is final.
    Stopped,
}

/// A session's stored fields: what persistence writes and reads back. Only
/// [`Session::from_record`] turns one into a session, after checking its order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRecord {
    /// The session's id.
    pub id: SessionId,
    /// What the session was started for.
    pub target: SessionTarget,
    /// How the session came to exist.
    pub source: SessionSource,
    /// When the session started.
    pub started_at: Timestamp,
    /// When the session stopped; `None` while running or paused.
    pub stopped_at: Option<Timestamp>,
    /// The pauses, oldest first; only the last may be open, and only while not stopped.
    pub pauses: Vec<Pause>,
    /// Whether the long-timer alert already fired for this session.
    pub has_long_timer_fired: bool,
}

/// One run of the timer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    id: SessionId,
    target: SessionTarget,
    source: SessionSource,
    started_at: Timestamp,
    stopped_at: Option<Timestamp>,
    pauses: Vec<Pause>,
    has_long_timer_fired: bool,
}

impl Session {
    /// Starts a session at `now`.
    pub fn start(id: impl Into<SessionId>, target: SessionTarget, now: Timestamp) -> Self {
        Self {
            id: id.into(),
            target,
            source: SessionSource::Timer,
            started_at: now,
            stopped_at: None,
            pauses: Vec::new(),
            has_long_timer_fired: false,
        }
    }

    /// Starts a session that began `minutes` before `now` ("Start minutes ago").
    ///
    /// # Errors
    /// [`DomainError::TimeOutOfRange`] when the start would precede the representable range.
    pub fn start_minutes_ago(
        id: impl Into<SessionId>,
        target: SessionTarget,
        now: Timestamp,
        minutes: u32,
    ) -> Result<Self> {
        let started_at = now
            .checked_sub(Duration::from_secs(u64::from(minutes) * 60))
            .map_err(|_| DomainError::TimeOutOfRange)?;
        Ok(Self::start(id, target, started_at))
    }

    /// A stopped meeting session covering the event, untouched by any rounding.
    ///
    /// # Errors
    /// [`DomainError::EndBeforeStart`] when the event ends before it starts.
    pub fn from_calendar_event(
        id: impl Into<SessionId>,
        target: SessionTarget,
        event: &CalendarEvent,
    ) -> Result<Self> {
        ensure_ordered(event.start, event.end)?;
        Ok(Self {
            id: id.into(),
            target,
            source: SessionSource::Meeting {
                title: event.title.clone(),
            },
            started_at: event.start,
            stopped_at: Some(event.end),
            pauses: Vec::new(),
            has_long_timer_fired: false,
        })
    }

    /// Rebuilds a session from its stored fields.
    ///
    /// # Errors
    /// [`DomainError::EndBeforeStart`] when a pause or the stop precedes the step
    /// before it, [`DomainError::MalformedStoredSession`] when an open pause is not
    /// the last one or a stopped session still has an open pause.
    pub fn from_record(record: SessionRecord) -> Result<Self> {
        let malformed = |reason: &'static str| DomainError::MalformedStoredSession {
            id: record.id.clone(),
            reason,
        };
        let mut last_step = record.started_at;
        for (index, pause) in record.pauses.iter().enumerate() {
            ensure_ordered(last_step, pause.started_at)?;
            match pause.ended_at {
                Some(ended_at) => {
                    ensure_ordered(pause.started_at, ended_at)?;
                    last_step = ended_at;
                }
                None => {
                    if index + 1 != record.pauses.len() {
                        return Err(malformed("an open pause is not the last pause"));
                    }
                    if record.stopped_at.is_some() {
                        return Err(malformed("a stopped session has an open pause"));
                    }
                    last_step = pause.started_at;
                }
            }
        }
        if let Some(stopped_at) = record.stopped_at {
            ensure_ordered(last_step, stopped_at)?;
        }
        Ok(Self {
            id: record.id,
            target: record.target,
            source: record.source,
            started_at: record.started_at,
            stopped_at: record.stopped_at,
            pauses: record.pauses,
            has_long_timer_fired: record.has_long_timer_fired,
        })
    }

    /// The session's stored fields, for persistence.
    pub fn to_record(&self) -> SessionRecord {
        SessionRecord {
            id: self.id.clone(),
            target: self.target.clone(),
            source: self.source.clone(),
            started_at: self.started_at,
            stopped_at: self.stopped_at,
            pauses: self.pauses.clone(),
            has_long_timer_fired: self.has_long_timer_fired,
        }
    }

    /// The session's id.
    pub fn id(&self) -> &SessionId {
        &self.id
    }

    /// What the session was started for.
    pub fn target(&self) -> &SessionTarget {
        &self.target
    }

    /// How the session came to exist.
    pub fn source(&self) -> &SessionSource {
        &self.source
    }

    /// When the session started.
    pub const fn started_at(&self) -> Timestamp {
        self.started_at
    }

    /// When the session stopped; `None` while running or paused.
    pub const fn stopped_at(&self) -> Option<Timestamp> {
        self.stopped_at
    }

    /// The pauses, oldest first.
    pub fn pauses(&self) -> &[Pause] {
        &self.pauses
    }

    /// Where the session is in its life.
    pub fn state(&self) -> SessionState {
        if self.stopped_at.is_some() {
            SessionState::Stopped
        } else if self.is_paused() {
            SessionState::Paused
        } else {
            SessionState::Running
        }
    }

    /// Running or paused: the session is the app's one active session.
    pub fn is_active(&self) -> bool {
        self.stopped_at.is_none()
    }

    /// Created from a calendar event.
    pub fn is_meeting(&self) -> bool {
        matches!(self.source, SessionSource::Meeting { .. })
    }

    /// Whether the long-timer alert already fired for this session.
    pub const fn has_long_timer_fired(&self) -> bool {
        self.has_long_timer_fired
    }

    /// Time counted up to `at`: a stopped session queried after its end gives
    /// its full length, one queried before its end only the part before `at`.
    pub fn elapsed(&self, at: Timestamp) -> SpentTime {
        let end = self.stopped_at.map_or(at, |stopped_at| stopped_at.min(at));
        let total = non_negative(end.duration_since(self.started_at));
        let paused = self
            .pauses
            .iter()
            .map(|pause| pause.length_until(end))
            .fold(Duration::ZERO, Duration::saturating_add);
        SpentTime::new(total.saturating_sub(paused))
    }

    /// Pauses a running session at `now`.
    ///
    /// # Errors
    /// [`DomainError::SessionNotRunning`] when paused, [`DomainError::SessionAlreadyStopped`]
    /// when stopped, [`DomainError::EndBeforeStart`] when `now` precedes the last step.
    pub fn pause(&mut self, now: Timestamp) -> Result<()> {
        match self.state() {
            SessionState::Running => {}
            SessionState::Paused => {
                return Err(DomainError::SessionNotRunning {
                    id: self.id.clone(),
                });
            }
            SessionState::Stopped => return Err(self.already_stopped()),
        }
        ensure_ordered(self.last_step(), now)?;
        self.pauses.push(Pause {
            started_at: now,
            ended_at: None,
        });
        Ok(())
    }

    /// Resumes a paused session at `now`; the pause is cut out of the elapsed time.
    ///
    /// # Errors
    /// [`DomainError::SessionNotPaused`] when running, [`DomainError::SessionAlreadyStopped`]
    /// when stopped, [`DomainError::EndBeforeStart`] when `now` precedes the pause.
    pub fn resume(&mut self, now: Timestamp) -> Result<()> {
        if self.stopped_at.is_some() {
            return Err(self.already_stopped());
        }
        if !self.is_paused() {
            return Err(DomainError::SessionNotPaused {
                id: self.id.clone(),
            });
        }
        ensure_ordered(self.last_step(), now)?;
        if let Some(open_pause) = self.pauses.last_mut() {
            open_pause.ended_at = Some(now);
        }
        Ok(())
    }

    /// Stops the session at `now`; a paused session closes its pause first.
    ///
    /// # Errors
    /// [`DomainError::SessionAlreadyStopped`] when stopped,
    /// [`DomainError::EndBeforeStart`] when `now` precedes the last step.
    pub fn stop(&mut self, now: Timestamp) -> Result<()> {
        if self.stopped_at.is_some() {
            return Err(self.already_stopped());
        }
        ensure_ordered(self.last_step(), now)?;
        if let Some(open_pause) = self
            .pauses
            .last_mut()
            .filter(|pause| pause.ended_at.is_none())
        {
            open_pause.ended_at = Some(now);
        }
        self.stopped_at = Some(now);
        Ok(())
    }

    /// Says, once per session, that the session is running and has counted
    /// `limit` or longer as of `now`. A paused session never fires.
    pub fn check_long_timer(&mut self, now: Timestamp, limit: Duration) -> bool {
        if self.has_long_timer_fired || self.state() != SessionState::Running {
            return false;
        }
        if self.elapsed(now).as_duration() < limit {
            return false;
        }
        self.has_long_timer_fired = true;
        true
    }

    fn is_paused(&self) -> bool {
        self.pauses
            .last()
            .is_some_and(|pause| pause.ended_at.is_none())
    }

    fn last_step(&self) -> Timestamp {
        self.pauses.last().map_or(self.started_at, |pause| {
            pause.ended_at.unwrap_or(pause.started_at)
        })
    }

    fn already_stopped(&self) -> DomainError {
        DomainError::SessionAlreadyStopped {
            id: self.id.clone(),
        }
    }
}

/// One day's timer time, the two kinds kept apart.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DayTime {
    /// Real time the timer counted inside the day, running sessions included.
    pub spent: SpentTime,
    /// The part of it already allocated to a story: stopped sessions with a story.
    pub logged: LoggedTime,
}

/// Every session of the app, with the rule that only one runs at a time.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionBook {
    sessions: Vec<Session>,
}

impl SessionBook {
    /// An empty book.
    pub const fn new() -> Self {
        Self {
            sessions: Vec::new(),
        }
    }

    /// Reconstitutes a book from stored sessions, in the order they were recorded.
    ///
    /// # Errors
    /// [`DomainError::MultipleActiveSessions`] when more than one is running or paused,
    /// [`DomainError::DuplicateSessionId`] when two share an id.
    pub fn from_sessions(sessions: Vec<Session>) -> Result<Self> {
        let count = sessions
            .iter()
            .filter(|session| session.is_active())
            .count();
        if count > 1 {
            return Err(DomainError::MultipleActiveSessions { count });
        }
        let mut book = Self::new();
        for session in sessions {
            book.push(session)?;
        }
        Ok(book)
    }

    /// Every session, oldest first.
    pub fn sessions(&self) -> &[Session] {
        &self.sessions
    }

    /// The running or paused session, if any.
    pub fn active(&self) -> Option<&Session> {
        self.sessions.iter().find(|session| session.is_active())
    }

    /// The session recorded last, whatever its state or source.
    pub fn latest(&self) -> Option<&Session> {
        self.sessions.last()
    }

    /// The timer session recorded last; meetings do not count.
    pub fn latest_timer(&self) -> Option<&Session> {
        self.sessions
            .iter()
            .rev()
            .find(|session| !session.is_meeting())
    }

    /// Starts a session at `now`.
    ///
    /// # Errors
    /// [`DomainError::SessionAlreadyRunning`] while another session is running or paused.
    pub fn start(
        &mut self,
        id: impl Into<SessionId>,
        target: SessionTarget,
        now: Timestamp,
    ) -> Result<&Session> {
        self.ensure_none_active()?;
        self.push(Session::start(id, target, now))
    }

    /// Starts a session that began `minutes` before `now`. The backdated start
    /// may not reach into a recorded session, so no minute is counted twice.
    ///
    /// # Errors
    /// [`DomainError::SessionAlreadyRunning`] while another session is active,
    /// [`DomainError::OverlapsPreviousSession`] when the start precedes the end of
    /// the last recorded session, [`DomainError::TimeOutOfRange`] when the start
    /// is unrepresentable, [`DomainError::DuplicateSessionId`] when the id is taken.
    pub fn start_minutes_ago(
        &mut self,
        id: impl Into<SessionId>,
        target: SessionTarget,
        now: Timestamp,
        minutes: u32,
    ) -> Result<&Session> {
        self.ensure_none_active()?;
        let session = Session::start_minutes_ago(id, target, now, minutes)?;
        self.ensure_no_overlap(session.started_at())?;
        self.push(session)
    }

    /// "Continue latest": starts a new session at `now` on the target of the
    /// timer session recorded last. Meetings are not continued.
    ///
    /// # Errors
    /// [`DomainError::SessionAlreadyRunning`] while a session is active,
    /// [`DomainError::NoSessionToContinue`] when no timer session exists,
    /// [`DomainError::DuplicateSessionId`] when the id is taken.
    pub fn continue_latest(
        &mut self,
        id: impl Into<SessionId>,
        now: Timestamp,
    ) -> Result<&Session> {
        self.ensure_none_active()?;
        let target = self
            .latest_timer()
            .map(|session| session.target().clone())
            .ok_or(DomainError::NoSessionToContinue)?;
        self.push(Session::start(id, target, now))
    }

    /// Records a stopped meeting session from a calendar event. It may sit
    /// beside an active timer session, since it never runs.
    ///
    /// # Errors
    /// [`DomainError::EndBeforeStart`] when the event ends before it starts,
    /// [`DomainError::DuplicateSessionId`] when the id is taken.
    pub fn record_meeting(
        &mut self,
        id: impl Into<SessionId>,
        target: SessionTarget,
        event: &CalendarEvent,
    ) -> Result<&Session> {
        let session = Session::from_calendar_event(id, target, event)?;
        self.push(session)
    }

    /// Pauses the active session.
    ///
    /// # Errors
    /// [`DomainError::NoRunningSession`] when nothing is active, plus [`Session::pause`]'s errors.
    pub fn pause(&mut self, now: Timestamp) -> Result<&Session> {
        let index = self.active_index()?;
        let session = &mut self.sessions[index];
        session.pause(now)?;
        Ok(session)
    }

    /// Resumes the active session.
    ///
    /// # Errors
    /// [`DomainError::NoRunningSession`] when nothing is active, plus [`Session::resume`]'s errors.
    pub fn resume(&mut self, now: Timestamp) -> Result<&Session> {
        let index = self.active_index()?;
        let session = &mut self.sessions[index];
        session.resume(now)?;
        Ok(session)
    }

    /// Stops the active session.
    ///
    /// # Errors
    /// [`DomainError::NoRunningSession`] when nothing is active, plus [`Session::stop`]'s errors.
    pub fn stop(&mut self, now: Timestamp) -> Result<&Session> {
        let index = self.active_index()?;
        let session = &mut self.sessions[index];
        session.stop(now)?;
        Ok(session)
    }

    /// Time spent on a task across its sessions, each counted up to `at`.
    pub fn spent_on_task(&self, task: &TaskId, at: Timestamp) -> SpentTime {
        self.sessions
            .iter()
            .filter(|session| session.target().task.as_ref() == Some(task))
            .map(|session| session.elapsed(at))
            .sum()
    }

    /// Time spent on a story across its sessions, each counted up to `at`.
    pub fn spent_on_story(&self, story: &StoryNumber, at: Timestamp) -> SpentTime {
        self.sessions
            .iter()
            .filter(|session| session.target().story.as_ref() == Some(story))
            .map(|session| session.elapsed(at))
            .sum()
    }

    /// The timer time inside `from..to` (one day), counted up to `now`. A
    /// session that crosses the day's edge counts only its part inside it.
    pub fn day_time(&self, from: Timestamp, to: Timestamp, now: Timestamp) -> DayTime {
        let until = to.min(now);
        let inside = |session: &Session| -> Duration {
            if until <= from {
                return Duration::ZERO;
            }
            let before = session.elapsed(from).as_duration();
            session.elapsed(until).as_duration().saturating_sub(before)
        };
        self.sessions
            .iter()
            .fold(DayTime::default(), |mut day, session| {
                let counted = inside(session);
                day.spent += SpentTime::new(counted);
                let is_logged =
                    session.state() == SessionState::Stopped && session.target().story.is_some();
                if is_logged {
                    day.logged += LoggedTime::new(counted);
                }
                day
            })
    }

    /// Tells the notifier, once per session, when the active session has run
    /// for `limit` or longer. Returns whether it fired this time.
    pub fn notify_long_timer(
        &mut self,
        now: Timestamp,
        limit: Duration,
        notifier: &dyn Notifier,
    ) -> bool {
        let Some(session) = self.sessions.iter_mut().find(|session| session.is_active()) else {
            return false;
        };
        if !session.check_long_timer(now, limit) {
            return false;
        }
        notifier.long_timer(session.id(), session.elapsed(now));
        true
    }

    fn ensure_none_active(&self) -> Result<()> {
        match self.active() {
            Some(running) => Err(DomainError::SessionAlreadyRunning {
                running: running.id().clone(),
            }),
            None => Ok(()),
        }
    }

    fn active_index(&self) -> Result<usize> {
        self.sessions
            .iter()
            .position(|session| session.is_active())
            .ok_or(DomainError::NoRunningSession)
    }

    fn ensure_no_overlap(&self, start: Timestamp) -> Result<()> {
        let overlapping = self
            .sessions
            .iter()
            .filter_map(|session| session.stopped_at().map(|ended_at| (session, ended_at)))
            .filter(|(_, ended_at)| *ended_at > start)
            .max_by_key(|(_, ended_at)| *ended_at);
        match overlapping {
            Some((previous, ended_at)) => Err(DomainError::OverlapsPreviousSession {
                previous: previous.id().clone(),
                ended_at,
            }),
            None => Ok(()),
        }
    }

    fn push(&mut self, session: Session) -> Result<&Session> {
        if self.sessions.iter().any(|known| known.id() == session.id()) {
            return Err(DomainError::DuplicateSessionId {
                id: session.id().clone(),
            });
        }
        self.sessions.push(session);
        let last = self.sessions.len() - 1;
        Ok(&self.sessions[last])
    }
}

fn ensure_ordered(start: Timestamp, end: Timestamp) -> Result<()> {
    if end < start {
        return Err(DomainError::EndBeforeStart { start, end });
    }
    Ok(())
}

fn non_negative(duration: jiff::SignedDuration) -> Duration {
    if duration.is_negative() {
        Duration::ZERO
    } else {
        duration.unsigned_abs()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::{Notification, RecordingNotifier};

    const EPOCH: i64 = 1_800_000_000;

    type Step = fn(&mut Session, Timestamp) -> Result<()>;
    type TransitionCase = (&'static str, Session, Step, DomainError);

    fn at(minutes: i64) -> Timestamp {
        Timestamp::from_second(EPOCH + minutes * 60).unwrap()
    }

    fn minutes(m: u64) -> SpentTime {
        SpentTime::from_minutes(m)
    }

    fn target() -> SessionTarget {
        SessionTarget::task_in_story("task-7", "WEB-142")
    }

    #[test]
    fn a_day_counts_its_part_of_each_session_and_logs_only_stopped_story_time() {
        let mut overnight = Session::start("ses-1", target(), at(-30));
        overnight.stop(at(30)).unwrap();
        let mut no_story = Session::start("ses-2", SessionTarget::task("task-8"), at(60));
        no_story.stop(at(70)).unwrap();
        let mut running = Session::start("ses-3", target(), at(100));
        running.pause(at(110)).unwrap();
        running.resume(at(115)).unwrap();
        let book = SessionBook::from_sessions(vec![overnight, no_story, running]).unwrap();

        let day = book.day_time(at(0), at(24 * 60), at(125));
        assert_eq!(day.spent, minutes(30 + 10 + 20));
        assert_eq!(day.logged, LoggedTime::from_minutes(30));

        let before_the_day = book.day_time(at(24 * 60), at(48 * 60), at(125));
        assert_eq!(before_the_day, DayTime::default());
    }

    #[test]
    fn pause_and_resume_cut_the_pause_out_of_one_session() {
        let mut session = Session::start("ses-1", target(), at(0));
        assert_eq!(session.state(), SessionState::Running);
        session.pause(at(10)).unwrap();
        assert_eq!(session.state(), SessionState::Paused);
        assert_eq!(
            session.elapsed(at(15)),
            minutes(10),
            "paused time does not count"
        );
        session.resume(at(20)).unwrap();
        assert_eq!(session.state(), SessionState::Running);
        session.pause(at(30)).unwrap();
        session.resume(at(35)).unwrap();
        session.stop(at(50)).unwrap();
        assert_eq!(session.state(), SessionState::Stopped);
        assert_eq!(session.pauses().len(), 2);
        assert_eq!(
            session.elapsed(at(999)),
            minutes(35),
            "50 minutes minus two pauses of 10 and 5"
        );
        assert_eq!(
            session.elapsed(at(25)),
            minutes(15),
            "a stopped session is clamped to `at`: 25 minutes minus the first pause"
        );
        assert_eq!(session.elapsed(at(-5)), SpentTime::ZERO);
        assert_eq!(session.stopped_at(), Some(at(50)));
    }

    #[test]
    fn stopping_while_paused_closes_the_pause() {
        let mut session = Session::start("ses-1", target(), at(0));
        session.pause(at(10)).unwrap();
        session.stop(at(25)).unwrap();
        assert_eq!(session.elapsed(at(100)), minutes(10));
        assert_eq!(session.pauses()[0].ended_at(), Some(at(25)));
    }

    #[test]
    fn transitions_reject_the_wrong_state() {
        let id = SessionId::new("ses-1");
        let running = || Session::start("ses-1", target(), at(0));
        let paused = || {
            let mut session = running();
            session.pause(at(5)).unwrap();
            session
        };
        let stopped = || {
            let mut session = running();
            session.stop(at(5)).unwrap();
            session
        };
        let cases: [TransitionCase; 6] = [
            (
                "pause a paused session",
                paused(),
                Session::pause,
                DomainError::SessionNotRunning { id: id.clone() },
            ),
            (
                "resume a running session",
                running(),
                Session::resume,
                DomainError::SessionNotPaused { id: id.clone() },
            ),
            (
                "pause a stopped session",
                stopped(),
                Session::pause,
                DomainError::SessionAlreadyStopped { id: id.clone() },
            ),
            (
                "resume a stopped session",
                stopped(),
                Session::resume,
                DomainError::SessionAlreadyStopped { id: id.clone() },
            ),
            (
                "stop a stopped session",
                stopped(),
                Session::stop,
                DomainError::SessionAlreadyStopped { id: id.clone() },
            ),
            (
                "pause before the start",
                running(),
                Session::pause,
                DomainError::EndBeforeStart {
                    start: at(0),
                    end: at(10),
                },
            ),
        ];
        for (name, mut session, step, expected) in cases {
            let now = if name == "pause before the start" {
                at(-10)
            } else {
                at(10)
            };
            let expected = match expected {
                DomainError::EndBeforeStart { start, .. } => {
                    DomainError::EndBeforeStart { start, end: now }
                }
                other => other,
            };
            assert_eq!(step(&mut session, now), Err(expected), "{name}");
        }
    }

    #[test]
    fn steps_before_the_previous_step_are_refused() {
        let mut session = Session::start("ses-1", target(), at(10));
        assert_eq!(
            session.stop(at(5)),
            Err(DomainError::EndBeforeStart {
                start: at(10),
                end: at(5)
            }),
            "stop before the start"
        );
        session.pause(at(20)).unwrap();
        assert_eq!(
            session.resume(at(15)),
            Err(DomainError::EndBeforeStart {
                start: at(20),
                end: at(15)
            }),
            "resume before the pause began"
        );
        session.resume(at(25)).unwrap();
        assert_eq!(
            session.pause(at(24)),
            Err(DomainError::EndBeforeStart {
                start: at(25),
                end: at(24)
            }),
            "a second pause before the resume"
        );
        assert_eq!(session.state(), SessionState::Running);
        assert_eq!(session.elapsed(at(30)), minutes(15));
    }

    #[test]
    fn a_session_round_trips_through_its_record() {
        let mut session = Session::start("ses-1", target(), at(0));
        session.pause(at(10)).unwrap();
        session.resume(at(20)).unwrap();
        session.pause(at(30)).unwrap();
        let paused = session.to_record();
        assert_eq!(
            paused.pauses,
            vec![Pause::new(at(10), Some(at(20))), Pause::new(at(30), None),]
        );
        let rebuilt = Session::from_record(paused.clone()).unwrap();
        assert_eq!(rebuilt, session);
        assert_eq!(rebuilt.state(), SessionState::Paused);

        session.resume(at(35)).unwrap();
        assert!(session.check_long_timer(at(60), Duration::from_secs(60)));
        session.stop(at(60)).unwrap();
        let stopped = session.to_record();
        assert!(stopped.has_long_timer_fired);
        let rebuilt = Session::from_record(stopped).unwrap();
        assert_eq!(rebuilt, session);
        assert!(
            rebuilt.has_long_timer_fired(),
            "the alert does not fire again after a reload"
        );
        assert_eq!(rebuilt.elapsed(at(999)), minutes(45));

        let meeting = Session::from_calendar_event(
            "ses-2",
            SessionTarget::story("WEB-142"),
            &CalendarEvent {
                title: "Sprint review".into(),
                start: at(100),
                end: at(130),
            },
        )
        .unwrap();
        assert_eq!(Session::from_record(meeting.to_record()), Ok(meeting));
    }

    #[test]
    fn malformed_records_are_refused() {
        let record = |pauses: Vec<Pause>, stopped_at: Option<Timestamp>| SessionRecord {
            id: SessionId::new("ses-1"),
            target: target(),
            source: SessionSource::Timer,
            started_at: at(10),
            stopped_at,
            pauses,
            has_long_timer_fired: false,
        };
        let malformed = |reason: &'static str| DomainError::MalformedStoredSession {
            id: SessionId::new("ses-1"),
            reason,
        };
        let backwards = |start: i64, end: i64| DomainError::EndBeforeStart {
            start: at(start),
            end: at(end),
        };
        let cases = [
            (
                "a pause before the start",
                record(vec![Pause::new(at(5), None)], None),
                backwards(10, 5),
            ),
            (
                "a pause that ends before it begins",
                record(vec![Pause::new(at(20), Some(at(15)))], None),
                backwards(20, 15),
            ),
            (
                "a pause before the previous resume",
                record(
                    vec![Pause::new(at(20), Some(at(30))), Pause::new(at(25), None)],
                    None,
                ),
                backwards(30, 25),
            ),
            (
                "the stop before the last pause",
                record(vec![Pause::new(at(20), Some(at(30)))], Some(at(25))),
                backwards(30, 25),
            ),
            (
                "the stop before the start",
                record(vec![], Some(at(5))),
                backwards(10, 5),
            ),
            (
                "an open pause that is not the last",
                record(
                    vec![Pause::new(at(20), None), Pause::new(at(30), Some(at(40)))],
                    None,
                ),
                malformed("an open pause is not the last pause"),
            ),
            (
                "a stopped session with an open pause",
                record(vec![Pause::new(at(20), None)], Some(at(30))),
                malformed("a stopped session has an open pause"),
            ),
        ];
        for (name, record, expected) in cases {
            assert_eq!(Session::from_record(record), Err(expected), "{name}");
        }
        let book = SessionBook::from_sessions(vec![
            Session::from_record(record(vec![Pause::new(at(20), Some(at(30)))], Some(at(40))))
                .unwrap(),
        ])
        .unwrap();
        assert_eq!(
            book.spent_on_task(&TaskId::new("task-7"), at(999)),
            minutes(20)
        );
    }

    #[test]
    fn elapsed_never_runs_backwards() {
        let session = Session::start("ses-1", target(), at(10));
        assert_eq!(session.elapsed(at(0)), SpentTime::ZERO);
        assert_eq!(session.elapsed(at(10)), SpentTime::ZERO);
        assert_eq!(session.elapsed(at(11)), minutes(1));
    }

    #[test]
    fn start_minutes_ago_backdates_the_start() {
        let session = Session::start_minutes_ago("ses-1", target(), at(60), 25).unwrap();
        assert_eq!(session.started_at(), at(35));
        assert_eq!(session.elapsed(at(60)), minutes(25));
        assert_eq!(session.state(), SessionState::Running);
        let zero = Session::start_minutes_ago("ses-2", target(), at(60), 0).unwrap();
        assert_eq!(zero.started_at(), at(60));
        assert_eq!(
            Session::start_minutes_ago("ses-3", target(), Timestamp::MIN, 1),
            Err(DomainError::TimeOutOfRange)
        );
    }

    #[test]
    fn meeting_sessions_come_stopped_and_raw() {
        let event = CalendarEvent {
            title: "Sprint review".into(),
            start: Timestamp::from_second(EPOCH + 7).unwrap(),
            end: Timestamp::from_second(EPOCH + 7 + 3_599).unwrap(),
        };
        let session =
            Session::from_calendar_event("ses-9", SessionTarget::story("WEB-142"), &event).unwrap();
        assert_eq!(session.state(), SessionState::Stopped);
        assert!(session.is_meeting());
        assert_eq!(
            session.source(),
            &SessionSource::Meeting {
                title: "Sprint review".into()
            }
        );
        assert_eq!(
            session.elapsed(at(120)).as_secs(),
            3_599,
            "no rounding to the minute"
        );
        assert_eq!(
            session.elapsed(at(0)).as_secs(),
            0,
            "not yet happened as of `at`"
        );
        let backwards = CalendarEvent {
            title: "Backwards".into(),
            start: at(10),
            end: at(5),
        };
        assert_eq!(
            Session::from_calendar_event("ses-10", SessionTarget::default(), &backwards),
            Err(DomainError::EndBeforeStart {
                start: at(10),
                end: at(5)
            })
        );
        let instant = CalendarEvent {
            title: "Instant".into(),
            start: at(5),
            end: at(5),
        };
        let instant =
            Session::from_calendar_event("ses-11", SessionTarget::default(), &instant).unwrap();
        assert_eq!(instant.elapsed(at(10)), SpentTime::ZERO);
    }

    #[test]
    fn the_book_allows_one_active_session() {
        let mut book = SessionBook::new();
        book.start("ses-1", target(), at(0)).unwrap();
        let second = book.start("ses-2", target(), at(1));
        assert_eq!(
            second,
            Err(DomainError::SessionAlreadyRunning {
                running: SessionId::new("ses-1")
            })
        );
        book.pause(at(2)).unwrap();
        let while_paused = book.start_minutes_ago("ses-3", target(), at(3), 1);
        assert_eq!(
            while_paused,
            Err(DomainError::SessionAlreadyRunning {
                running: SessionId::new("ses-1")
            })
        );
        assert_eq!(
            book.continue_latest("ses-4", at(3)),
            Err(DomainError::SessionAlreadyRunning {
                running: SessionId::new("ses-1")
            })
        );
        book.stop(at(4)).unwrap();
        assert!(book.active().is_none());
        book.start("ses-5", target(), at(5)).unwrap();
        assert_eq!(
            book.active().map(|session| session.id().as_str()),
            Some("ses-5")
        );
        assert_eq!(book.sessions().len(), 2);
    }

    #[test]
    fn pausing_resuming_or_stopping_nothing_is_an_error() {
        let mut book = SessionBook::new();
        assert_eq!(book.pause(at(0)), Err(DomainError::NoRunningSession));
        assert_eq!(book.resume(at(0)), Err(DomainError::NoRunningSession));
        assert_eq!(book.stop(at(0)), Err(DomainError::NoRunningSession));
        assert_eq!(
            book.continue_latest("ses-1", at(0)),
            Err(DomainError::NoSessionToContinue)
        );
        assert!(book.latest().is_none());
        assert_eq!(
            book.spent_on_task(&TaskId::new("task-7"), at(0)),
            SpentTime::ZERO
        );
    }

    #[test]
    fn continue_latest_reuses_the_latest_target() {
        let mut book = SessionBook::new();
        book.start("ses-1", SessionTarget::task("task-1"), at(0))
            .unwrap();
        book.stop(at(10)).unwrap();
        book.start(
            "ses-2",
            SessionTarget::task_in_story("task-2", "WEB-7"),
            at(20),
        )
        .unwrap();
        book.stop(at(30)).unwrap();
        let continued = book.continue_latest("ses-3", at(40)).unwrap();
        assert_eq!(
            continued.target(),
            &SessionTarget::task_in_story("task-2", "WEB-7")
        );
        assert_eq!(continued.started_at(), at(40));
        assert_eq!(continued.source(), &SessionSource::Timer);
        assert_eq!(
            book.spent_on_task(&TaskId::new("task-2"), at(45)),
            minutes(15)
        );
    }

    #[test]
    fn meetings_record_beside_a_running_timer_and_count_for_the_story() {
        let mut book = SessionBook::new();
        book.start(
            "ses-1",
            SessionTarget::task_in_story("task-1", "WEB-142"),
            at(0),
        )
        .unwrap();
        let event = CalendarEvent {
            title: "Stand-up".into(),
            start: at(-60),
            end: at(-45),
        };
        book.record_meeting("ses-2", SessionTarget::story("WEB-142"), &event)
            .unwrap();
        assert_eq!(
            book.active().map(|session| session.id().as_str()),
            Some("ses-1")
        );
        assert_eq!(
            book.spent_on_story(&StoryNumber::new("WEB-142"), at(30)),
            minutes(45)
        );
        assert_eq!(
            book.spent_on_task(&TaskId::new("task-1"), at(30)),
            minutes(30)
        );
        assert_eq!(
            book.latest().map(|session| session.id().as_str()),
            Some("ses-2"),
            "the meeting was recorded last"
        );
        assert_eq!(
            book.latest_timer().map(|session| session.id().as_str()),
            Some("ses-1")
        );
    }

    #[test]
    fn from_sessions_rejects_two_active_sessions_and_duplicate_ids() {
        let two_active = SessionBook::from_sessions(vec![
            Session::start("ses-1", target(), at(0)),
            Session::start("ses-2", target(), at(1)),
        ]);
        assert_eq!(
            two_active.map(|_| ()),
            Err(DomainError::MultipleActiveSessions { count: 2 })
        );
        let mut stopped = Session::start("ses-1", target(), at(0));
        stopped.stop(at(5)).unwrap();
        let duplicate =
            SessionBook::from_sessions(vec![stopped, Session::start("ses-1", target(), at(6))]);
        assert_eq!(
            duplicate.map(|_| ()),
            Err(DomainError::DuplicateSessionId {
                id: SessionId::new("ses-1")
            })
        );
        let book =
            SessionBook::from_sessions(vec![Session::start("ses-1", target(), at(0))]).unwrap();
        assert_eq!(
            book.active().map(|session| session.id().as_str()),
            Some("ses-1")
        );
        assert_eq!(
            SessionBook::from_sessions(vec![]).unwrap(),
            SessionBook::new()
        );
    }

    #[test]
    fn duplicate_ids_are_refused_on_every_way_in() {
        let mut book = SessionBook::new();
        book.start("ses-1", target(), at(0)).unwrap();
        book.stop(at(5)).unwrap();
        let taken = Err(DomainError::DuplicateSessionId {
            id: SessionId::new("ses-1"),
        });
        assert_eq!(book.start("ses-1", target(), at(6)), taken);
        assert_eq!(book.start_minutes_ago("ses-1", target(), at(6), 1), taken);
        assert_eq!(book.continue_latest("ses-1", at(6)), taken);
        let event = CalendarEvent {
            title: "Stand-up".into(),
            start: at(10),
            end: at(20),
        };
        assert_eq!(book.record_meeting("ses-1", target(), &event), taken);
        assert_eq!(book.sessions().len(), 1);
    }

    #[test]
    fn backdated_starts_never_count_a_minute_twice() {
        let mut book = SessionBook::new();
        book.start("ses-1", target(), at(0)).unwrap();
        book.stop(at(60)).unwrap();
        let overlap = book.start_minutes_ago("ses-2", target(), at(90), 31);
        assert_eq!(
            overlap,
            Err(DomainError::OverlapsPreviousSession {
                previous: SessionId::new("ses-1"),
                ended_at: at(60)
            })
        );
        book.start_minutes_ago("ses-2", target(), at(90), 30)
            .unwrap();
        assert_eq!(
            book.spent_on_task(&TaskId::new("task-7"), at(90)),
            minutes(90)
        );
        book.stop(at(90)).unwrap();
        let event = CalendarEvent {
            title: "Review".into(),
            start: at(100),
            end: at(130),
        };
        book.record_meeting("ses-3", SessionTarget::story("WEB-142"), &event)
            .unwrap();
        assert_eq!(
            book.start_minutes_ago("ses-4", target(), at(140), 20),
            Err(DomainError::OverlapsPreviousSession {
                previous: SessionId::new("ses-3"),
                ended_at: at(130)
            })
        );
    }

    #[test]
    fn continue_latest_follows_recording_order_and_skips_meetings() {
        let mut book = SessionBook::new();
        book.start("ses-1", SessionTarget::task("task-1"), at(0))
            .unwrap();
        book.stop(at(10)).unwrap();
        book.start("ses-2", SessionTarget::task("task-2"), at(20))
            .unwrap();
        book.stop(at(25)).unwrap();
        let event = CalendarEvent {
            title: "Planning".into(),
            start: at(30),
            end: at(60),
        };
        book.record_meeting("ses-3", SessionTarget::story("WEB-7"), &event)
            .unwrap();
        assert_eq!(
            book.latest().map(|session| session.id().as_str()),
            Some("ses-3")
        );
        assert_eq!(
            book.latest_timer().map(|session| session.id().as_str()),
            Some("ses-2")
        );
        let continued = book.continue_latest("ses-4", at(300)).unwrap();
        assert_eq!(continued.target(), &SessionTarget::task("task-2"));
        let mut meetings_only = SessionBook::new();
        meetings_only
            .record_meeting("ses-9", SessionTarget::story("WEB-7"), &event)
            .unwrap();
        assert_eq!(
            meetings_only.continue_latest("ses-10", at(300)),
            Err(DomainError::NoSessionToContinue)
        );
    }

    #[test]
    fn resume_reports_the_state_before_the_order() {
        let mut session = Session::start("ses-1", target(), at(10));
        assert_eq!(
            session.resume(at(0)),
            Err(DomainError::SessionNotPaused {
                id: SessionId::new("ses-1")
            })
        );
    }

    #[test]
    fn long_timer_fires_once_per_session() {
        let limit = Duration::from_secs(4 * 3600);
        let notifier = RecordingNotifier::new();
        let mut book = SessionBook::new();
        assert!(
            !book.notify_long_timer(at(0), limit, &notifier),
            "nothing running"
        );
        book.start("ses-1", target(), at(0)).unwrap();
        let checks = [
            ("early", 239, false),
            ("at the limit", 240, true),
            ("later", 300, false),
        ];
        for (name, minute, fires) in checks {
            assert_eq!(
                book.notify_long_timer(at(minute), limit, &notifier),
                fires,
                "{name}"
            );
        }
        book.pause(at(301)).unwrap();
        assert!(
            !book.notify_long_timer(at(400), limit, &notifier),
            "already fired"
        );
        book.stop(at(401)).unwrap();
        book.start("ses-2", target(), at(500)).unwrap();
        book.pause(at(501)).unwrap();
        assert!(
            !book.notify_long_timer(at(999), limit, &notifier),
            "paused sessions never fire"
        );
        book.resume(at(502)).unwrap();
        assert!(
            !book.notify_long_timer(at(740), limit, &notifier),
            "the pause does not count"
        );
        assert!(
            book.notify_long_timer(at(741), limit, &notifier),
            "a new session fires again"
        );
        assert_eq!(
            notifier.sent(),
            vec![
                Notification::LongTimer {
                    session: SessionId::new("ses-1"),
                    elapsed: minutes(240)
                },
                Notification::LongTimer {
                    session: SessionId::new("ses-2"),
                    elapsed: minutes(240)
                },
            ]
        );
    }
}
