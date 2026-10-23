//! The per-workspace document: day order, time boxes, sessions, rollover
//! history, ritual answers, notification state and the id counters.
//!
//! Read it through [`crate::sync_folder::WorkspaceSync::state`] and change it
//! inside [`crate::sync_folder::WorkspaceSync::edit`], which commits the
//! edits as one change and batches it into the next change file. Records
//! whose stored fields do not parse (written by a newer build) are left out
//! of reads rather than failing them.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use automerge::transaction::Transactable;
use automerge::{AutoCommit, ChangeHash, ObjId, ObjType, ReadDoc, Value};
use domain::day_plan::{Lock, RolloverReason, TimeBox};
use domain::estimate::{Estimate, ThresholdAlert};
use domain::session::{Pause, SessionId, SessionRecord, SessionSource, SessionTarget};
use domain::task::{StoryNumber, TaskId, TimeBoxId};
use jiff::Timestamp;
use jiff::civil::{Date, DateTime};

use crate::device::DeviceId;
use crate::doc::{
    composite_key, delete_if_present, extend_schema, get_all_str, get_bool, get_map, get_str,
    get_u64, keys, put_optional_str, root_object, schema_document,
};
use crate::error::AppStateError;
use crate::ids::{self, IdBlock, IdKind, IdMaps};

mod features;
mod mail;

pub use mail::MAX_RAW_TEXT_BYTES;

/// Version of this document's schema, carried in every file's header.
pub const WORKSPACE_SCHEMA_VERSION: u16 = 1;
const SCHEMA_ACTOR: &str = "vaultday:schema:workspace:1";
/// The second schema change: the mail signal store (PR 12). A separate
/// change keeps the first one's hash, so documents started before it merge.
const MAIL_SCHEMA_ACTOR: &str = "vaultday:schema:workspace:mail:1";
/// The third schema change: records feature sets keep per workspace (the
/// Toggl queue, PR 16 of Phase 2), in a change of their own like the mail one.
const FEATURES_SCHEMA_ACTOR: &str = "vaultday:schema:workspace:features:1";

const DAY_ORDER: &str = "day_order";
const TIME_BOXES: &str = "time_boxes";
const SESSIONS: &str = "sessions";
const ROLLOVERS: &str = "rollovers";
const ROLLOVER_STREAKS: &str = "rollover_streaks";
const RITUAL_ANSWERS: &str = "ritual_answers";
const NOTIFICATIONS: &str = "notifications";
const THRESHOLD_ALERTS: &str = "threshold_alerts";
const ID_BLOCKS: &str = "id_blocks";
const ID_COUNTERS: &str = "id_counters";
const SIGNALS: &str = "signals";
const SIGNAL_STATES: &str = "signal_states";
const SIGNAL_TEXTS: &str = "signal_texts";
const MAIL_CURSORS: &str = "mail_cursors";
const MAIL_MAPS: [&str; 4] = [SIGNALS, SIGNAL_STATES, SIGNAL_TEXTS, MAIL_CURSORS];
const FEATURE_RECORDS: &str = "feature_records";
const MAPS: [&str; 9] = [
    DAY_ORDER,
    TIME_BOXES,
    SESSIONS,
    ROLLOVER_STREAKS,
    RITUAL_ANSWERS,
    NOTIFICATIONS,
    THRESHOLD_ALERTS,
    ID_BLOCKS,
    ID_COUNTERS,
];
/// Separates task ids in a stored day order; ids never contain it.
const ORDER_SEPARATOR: char = ' ';

/// A time box and the task it is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedTimeBox {
    /// The task the box holds.
    pub task: TaskId,
    /// The box; its duration is stored in whole seconds.
    pub time_box: TimeBox,
}

/// One shutdown rollover, kept as history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RolloverEntry {
    /// The task that moved.
    pub task: TaskId,
    /// The day it left.
    pub from: Date,
    /// The day it moved to.
    pub to: Date,
    /// Why, as the user gave it at shutdown.
    pub reason: RolloverReason,
    /// The task's streak of rollovers after this one.
    pub rollover_count: u32,
}

/// The per-workspace document of one device.
#[derive(Debug)]
pub struct WorkspaceState {
    doc: AutoCommit,
    device: DeviceId,
    objects: Objects,
    /// This run's actor bytes, the seed of its id block claims.
    run_seed: Vec<u8>,
    /// Id blocks this run claimed itself.
    run_blocks: BTreeSet<(IdKind, u64)>,
    /// Whether every file of this device's folder has been read.
    has_whole_own_history: bool,
}

#[derive(Debug, Clone)]
struct Objects {
    day_order: ObjId,
    time_boxes: ObjId,
    sessions: ObjId,
    rollovers: ObjId,
    rollover_streaks: ObjId,
    ritual_answers: ObjId,
    notifications: ObjId,
    threshold_alerts: ObjId,
    id_blocks: ObjId,
    id_counters: ObjId,
    signals: ObjId,
    signal_states: ObjId,
    signal_texts: ObjId,
    mail_cursors: ObjId,
    feature_records: ObjId,
}

impl WorkspaceState {
    /// A document holding only the schema change, writing as `device`.
    pub(crate) fn new(device: DeviceId) -> Result<WorkspaceState, AppStateError> {
        let doc = schema_document(SCHEMA_ACTOR, &MAPS, &[ROLLOVERS])?;
        let doc = extend_schema(doc, MAIL_SCHEMA_ACTOR, &MAIL_MAPS)?;
        let doc = extend_schema(doc, FEATURES_SCHEMA_ACTOR, &[FEATURE_RECORDS])?
            .with_actor(device.run_actor()?);
        let objects = Objects {
            day_order: root_object(&doc, DAY_ORDER)?,
            time_boxes: root_object(&doc, TIME_BOXES)?,
            sessions: root_object(&doc, SESSIONS)?,
            rollovers: root_object(&doc, ROLLOVERS)?,
            rollover_streaks: root_object(&doc, ROLLOVER_STREAKS)?,
            ritual_answers: root_object(&doc, RITUAL_ANSWERS)?,
            notifications: root_object(&doc, NOTIFICATIONS)?,
            threshold_alerts: root_object(&doc, THRESHOLD_ALERTS)?,
            id_blocks: root_object(&doc, ID_BLOCKS)?,
            id_counters: root_object(&doc, ID_COUNTERS)?,
            signals: root_object(&doc, SIGNALS)?,
            signal_states: root_object(&doc, SIGNAL_STATES)?,
            signal_texts: root_object(&doc, SIGNAL_TEXTS)?,
            mail_cursors: root_object(&doc, MAIL_CURSORS)?,
            feature_records: root_object(&doc, FEATURE_RECORDS)?,
        };
        let run_seed = doc.get_actor().to_bytes().to_vec();
        Ok(WorkspaceState {
            doc,
            device,
            run_seed,
            objects,
            run_blocks: BTreeSet::new(),
            has_whole_own_history: true,
        })
    }

    /// Tells id minting whether this run has read every file of its own
    /// device folder; until it has, it mints only from blocks it claims itself.
    pub(crate) fn set_has_whole_own_history(&mut self, has_whole_own_history: bool) {
        self.has_whole_own_history = has_whole_own_history;
    }

    pub(crate) fn doc_mut(&mut self) -> &mut AutoCommit {
        &mut self.doc
    }

    /// The device this document writes as.
    #[must_use]
    pub fn device_id(&self) -> &DeviceId {
        &self.device
    }

    /// The document heads: equal on two devices exactly when they converged.
    pub fn heads(&mut self) -> Vec<ChangeHash> {
        self.doc.get_heads()
    }

    // Day order.

    /// The tasks planned on `date`, in order. When two devices reordered the
    /// day apart, the winning order comes first and the tasks only the other
    /// order had follow it, so a task planned on either device stays planned.
    #[must_use]
    pub fn day_order(&self, date: Date) -> Vec<TaskId> {
        let mut order: Vec<TaskId> = Vec::new();
        for stored in get_all_str(&self.doc, &self.objects.day_order, &date.to_string()) {
            for id in stored.split(ORDER_SEPARATOR).filter(|id| !id.is_empty()) {
                let id = TaskId::new(id);
                if !order.contains(&id) {
                    order.push(id);
                }
            }
        }
        order
    }

    /// Replaces the order of `date`; an empty order clears the day.
    ///
    /// # Panics
    /// When a task id is empty or contains a space: the vault's ids never do.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_day_order(&mut self, date: Date, order: &[TaskId]) -> Result<(), AppStateError> {
        assert!(
            order
                .iter()
                .all(|id| !id.as_str().is_empty() && !id.as_str().contains(ORDER_SEPARATOR)),
            "task ids are non-empty and contain no space"
        );
        let stored: Vec<&str> = order.iter().map(TaskId::as_str).collect();
        let value = stored.join(&ORDER_SEPARATOR.to_string());
        put_optional_str(
            &mut self.doc,
            &self.objects.day_order,
            &date.to_string(),
            (!value.is_empty()).then_some(value.as_str()),
        )
    }

    /// Every day that has an order, earliest first.
    #[must_use]
    pub fn planned_days(&self) -> Vec<Date> {
        let mut days: Vec<Date> = keys(&self.doc, &self.objects.day_order)
            .iter()
            .filter_map(|key| key.parse().ok())
            .collect();
        days.sort();
        days
    }

    // Time boxes.

    /// Stores a time box for `task`, replacing the box with the same id.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_time_box(&mut self, task: &TaskId, time_box: &TimeBox) -> Result<(), AppStateError> {
        let record = self.record_map(&self.objects.time_boxes.clone(), time_box.id.as_str())?;
        let lock = match time_box.lock {
            Lock::Locked => "locked",
            Lock::Unlocked => "unlocked",
        };
        self.doc.put(&record, "task", task.as_str())?;
        self.doc.put(&record, "start", time_box.start.to_string())?;
        self.doc
            .put(&record, "duration_secs", time_box.duration.as_secs())?;
        self.doc.put(&record, "lock", lock)?;
        Ok(())
    }

    /// Removes a time box; removing one that does not exist is a no-op.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn remove_time_box(&mut self, id: &TimeBoxId) -> Result<(), AppStateError> {
        delete_if_present(&mut self.doc, &self.objects.time_boxes, id.as_str())
    }

    /// One time box.
    #[must_use]
    pub fn time_box(&self, id: &TimeBoxId) -> Option<PlannedTimeBox> {
        let record = get_map(&self.doc, &self.objects.time_boxes, id.as_str())?;
        let lock = match get_str(&self.doc, &record, "lock")?.as_str() {
            "locked" => Lock::Locked,
            "unlocked" => Lock::Unlocked,
            _ => return None,
        };
        Some(PlannedTimeBox {
            task: TaskId::new(get_str(&self.doc, &record, "task")?),
            time_box: TimeBox {
                id: id.clone(),
                start: get_str(&self.doc, &record, "start")?
                    .parse::<DateTime>()
                    .ok()?,
                duration: Duration::from_secs(get_u64(&self.doc, &record, "duration_secs")?),
                lock,
            },
        })
    }

    /// The time boxes starting on `date`, by start.
    #[must_use]
    pub fn time_boxes_on(&self, date: Date) -> Vec<PlannedTimeBox> {
        let mut boxes: Vec<PlannedTimeBox> = keys(&self.doc, &self.objects.time_boxes)
            .into_iter()
            .filter_map(|id| self.time_box(&TimeBoxId::new(id)))
            .filter(|planned| planned.time_box.start.date() == date)
            .collect();
        boxes.sort_by_key(|planned| (planned.time_box.start, planned.time_box.id.clone()));
        boxes
    }

    /// The time boxes `task` holds on any day, by start.
    #[must_use]
    pub fn time_boxes_of(&self, task: &TaskId) -> Vec<PlannedTimeBox> {
        let mut boxes: Vec<PlannedTimeBox> = keys(&self.doc, &self.objects.time_boxes)
            .into_iter()
            .filter_map(|id| self.time_box(&TimeBoxId::new(id)))
            .filter(|planned| planned.task == *task)
            .collect();
        boxes.sort_by_key(|planned| (planned.time_box.start, planned.time_box.id.clone()));
        boxes
    }

    // Sessions.

    /// Stores a session, replacing the stored fields of the same id.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn put_session(&mut self, session: &SessionRecord) -> Result<(), AppStateError> {
        let record = self.record_map(&self.objects.sessions.clone(), session.id.as_str())?;
        let (source, title) = match &session.source {
            SessionSource::Timer => ("timer", None),
            SessionSource::Meeting { title } => ("meeting", Some(title.as_str())),
        };
        let doc = &mut self.doc;
        put_optional_str(
            doc,
            &record,
            "task",
            session.target.task.as_ref().map(TaskId::as_str),
        )?;
        put_optional_str(
            doc,
            &record,
            "story",
            session.target.story.as_ref().map(StoryNumber::as_str),
        )?;
        doc.put(&record, "source", source)?;
        put_optional_str(doc, &record, "meeting_title", title)?;
        doc.put(&record, "started_at", session.started_at.to_string())?;
        let stopped_at = session.stopped_at.map(|at| at.to_string());
        put_optional_str(doc, &record, "stopped_at", stopped_at.as_deref())?;
        doc.put(&record, "pauses", encode_pauses(&session.pauses))?;
        doc.put(&record, "long_timer_fired", session.has_long_timer_fired)?;
        Ok(())
    }

    /// One session.
    #[must_use]
    pub fn session(&self, id: &SessionId) -> Option<SessionRecord> {
        let record = get_map(&self.doc, &self.objects.sessions, id.as_str())?;
        let field = |key: &str| get_str(&self.doc, &record, key);
        let source = match field("source")?.as_str() {
            "timer" => SessionSource::Timer,
            "meeting" => SessionSource::Meeting {
                title: field("meeting_title").unwrap_or_default(),
            },
            _ => return None,
        };
        let stopped_at = match field("stopped_at") {
            Some(text) => Some(text.parse::<Timestamp>().ok()?),
            None => None,
        };
        Some(SessionRecord {
            id: id.clone(),
            target: SessionTarget {
                task: field("task").map(TaskId::new),
                story: field("story").map(StoryNumber::new),
            },
            source,
            started_at: field("started_at")?.parse().ok()?,
            stopped_at,
            pauses: decode_pauses(&field("pauses").unwrap_or_default())?,
            has_long_timer_fired: get_bool(&self.doc, &record, "long_timer_fired").unwrap_or(false),
        })
    }

    /// Every session, by start.
    #[must_use]
    pub fn sessions(&self) -> Vec<SessionRecord> {
        let mut sessions: Vec<SessionRecord> = keys(&self.doc, &self.objects.sessions)
            .into_iter()
            .filter_map(|id| self.session(&SessionId::new(id)))
            .collect();
        sessions.sort_by_key(|session| (session.started_at, session.id.clone()));
        sessions
    }

    // Rollover history.

    /// Appends one rollover to the history.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn record_rollover(&mut self, entry: &RolloverEntry) -> Result<(), AppStateError> {
        let index = self.doc.length(&self.objects.rollovers);
        let record = self
            .doc
            .insert_object(&self.objects.rollovers, index, ObjType::Map)?;
        self.doc.put(&record, "task", entry.task.as_str())?;
        self.doc.put(&record, "from", entry.from.to_string())?;
        self.doc.put(&record, "to", entry.to.to_string())?;
        self.doc.put(&record, "reason", entry.reason.as_str())?;
        self.doc
            .put(&record, "count", u64::from(entry.rollover_count))?;
        Ok(())
    }

    /// The rollover history, by day left and task.
    #[must_use]
    pub fn rollovers(&self) -> Vec<RolloverEntry> {
        let mut entries: Vec<RolloverEntry> = self
            .doc
            .list_range(&self.objects.rollovers, ..)
            .filter_map(|item| match item.value {
                automerge::ValueRef::Object(ObjType::Map) => Some(item.id()),
                _ => None,
            })
            .filter_map(|record| {
                let field = |key: &str| get_str(&self.doc, &record, key);
                Some(RolloverEntry {
                    task: TaskId::new(field("task")?),
                    from: field("from")?.parse().ok()?,
                    to: field("to")?.parse().ok()?,
                    reason: RolloverReason::new(field("reason")?),
                    rollover_count: u32::try_from(get_u64(&self.doc, &record, "count")?).ok()?,
                })
            })
            .collect();
        entries.sort_by(|a, b| (a.from, &a.task, a.to).cmp(&(b.from, &b.task, b.to)));
        entries
    }

    /// The task's current streak of shutdown rollovers.
    #[must_use]
    pub fn rollover_streak(&self, task: &TaskId) -> u32 {
        get_u64(&self.doc, &self.objects.rollover_streaks, task.as_str())
            .and_then(|count| u32::try_from(count).ok())
            .unwrap_or(0)
    }

    /// Sets the task's streak; zero clears it.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_rollover_streak(&mut self, task: &TaskId, count: u32) -> Result<(), AppStateError> {
        let streaks = &self.objects.rollover_streaks;
        if count == 0 {
            return delete_if_present(&mut self.doc, streaks, task.as_str());
        }
        self.doc.put(streaks, task.as_str(), u64::from(count))?;
        Ok(())
    }

    // Ritual answers.

    /// Stores the answer to one question of a ritual on `date`.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_ritual_answer(
        &mut self,
        date: Date,
        ritual: &str,
        question: &str,
        answer: &str,
    ) -> Result<(), AppStateError> {
        let key = composite_key(&[&date.to_string(), ritual, question]);
        self.doc.put(&self.objects.ritual_answers, key, answer)?;
        Ok(())
    }

    /// The answers of one ritual on `date`, by question.
    #[must_use]
    pub fn ritual_answers(&self, date: Date, ritual: &str) -> BTreeMap<String, String> {
        let prefix = composite_key(&[&date.to_string(), ritual, ""]);
        keys(&self.doc, &self.objects.ritual_answers)
            .into_iter()
            .filter_map(|key| {
                let question = key.strip_prefix(&prefix)?.to_owned();
                let answer = get_str(&self.doc, &self.objects.ritual_answers, &key)?;
                Some((question, answer))
            })
            .collect()
    }

    // Notification state.

    /// Records that the notification `key` was shown at `at`, so no device shows it again.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn mark_notified(&mut self, key: &str, at: Timestamp) -> Result<(), AppStateError> {
        self.doc
            .put(&self.objects.notifications, key, at.to_string())?;
        Ok(())
    }

    /// When the notification `key` was shown, if it was.
    #[must_use]
    pub fn notified_at(&self, key: &str) -> Option<Timestamp> {
        get_str(&self.doc, &self.objects.notifications, key)?
            .parse()
            .ok()
    }

    /// Forgets that the notification `key` was shown.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn clear_notified(&mut self, key: &str) -> Result<(), AppStateError> {
        delete_if_present(&mut self.doc, &self.objects.notifications, key)
    }

    /// The 80 % estimate alert of a task, as last stored.
    #[must_use]
    pub fn threshold_alert(&self, task: &TaskId) -> ThresholdAlert {
        get_u64(&self.doc, &self.objects.threshold_alerts, task.as_str())
            .map_or_else(ThresholdAlert::new, |secs| {
                ThresholdAlert::fired_for(Estimate::new(Duration::from_secs(secs)))
            })
    }

    /// Stores the 80 % estimate alert of a task after it observed new numbers.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_threshold_alert(
        &mut self,
        task: &TaskId,
        alert: &ThresholdAlert,
    ) -> Result<(), AppStateError> {
        let alerts = &self.objects.threshold_alerts;
        match alert.fired_estimate() {
            Some(estimate) => {
                self.doc.put(alerts, task.as_str(), estimate.as_secs())?;
                Ok(())
            }
            None => delete_if_present(&mut self.doc, alerts, task.as_str()),
        }
    }

    // Ids.

    /// Mints a `task-` id from this device's reserved range, skipping ids
    /// `is_taken` reports as present in the vault.
    ///
    /// # Errors
    /// [`AppStateError::IdSpaceExhausted`], or the document refused the edit.
    pub fn mint_task_id(
        &mut self,
        is_taken: &dyn Fn(&str) -> bool,
    ) -> Result<TaskId, AppStateError> {
        self.mint(IdKind::Task, is_taken).map(TaskId::new)
    }

    /// Mints a `ses-` id from this device's reserved range.
    ///
    /// # Errors
    /// [`AppStateError::IdSpaceExhausted`], or the document refused the edit.
    pub fn mint_session_id(&mut self) -> Result<SessionId, AppStateError> {
        self.mint(IdKind::Session, &|_| false).map(SessionId::new)
    }

    /// Mints a `box-` id from this device's reserved range.
    ///
    /// # Errors
    /// [`AppStateError::IdSpaceExhausted`], or the document refused the edit.
    pub fn mint_time_box_id(&mut self) -> Result<TimeBoxId, AppStateError> {
        self.mint(IdKind::TimeBox, &|_| false).map(TimeBoxId::new)
    }

    /// The id blocks `device` holds, for the Settings window's device id block.
    #[must_use]
    pub fn id_blocks(&self, device: &DeviceId) -> Vec<IdBlock> {
        ids::blocks_of(&self.doc, &self.id_maps(), device)
    }

    pub(crate) fn claim_creator_blocks(&mut self) -> Result<(), AppStateError> {
        let (doc, maps, mut minter) = self.minting();
        ids::claim_creator_blocks(doc, &maps, &mut minter)
    }

    fn mint(
        &mut self,
        kind: IdKind,
        is_taken: &dyn Fn(&str) -> bool,
    ) -> Result<String, AppStateError> {
        let (doc, maps, mut minter) = self.minting();
        ids::mint(doc, &maps, &mut minter, kind, is_taken)
    }

    /// Splits the borrows id minting needs.
    fn minting(&mut self) -> (&mut AutoCommit, IdMaps<'_>, ids::Minter<'_>) {
        let maps = IdMaps {
            blocks: &self.objects.id_blocks,
            counters: &self.objects.id_counters,
        };
        let minter = ids::Minter {
            device: &self.device,
            run_seed: &self.run_seed,
            run_blocks: &mut self.run_blocks,
            has_whole_own_history: self.has_whole_own_history,
        };
        (&mut self.doc, maps, minter)
    }

    fn id_maps(&self) -> IdMaps<'_> {
        IdMaps {
            blocks: &self.objects.id_blocks,
            counters: &self.objects.id_counters,
        }
    }

    /// The record map under `id`, created when absent. Only the device that
    /// minted `id` creates it, so two devices never create it concurrently.
    fn record_map(&mut self, parent: &ObjId, id: &str) -> Result<ObjId, AppStateError> {
        match self.doc.get(parent, id)? {
            Some((Value::Object(ObjType::Map), record)) => Ok(record),
            _ => Ok(self.doc.put_object(parent, id, ObjType::Map)?),
        }
    }
}

/// Pauses as `start~end` pairs joined by `,`; an open pause has no end.
fn encode_pauses(pauses: &[Pause]) -> String {
    pauses
        .iter()
        .map(|pause| {
            let end = pause
                .ended_at()
                .map(|at| at.to_string())
                .unwrap_or_default();
            format!("{}~{end}", pause.started_at())
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn decode_pauses(text: &str) -> Option<Vec<Pause>> {
    text.split(',')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (start, end) = pair.split_once('~')?;
            let ended_at = match end {
                "" => None,
                end => Some(end.parse().ok()?),
            };
            Some(Pause::new(start.parse().ok()?, ended_at))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    fn state(device: &str) -> WorkspaceState {
        WorkspaceState::new(DeviceId::new(device).unwrap()).unwrap()
    }

    fn at(text: &str) -> Timestamp {
        text.parse().unwrap()
    }

    #[test]
    fn the_schema_change_is_identical_on_every_device() {
        assert_eq!(state("mac-a").heads(), state("linux-b").heads());
    }

    #[test]
    fn records_round_trip_through_the_document() {
        let mut state = state("mac-a");
        let day = date(2026, 10, 22);
        state
            .set_day_order(day, &[TaskId::new("task-2"), TaskId::new("task-1")])
            .unwrap();
        assert_eq!(
            state.day_order(day),
            [TaskId::new("task-2"), TaskId::new("task-1")]
        );
        assert_eq!(state.planned_days(), [day]);
        state.set_day_order(day, &[]).unwrap();
        assert!(state.planned_days().is_empty());

        let time_box = TimeBox {
            id: TimeBoxId::new("box-1"),
            start: day.at(9, 30, 0, 0),
            duration: Duration::from_secs(45 * 60),
            lock: Lock::Locked,
        };
        state
            .set_time_box(&TaskId::new("task-1"), &time_box)
            .unwrap();
        let planned = state.time_boxes_on(day);
        assert_eq!(planned.len(), 1);
        assert_eq!(planned[0].time_box, time_box);
        state.remove_time_box(&time_box.id).unwrap();
        assert!(state.time_box(&time_box.id).is_none());

        let session = SessionRecord {
            id: SessionId::new("ses-1"),
            target: SessionTarget::task_in_story("task-1", "WEB-142"),
            source: SessionSource::Meeting {
                title: "Synthetic sync".to_owned(),
            },
            started_at: at("2026-10-22T07:00:00Z"),
            stopped_at: Some(at("2026-10-22T08:00:00Z")),
            pauses: vec![
                Pause::new(at("2026-10-22T07:10:00Z"), Some(at("2026-10-22T07:15:00Z"))),
                Pause::new(at("2026-10-22T07:40:00Z"), None),
            ],
            has_long_timer_fired: true,
        };
        state.put_session(&session).unwrap();
        assert_eq!(state.sessions(), std::slice::from_ref(&session));
        let mut stopped_later = session.clone();
        stopped_later.target = SessionTarget::story("WEB-142");
        stopped_later.source = SessionSource::Timer;
        stopped_later.pauses.clear();
        state.put_session(&stopped_later).unwrap();
        assert_eq!(state.session(&session.id), Some(stopped_later));

        let entry = RolloverEntry {
            task: TaskId::new("task-1"),
            from: day,
            to: date(2026, 10, 23),
            reason: RolloverReason::new("waiting on review"),
            rollover_count: 2,
        };
        state.record_rollover(&entry).unwrap();
        assert_eq!(state.rollovers(), [entry]);
        state
            .set_rollover_streak(&TaskId::new("task-1"), 2)
            .unwrap();
        assert_eq!(state.rollover_streak(&TaskId::new("task-1")), 2);
        state
            .set_rollover_streak(&TaskId::new("task-1"), 0)
            .unwrap();
        assert_eq!(state.rollover_streak(&TaskId::new("task-1")), 0);

        state
            .set_ritual_answer(day, "shutdown", "wins", "Shipped the parser")
            .unwrap();
        state
            .set_ritual_answer(day, "planning", "focus", "Reviews")
            .unwrap();
        let answers = state.ritual_answers(day, "shutdown");
        assert_eq!(answers.len(), 1);
        assert_eq!(answers["wins"], "Shipped the parser");

        state
            .mark_notified("long-timer:ses-1", at("2026-10-22T09:00:00Z"))
            .unwrap();
        assert_eq!(
            state.notified_at("long-timer:ses-1"),
            Some(at("2026-10-22T09:00:00Z"))
        );
        state.clear_notified("long-timer:ses-1").unwrap();
        assert_eq!(state.notified_at("long-timer:ses-1"), None);

        let fired = ThresholdAlert::fired_for(Estimate::from_minutes(90));
        state
            .set_threshold_alert(&TaskId::new("task-1"), &fired)
            .unwrap();
        assert_eq!(state.threshold_alert(&TaskId::new("task-1")), fired);
        state
            .set_threshold_alert(&TaskId::new("task-1"), &ThresholdAlert::new())
            .unwrap();
        assert_eq!(
            state.threshold_alert(&TaskId::new("task-1")),
            ThresholdAlert::new()
        );
    }

    #[test]
    fn concurrent_day_orders_keep_every_planned_task() {
        let day = date(2026, 10, 22);
        let mut mac = state("mac-a");
        let mut linux = state("linux-b");
        mac.set_day_order(day, &[TaskId::new("task-1"), TaskId::new("task-2")])
            .unwrap();
        linux
            .set_day_order(day, &[TaskId::new("task-3"), TaskId::new("task-1")])
            .unwrap();
        mac.doc.merge(&mut linux.doc).unwrap();
        linux.doc.merge(&mut mac.doc).unwrap();
        let order = mac.day_order(day);
        assert_eq!(order, linux.day_order(day));
        let mut sorted = order.clone();
        sorted.sort();
        assert_eq!(
            sorted,
            [
                TaskId::new("task-1"),
                TaskId::new("task-2"),
                TaskId::new("task-3")
            ]
        );
    }

    #[test]
    fn minted_ids_skip_ids_the_vault_already_has() {
        let mut creator = state("mac-a");
        creator.claim_creator_blocks().unwrap();
        let taken = |id: &str| matches!(id, "task-1" | "task-2");
        assert_eq!(creator.mint_task_id(&taken).unwrap(), TaskId::new("task-3"));
        assert_eq!(creator.mint_session_id().unwrap(), SessionId::new("ses-1"));
        assert_eq!(creator.mint_time_box_id().unwrap(), TimeBoxId::new("box-1"));
        let device = creator.device_id().clone();
        assert_eq!(creator.id_blocks(&device).len(), 3);
    }

    #[test]
    fn a_full_block_moves_on_to_a_new_one() {
        let mut creator = state("mac-a");
        creator.claim_creator_blocks().unwrap();
        let minted: Vec<String> = (0..1_000)
            .map(|_| creator.mint_session_id().unwrap().as_str().to_owned())
            .collect();
        assert_eq!(minted[998], "ses-999");
        let next: u64 = minted[999].trim_start_matches("ses-").parse().unwrap();
        assert!(
            next >= ids::BLOCK_SIZE && next.is_multiple_of(ids::BLOCK_SIZE),
            "{next}"
        );
        let device = creator.device_id().clone();
        assert_eq!(
            creator
                .id_blocks(&device)
                .iter()
                .filter(|block| block.kind == IdKind::Session)
                .count(),
            2
        );
    }

    #[test]
    fn of_two_concurrent_claims_on_one_block_the_smaller_device_keeps_it() {
        let mut a = state("aaa");
        let mut b = state("bbb");
        a.claim_creator_blocks().unwrap();
        b.claim_creator_blocks().unwrap();
        a.doc.merge(&mut b.doc).unwrap();
        b.doc.merge(&mut a.doc).unwrap();
        assert_eq!(a.mint_task_id(&|_| false).unwrap(), TaskId::new("task-1"));
        let moved = b.mint_task_id(&|_| false).unwrap();
        assert_ne!(moved, TaskId::new("task-1"));
        assert!(
            b.id_blocks(&DeviceId::new("bbb").unwrap())
                .iter()
                .all(|block| block.index != 0 || block.kind != IdKind::Task)
        );
    }
}
