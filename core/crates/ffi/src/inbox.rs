//! The Inbox over the bridge: the signals with what triage pre-fills from
//! their plain fields, and the outcomes that write. No model is involved:
//! the headline is the cleaned subject, the sender the name the message
//! gives, the ticket the first number the workspace's `ticket_pattern`
//! finds (see [`app_state::triage`]).
//!
//! A conversion writes one task line through the vault writer and records
//! the signal as converted with that task's id, under one lock of the
//! workspace document, so it happens once: a second attempt is refused, and
//! a line that already carries `[source:: sig-…]` (written on another
//! device, or before a crash) is found instead of written again. The line
//! carries only the cleaned headline, or what the user typed instead, and
//! `[source:: sig-…]`; the excerpt and the message text stay in the
//! encrypted store.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use app_state::triage::{self, SignalPrefill};
use app_state::{SignalAction, SignalId, SignalRecord, SignalState};
use domain::task::TaskId;
use domain::ticket::TicketPattern;
use jiff::{Timestamp, Zoned};
use vault_base::ops::{self, OpError};
use vault_base::{NewTask, PlainFileAccess, TaskSection, Vault, VaultWriter};

use crate::app_state::{WorkspaceStateHandle, lock};
use crate::home::parse_estimate_minutes;
use crate::mail_accounts::{SignalStateView, StateParts, state_parts};
use crate::timer::NotifierPort;
use crate::{CoreError, WorkspaceHandle};

/// The note a signal converted without a story is filed into.
pub const MAIL_INTAKE_NOTE: &str = "Inbox/Mail.md";

/// One signal as the Inbox column and the triage screen show it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct InboxSignalView {
    /// `sig-…`.
    pub id: String,
    /// The account's connection id.
    pub account: String,
    /// The sender's name, else the address.
    pub sender: String,
    /// The sender's address.
    pub sender_address: String,
    /// RFC 3339 arrival time.
    pub received_at: String,
    /// The subject as it arrived.
    pub subject: String,
    /// The subject cleaned for a task line: what + Task pre-fills.
    pub headline: String,
    /// A short excerpt, shown in the preview only.
    pub excerpt: String,
    /// The first ticket number `ticket_pattern` finds, subject before excerpt.
    pub ticket: Option<String>,
    /// The title of the story with that number, when the vault holds one.
    pub ticket_story_title: Option<String>,
    /// The state as of now.
    pub state: SignalStateView,
    /// RFC 3339 end of a snooze.
    pub snoozed_until: Option<String>,
    /// The task a converted signal became.
    pub task: Option<String>,
    /// The story an attached signal is kept on.
    pub story: Option<String>,
}

/// Which section a conversion writes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum SignalOutcomeView {
    /// + Task: a line under `## Next Steps`.
    Task,
    /// Wait for the sender: a line under `## Pending Input` with `assigned_to`.
    WaitFor,
}

/// A confirmed + Task or Wait for popover.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SignalConversionInput {
    /// The signal, `sig-…`.
    pub signal: String,
    /// + Task or Wait for.
    pub outcome: SignalOutcomeView,
    /// The task text; the popover starts it from the headline.
    pub text: String,
    /// The story number whose note gets the line; `None` files it into
    /// [`MAIL_INTAKE_NOTE`].
    pub story: Option<String>,
    /// Who the line waits on (Wait for); empty falls back to the sender.
    pub assigned_to: Option<String>,
    /// `YYYY-MM-DD` planned day, written as `[scheduled:: ]`.
    pub scheduled: Option<String>,
    /// An estimate such as `45m`, written as `[estimate:: ]`.
    pub estimate: Option<String>,
}

/// What a conversion wrote.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SignalConversionView {
    /// The new task's id.
    pub task_id: String,
    /// The note, relative to the workspace root.
    pub note: String,
    /// The line as written.
    pub line: String,
}

#[uniffi::export]
impl WorkspaceStateHandle {
    /// Every signal, newest first, with what triage pre-fills: the cleaned
    /// headline, the sender and the ticket number under the workspace's
    /// `ticket_pattern`, with the matching story's title when there is one.
    ///
    /// # Errors
    /// [`CoreError::InvalidSettings`] when the pattern does not compile,
    /// [`CoreError::FileAccess`] when the vault cannot be read.
    pub fn inbox_signals(
        &self,
        workspace: &WorkspaceHandle,
    ) -> Result<Vec<InboxSignalView>, CoreError> {
        let pattern = ticket_pattern(workspace)?;
        let vault = ops::open_vault(&workspace.workspace, &PlainFileAccess)?;
        let now = Timestamp::now();
        let records = lock(&self.sync).state().signals();
        Ok(records
            .into_iter()
            .map(|record| inbox_view(record, pattern.as_ref(), &vault, now))
            .collect())
    }

    /// Turns a signal into one task line, once: under `## Next Steps` for
    /// + Task, under `## Pending Input` with `assigned_to` for Wait for, in
    /// the story's note or else [`MAIL_INTAKE_NOTE`]. The line ends in
    /// `[source:: sig-…]` and the signal is stored as converted with the
    /// task's id. `today` is the creation day and defaults to the host's.
    ///
    /// # Errors
    /// [`CoreError::NotFound`] for an unknown signal or story,
    /// [`CoreError::Conflict`] when the signal is already converted or
    /// otherwise decided (a line found carrying its source is recorded and
    /// refused too), [`CoreError::InvalidInput`] or [`CoreError::InvalidDate`]
    /// for an unusable text, estimate or day, [`CoreError::FileAccess`] when
    /// the vault cannot be written.
    #[uniffi::method(default(today = None))]
    pub fn convert_signal(
        &self,
        workspace: &WorkspaceHandle,
        input: SignalConversionInput,
        today: Option<String>,
    ) -> Result<SignalConversionView, CoreError> {
        let created = match today.as_deref() {
            Some(text) => ops::parse_date(text)?,
            None => local_today()?,
        };
        let scheduled = non_empty(input.scheduled.as_deref())
            .map(|text| ops::parse_date(&text))
            .transpose()?;
        let estimate = non_empty(input.estimate.as_deref())
            .map(|text| checked_estimate(&text))
            .transpose()?;
        let id = SignalId::new(input.signal.as_str());
        let now = Timestamp::now();
        // One lock over the check, the write and the record keeps a double
        // press from writing two lines.
        let mut sync = lock(&self.sync);
        let record = sync
            .state()
            .signal(&id)
            .ok_or_else(|| CoreError::NotFound(format!("no signal {id}")))?;
        refuse_decided(&id, &record.state.effective(now))?;
        let vault = ops::open_vault(&workspace.workspace, &PlainFileAccess)?;
        if let Some(existing) = task_of_source(&vault, &id) {
            sync.edit(Instant::now(), |state| {
                state.apply_signal_action(
                    &id,
                    &SignalAction::Convert {
                        task: Some(existing.clone()),
                    },
                    now,
                )
            })?;
            return Err(CoreError::Conflict(format!(
                "{id} is already the task {existing}"
            )));
        }
        let writer = VaultWriter::new(&PlainFileAccess, &workspace.workspace);
        let note = target_note(&vault, &writer, input.story.as_deref())?;
        let taken: HashSet<String> = vault
            .tasks()
            .filter_map(|found| found.task.id().map(str::to_owned))
            .collect();
        let task_id = sync.edit(Instant::now(), |state| {
            state.mint_task_id(&|candidate| taken.contains(candidate))
        })?;
        let (section, assigned_to) = match input.outcome {
            SignalOutcomeView::Task => (TaskSection::NextSteps, None),
            SignalOutcomeView::WaitFor => (
                TaskSection::PendingInput,
                Some(
                    non_empty(input.assigned_to.as_deref())
                        .map(|name| triage::line_text(&name))
                        .unwrap_or_else(|| triage::sender_label(&record.signal)),
                ),
            ),
        };
        let task = NewTask {
            // The popover starts from the cleaned headline; whatever arrives
            // is made one plain line again, so no `%%` or line break lands.
            text: triage::line_text(&input.text),
            id: task_id.as_str().to_owned(),
            created,
            assigned_to,
            scheduled,
            estimate,
            due: None,
            reminder: None,
            source: Some(id.as_str().to_owned()),
        };
        let receipt = writer
            .add_task(&note, section, &task)
            .map_err(OpError::from)?;
        sync.edit(Instant::now(), |state| {
            state.apply_signal_action(
                &id,
                &SignalAction::Convert {
                    task: Some(task_id.clone()),
                },
                now,
            )
        })?;
        Ok(SignalConversionView {
            task_id: task_id.as_str().to_owned(),
            note: note.to_string_lossy().into_owned(),
            line: receipt.new_line,
        })
    }

    /// Keeps a signal on a story for reference, without a task; nothing is
    /// written to the vault.
    ///
    /// # Errors
    /// [`CoreError::NotFound`] for an unknown signal or story,
    /// [`CoreError::AppState`] when the signal is already decided.
    pub fn attach_signal(
        &self,
        workspace: &WorkspaceHandle,
        id: String,
        story: String,
    ) -> Result<SignalStateView, CoreError> {
        let vault = ops::open_vault(&workspace.workspace, &PlainFileAccess)?;
        let number = story.trim();
        if vault.story_by_number(number).is_none() {
            return Err(CoreError::NotFound(format!(
                "No story {number} in this workspace"
            )));
        }
        self.act_on_signal(
            &id,
            &SignalAction::Attach {
                story: number.to_owned(),
            },
        )
    }

    /// "Not a signal": puts a signal away for good, apart from a dismissal.
    ///
    /// # Errors
    /// [`CoreError::NotFound`] when the signal is unknown,
    /// [`CoreError::AppState`] when it is already decided.
    pub fn reject_signal(&self, id: String) -> Result<SignalStateView, CoreError> {
        self.act_on_signal(&id, &SignalAction::Reject)
    }

    /// Tells `notifier` once that a fetch added `added` signals, with how
    /// many are new now; a fetch that added none tells nothing. Returns
    /// whether it told.
    pub fn announce_new_signals(
        &self,
        workspace: String,
        added: u32,
        notifier: Arc<dyn NotifierPort>,
    ) -> bool {
        if added == 0 {
            return false;
        }
        let waiting = self.new_signal_count();
        notifier.new_signals(workspace, added, waiting);
        true
    }
}

fn inbox_view(
    record: SignalRecord,
    pattern: Option<&TicketPattern>,
    vault: &Vault,
    now: Timestamp,
) -> InboxSignalView {
    let SignalPrefill {
        headline,
        sender,
        ticket,
    } = triage::prefill(&record.signal, pattern);
    let ticket_story_title = ticket.as_deref().and_then(|number| {
        let entry = vault.story_by_number(number)?;
        Some(
            entry
                .story
                .title
                .clone()
                .unwrap_or_else(|| number.to_owned()),
        )
    });
    let StateParts {
        state,
        snoozed_until,
        task,
        story,
    } = state_parts(&record.state.effective(now));
    let signal = record.signal;
    InboxSignalView {
        id: signal.id.as_str().to_owned(),
        account: signal.account.as_str().to_owned(),
        sender,
        sender_address: signal.sender_address,
        received_at: signal.received_at.to_string(),
        subject: signal.subject,
        headline,
        excerpt: signal.excerpt,
        ticket,
        ticket_story_title,
        state,
        snoozed_until,
        task,
        story,
    }
}

fn ticket_pattern(workspace: &WorkspaceHandle) -> Result<Option<TicketPattern>, CoreError> {
    Ok(workspace
        .workspace
        .settings
        .ticket_pattern
        .as_deref()
        .map(TicketPattern::new)
        .transpose()?)
}

/// Only a new or snoozed signal may become a task.
fn refuse_decided(id: &SignalId, state: &SignalState) -> Result<(), CoreError> {
    match state {
        SignalState::New | SignalState::Snoozed { .. } => Ok(()),
        SignalState::Converted { task: Some(task) } => Err(CoreError::Conflict(format!(
            "{id} is already the task {task}"
        ))),
        decided => Err(CoreError::Conflict(format!(
            "{id} is {} and cannot become a task",
            decided.label()
        ))),
    }
}

/// The task whose line already carries `[source:: id]`.
fn task_of_source(vault: &Vault, id: &SignalId) -> Option<TaskId> {
    vault
        .tasks()
        .find(|found| found.task.field("source") == Some(id.as_str()))
        .and_then(|found| found.task.id().map(TaskId::new))
}

/// The story's note, else the mail intake note, created when missing.
fn target_note(
    vault: &Vault,
    writer: &VaultWriter<'_>,
    story: Option<&str>,
) -> Result<PathBuf, CoreError> {
    match story.map(str::trim).filter(|number| !number.is_empty()) {
        Some(number) => vault
            .story_by_number(number)
            .map(|entry| entry.path.to_path_buf())
            .ok_or_else(|| CoreError::NotFound(format!("No story {number} in this workspace"))),
        None => {
            let path = Path::new(MAIL_INTAKE_NOTE);
            writer
                .create_intake_note(path, "Mail")
                .map_err(OpError::from)?;
            Ok(path.to_path_buf())
        }
    }
}

fn checked_estimate(text: &str) -> Result<String, CoreError> {
    if parse_estimate_minutes(text.to_owned()).is_some() {
        Ok(text.to_owned())
    } else {
        Err(CoreError::InvalidInput(format!(
            "{text} is not an estimate; write it as 45m, 2h or 1h 30m"
        )))
    }
}

fn non_empty(text: Option<&str>) -> Option<String> {
    text.map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

fn local_today() -> Result<vault_base::Date, CoreError> {
    Ok(ops::parse_date(&Zoned::now().date().to_string())?)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::Mutex;

    use app_state::{ConnectionId, IncomingSignal};
    use serde_json::Value;

    use super::*;
    use crate::browse::{browse_presets, browse_snapshot};
    use crate::home::tests::{open, sandbox, state};
    use crate::home::{NewTaskPlacementView, day_list};

    const WEB_142: &str = "Projects/Website/WEB-142 Checkout redesign/WEB-142 Checkout redesign.md";
    const TODAY: &str = "2026-10-23";

    /// The synthetic signals of `core/fixtures/signals/inbox.json`, with what
    /// triage must pre-fill from each.
    struct Fixture {
        incoming: IncomingSignal,
        headline: String,
        sender: String,
        ticket: Option<String>,
    }

    fn fixture() -> Vec<Fixture> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/signals/inbox.json");
        let json: Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
        let text = |value: &Value, key: &str| value[key].as_str().unwrap().to_owned();
        json["signals"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| Fixture {
                incoming: IncomingSignal {
                    account: ConnectionId::new(text(entry, "account")),
                    folder: text(entry, "folder"),
                    provider_ref: text(entry, "provider_ref"),
                    message_id: text(entry, "message_id"),
                    thread_id: text(entry, "thread_id"),
                    sender_name: entry["sender_name"].as_str().map(str::to_owned),
                    sender_address: text(entry, "sender_address"),
                    received_at: text(entry, "received_at").parse().unwrap(),
                    subject: text(entry, "subject"),
                    excerpt: text(entry, "excerpt"),
                    link: text(entry, "link"),
                    raw_text: text(entry, "raw_text"),
                    full_text_bytes: 0,
                },
                headline: text(&entry["expected"], "headline"),
                sender: text(&entry["expected"], "sender"),
                ticket: entry["expected"]["ticket"].as_str().map(str::to_owned),
            })
            .collect()
    }

    /// A sandboxed mock vault with its workspace state holding the fixture.
    fn inbox() -> (
        tempfile::TempDir,
        Arc<WorkspaceHandle>,
        Arc<WorkspaceStateHandle>,
        Vec<Fixture>,
    ) {
        let sandbox = sandbox();
        let workspace = open(sandbox.path());
        let state = state(sandbox.path());
        let signals = fixture();
        for signal in &signals {
            lock(&state.sync)
                .edit(Instant::now(), |doc| doc.record_signal(&signal.incoming))
                .unwrap();
        }
        (sandbox, workspace, state, signals)
    }

    fn task_input(signal: &Fixture, story: Option<&str>) -> SignalConversionInput {
        SignalConversionInput {
            signal: signal.incoming.id().as_str().to_owned(),
            outcome: SignalOutcomeView::Task,
            text: signal.headline.clone(),
            story: story.map(str::to_owned),
            assigned_to: None,
            scheduled: None,
            estimate: None,
        }
    }

    fn view_of<'a>(views: &'a [InboxSignalView], signal: &Fixture) -> &'a InboxSignalView {
        views
            .iter()
            .find(|view| view.id == signal.incoming.id().as_str())
            .unwrap()
    }

    /// The lines of `section` in a note's text, without the heading.
    fn section_lines<'a>(text: &'a str, heading: &str) -> Vec<&'a str> {
        text.lines()
            .skip_while(|line| *line != heading)
            .skip(1)
            .take_while(|line| !line.starts_with("## "))
            .filter(|line| !line.trim().is_empty())
            .collect()
    }

    #[derive(Default)]
    struct Recorded {
        announced: Mutex<Vec<(String, u32, u32)>>,
    }

    impl NotifierPort for Recorded {
        fn long_timer(&self, _: String, _: u64) {}
        fn threshold_crossed(&self, _: String, _: Option<String>, _: u64, _: u64) {}
        fn new_signals(&self, workspace: String, added: u32, waiting: u32) {
            self.announced
                .lock()
                .unwrap()
                .push((workspace, added, waiting));
        }
        fn close_out_missing(&self, _: String, _: String, _: Vec<String>) {}
    }

    #[test]
    fn signals_are_prefilled_from_their_plain_fields_and_the_ticket_pattern() {
        let (_sandbox, workspace, state, signals) = inbox();
        let views = state.inbox_signals(&workspace).unwrap();
        assert_eq!(views.len(), signals.len());
        assert!(
            views
                .windows(2)
                .all(|pair| pair[0].received_at >= pair[1].received_at),
            "newest first"
        );
        for signal in &signals {
            let view = view_of(&views, signal);
            assert_eq!(view.headline, signal.headline);
            assert_eq!(view.sender, signal.sender);
            assert_eq!(view.ticket, signal.ticket);
            assert_eq!(view.state, SignalStateView::New);
        }
        assert_eq!(
            view_of(&views, &signals[0]).ticket_story_title.as_deref(),
            Some("Checkout redesign")
        );
        assert_eq!(
            view_of(&views, &signals[1]).ticket_story_title.as_deref(),
            Some("Invoice export")
        );
        assert_eq!(view_of(&views, &signals[2]).ticket_story_title, None);
    }

    #[test]
    fn a_task_line_is_written_byte_for_byte_and_only_once() {
        let (sandbox, workspace, state, signals) = inbox();
        let signal = &signals[0];
        let id = signal.incoming.id();
        let before = fs::read_to_string(sandbox.path().join(WEB_142)).unwrap();
        let converted = state
            .convert_signal(
                &workspace,
                task_input(signal, Some("WEB-142")),
                Some(TODAY.to_owned()),
            )
            .unwrap();
        let expected = format!(
            "- [ ] WEB-142 checkout, two comments on the summary [id:: {}] [created:: 2026-10-23] [source:: {id}]",
            converted.task_id
        );
        assert_eq!(converted.line, expected);
        assert_eq!(converted.note, WEB_142);
        let after = fs::read_to_string(sandbox.path().join(WEB_142)).unwrap();
        let next_steps = section_lines(&after, "## Next Steps");
        assert_eq!(next_steps.last(), Some(&expected.as_str()));
        assert_eq!(after.matches(id.as_str()).count(), 1);
        assert_eq!(
            after.lines().count(),
            before.lines().count() + 1,
            "one line added, nothing else"
        );
        for kept_out in [
            signal.incoming.excerpt.as_str(),
            "SYNTHETIC-BODY-MARKER",
            "lena@example.test",
        ] {
            assert!(!after.contains(kept_out), "{kept_out} reached the vault");
        }
        let view = state
            .inbox_signals(&workspace)
            .unwrap()
            .into_iter()
            .find(|view| view.id == id.as_str())
            .unwrap();
        assert_eq!(view.state, SignalStateView::Converted);
        assert_eq!(view.task.as_deref(), Some(converted.task_id.as_str()));
        let again = state.convert_signal(
            &workspace,
            task_input(signal, Some("WEB-142")),
            Some(TODAY.to_owned()),
        );
        assert!(matches!(again, Err(CoreError::Conflict(_))), "{again:?}");
        assert_eq!(
            fs::read_to_string(sandbox.path().join(WEB_142)).unwrap(),
            after,
            "a refused second attempt writes nothing"
        );
        assert_eq!(
            lock(&state.sync).state().signal_raw_text(&id).as_deref(),
            Some(signal.incoming.raw_text.as_str()),
            "the source text stays in the encrypted store"
        );
    }

    #[test]
    fn wait_for_writes_a_pending_input_line_with_the_sender() {
        let (sandbox, workspace, state, signals) = inbox();
        let signal = &signals[1];
        let converted = state
            .convert_signal(
                &workspace,
                SignalConversionInput {
                    outcome: SignalOutcomeView::WaitFor,
                    ..task_input(signal, None)
                },
                Some(TODAY.to_owned()),
            )
            .unwrap();
        assert_eq!(converted.note, MAIL_INTAKE_NOTE);
        let expected = format!(
            "- [ ] (ext) Export failed overnight [id:: {}] [created:: 2026-10-23] [assigned_to:: billing-bot@example.test] [source:: {}]",
            converted.task_id,
            signal.incoming.id()
        );
        assert_eq!(converted.line, expected);
        let note = fs::read_to_string(sandbox.path().join(MAIL_INTAKE_NOTE)).unwrap();
        assert_eq!(
            section_lines(&note, "## Pending Input"),
            [expected.as_str()]
        );
        assert!(note.starts_with("---\ntype: story\n---\n# Mail\n"));
    }

    #[test]
    fn a_planned_task_carries_its_day_and_estimate_before_the_source() {
        let (_sandbox, workspace, state, signals) = inbox();
        let signal = &signals[2];
        let converted = state
            .convert_signal(
                &workspace,
                SignalConversionInput {
                    text: "Check the first load on staging".to_owned(),
                    scheduled: Some(TODAY.to_owned()),
                    estimate: Some("30m".to_owned()),
                    ..task_input(signal, Some("WEB-157"))
                },
                Some(TODAY.to_owned()),
            )
            .unwrap();
        assert_eq!(
            converted.line,
            format!(
                "- [ ] Check the first load on staging [id:: {}] [created:: 2026-10-23] [scheduled:: 2026-10-23] [estimate:: 30m] [source:: {}]",
                converted.task_id,
                signal.incoming.id()
            )
        );
        for (field, value) in [("estimate", "soon"), ("scheduled", "tomorrowish")] {
            let mut input = task_input(&signals[0], None);
            match field {
                "estimate" => input.estimate = Some(value.to_owned()),
                _ => input.scheduled = Some(value.to_owned()),
            }
            assert!(
                state
                    .convert_signal(&workspace, input, Some(TODAY.to_owned()))
                    .is_err(),
                "{field} {value} is refused"
            );
        }
        assert!(matches!(
            state.convert_signal(
                &workspace,
                task_input(&signals[0], Some("WEB-999")),
                Some(TODAY.to_owned())
            ),
            Err(CoreError::NotFound(_))
        ));
        let new = state.inbox_signals(&workspace).unwrap();
        assert_eq!(
            view_of(&new, &signals[0]).state,
            SignalStateView::New,
            "a refused conversion leaves the signal in triage"
        );
    }

    #[test]
    fn a_line_already_carrying_the_source_is_recorded_and_refused() {
        let (sandbox, workspace, state, signals) = inbox();
        let signal = &signals[0];
        let path = sandbox.path().join(WEB_142);
        let text = fs::read_to_string(&path).unwrap().replace(
            "## Pending Input\n",
            &format!(
                "- [ ] Written on another Mac [id:: task-900] [created:: 2026-10-22] [source:: {}]\n\n## Pending Input\n",
                signal.incoming.id()
            ),
        );
        fs::write(&path, &text).unwrap();
        let refused = state.convert_signal(
            &workspace,
            task_input(signal, Some("WEB-142")),
            Some(TODAY.to_owned()),
        );
        assert!(
            matches!(refused, Err(CoreError::Conflict(_))),
            "{refused:?}"
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), text);
        let view = state.inbox_signals(&workspace).unwrap();
        assert_eq!(view_of(&view, signal).state, SignalStateView::Converted);
        assert_eq!(view_of(&view, signal).task.as_deref(), Some("task-900"));
    }

    #[test]
    fn snooze_dismiss_reject_and_attach_move_the_state_machine() {
        let (sandbox, workspace, state, signals) = inbox();
        let [first, second, third] = [&signals[0], &signals[1], &signals[2]];
        let id = |signal: &Fixture| signal.incoming.id().as_str().to_owned();
        let until = Timestamp::now()
            .checked_add(jiff::SignedDuration::from_hours(24))
            .unwrap()
            .to_string();
        assert_eq!(
            state.snooze_signal(id(first), until.clone()).unwrap(),
            SignalStateView::Snoozed
        );
        assert!(
            state
                .snooze_signal(id(first), "2020-01-01T00:00:00Z".to_owned())
                .is_err()
        );
        assert_eq!(
            state.dismiss_signal(id(first)).unwrap(),
            SignalStateView::Dismissed,
            "a snoozed signal can still be dismissed"
        );
        assert!(matches!(
            state.convert_signal(&workspace, task_input(first, None), Some(TODAY.to_owned())),
            Err(CoreError::Conflict(_))
        ));
        assert!(
            !sandbox.path().join(MAIL_INTAKE_NOTE).exists(),
            "nothing written"
        );
        assert_eq!(
            state.reject_signal(id(second)).unwrap(),
            SignalStateView::Rejected
        );
        assert!(state.snooze_signal(id(second), until).is_err());
        assert!(matches!(
            state.attach_signal(&workspace, id(third), "WEB-999".to_owned()),
            Err(CoreError::NotFound(_))
        ));
        assert_eq!(
            state
                .attach_signal(&workspace, id(third), "WEB-142".to_owned())
                .unwrap(),
            SignalStateView::Attached
        );
        let views = state.inbox_signals(&workspace).unwrap();
        assert_eq!(view_of(&views, third).story.as_deref(), Some("WEB-142"));
        assert_eq!(state.new_signal_count(), 0);
        let note = fs::read_to_string(sandbox.path().join(WEB_142)).unwrap();
        assert!(
            !note.contains("sig-"),
            "attaching writes nothing to the vault"
        );
    }

    #[test]
    fn triage_state_stays_out_of_today_browse_and_the_hours_until_accepted() {
        let (_sandbox, workspace, state, signals) = inbox();
        let until = Timestamp::now()
            .checked_add(jiff::SignedDuration::from_hours(48))
            .unwrap()
            .to_string();
        state
            .snooze_signal(signals[1].incoming.id().as_str().to_owned(), until)
            .unwrap();
        assert_eq!(state.new_signal_count(), 2, "the snoozed one is not new");
        let planning_views = |state: &WorkspaceStateHandle| -> String {
            let today = day_list(
                &workspace,
                TODAY.to_owned(),
                Vec::new(),
                NewTaskPlacementView::Bottom,
                Some(TODAY.to_owned()),
            )
            .unwrap();
            let snapshot = browse_snapshot(&workspace, Some(TODAY.to_owned())).unwrap();
            let browse: Vec<String> = browse_presets()
                .into_iter()
                .map(|preset| format!("{:?}", snapshot.query(preset.query).unwrap()))
                .collect();
            let hours = state.hours(&workspace, TODAY.to_owned()).unwrap();
            let day_time = state.day_time(TODAY.to_owned()).unwrap();
            format!("{today:?}{browse:?}{hours:?}{day_time:?}")
        };
        let untouched = planning_views(&state);
        for signal in &signals {
            for text in [
                signal.headline.as_str(),
                signal.incoming.subject.as_str(),
                signal.incoming.id().as_str(),
            ] {
                assert!(!untouched.contains(text), "{text} reached a planning view");
            }
        }
        assert!(!untouched.contains("sig-"));
        let accepted = state
            .convert_signal(
                &workspace,
                SignalConversionInput {
                    scheduled: Some(TODAY.to_owned()),
                    ..task_input(&signals[0], Some("WEB-142"))
                },
                Some(TODAY.to_owned()),
            )
            .unwrap();
        let after = planning_views(&state);
        assert!(
            after.contains(&accepted.task_id),
            "the accepted task is planned"
        );
        assert!(after.contains(signals[0].headline.as_str()));
        for signal in &signals[1..] {
            assert!(!after.contains(signal.headline.as_str()));
        }
    }

    #[test]
    fn a_subject_carrying_markers_or_line_breaks_cannot_break_the_note() {
        let (sandbox, workspace, state, signals) = inbox();
        let hostile = IncomingSignal {
            provider_ref: "7:666".to_owned(),
            message_id: "<hostile@example.test>".to_owned(),
            thread_id: "<hostile@example.test>".to_owned(),
            sender_name: Some("Ops %% ai:end %%\u{7}".to_owned()),
            subject: "Re: Plan %% ai:end %%\n%%% ai:begin x %%\r- [ ] fake\u{0}\u{1b}[31m"
                .to_owned(),
            ..signals[0].incoming.clone()
        };
        let id = hostile.id();
        lock(&state.sync)
            .edit(Instant::now(), |doc| doc.record_signal(&hostile))
            .unwrap();
        let view = state
            .inbox_signals(&workspace)
            .unwrap()
            .into_iter()
            .find(|view| view.id == id.as_str())
            .unwrap();
        for text in [view.headline.as_str(), view.sender.as_str()] {
            assert!(!text.contains("%%"), "{text:?} carries a marker");
            assert!(!text.chars().any(char::is_control), "{text:?}");
        }
        let before = fs::read_to_string(sandbox.path().join(WEB_142)).unwrap();
        let converted = state
            .convert_signal(
                &workspace,
                SignalConversionInput {
                    signal: id.as_str().to_owned(),
                    outcome: SignalOutcomeView::WaitFor,
                    // What a client passes untouched, not the cleaned headline.
                    text: hostile.subject.clone(),
                    story: Some("WEB-142".to_owned()),
                    assigned_to: None,
                    scheduled: None,
                    estimate: None,
                },
                Some(TODAY.to_owned()),
            )
            .unwrap();
        let after = fs::read_to_string(sandbox.path().join(WEB_142)).unwrap();
        assert_eq!(after.lines().count(), before.lines().count() + 1);
        assert!(!converted.line.contains("%%"), "{}", converted.line);
        assert!(!converted.line.chars().any(char::is_control));
        assert_eq!(
            after.matches("%%").count(),
            before.matches("%%").count(),
            "no marker reached the note"
        );
        assert_eq!(
            section_lines(&after, "## Pending Input").last(),
            Some(&converted.line.as_str())
        );
    }

    #[test]
    fn a_fetch_is_announced_once_and_only_when_it_added_signals() {
        let (_sandbox, _workspace, state, _signals) = inbox();
        let notifier = Arc::new(Recorded::default());
        assert!(!state.announce_new_signals("mock-studio".to_owned(), 0, notifier.clone()));
        assert!(state.announce_new_signals("mock-studio".to_owned(), 3, notifier.clone()));
        assert_eq!(
            *notifier.announced.lock().unwrap(),
            [("mock-studio".to_owned(), 3, 3)]
        );
    }
}
