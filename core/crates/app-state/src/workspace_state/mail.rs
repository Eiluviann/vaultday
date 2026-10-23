//! The signal store and the mail cursors inside the workspace document.
//!
//! Each signal is one scalar holding its fields as JSON, keyed by its derived
//! id, so two devices recording the same message write the same key and
//! never create competing objects. The state sits under the same id in its
//! own map, a scalar too; concurrent states resolve to the most final one
//! ([`SignalState::finality`]). The raw text sits in a third map with the
//! compacted-record shape reserved. Cursors are per device, per account and
//! per folder: every device fetches on its own.

use automerge::transaction::Transactable;
use jiff::Timestamp;
use serde_json::{Map, Value, json};

use super::WorkspaceState;
use crate::app_doc::ConnectionId;
use crate::doc::{composite_key, get_all_str, get_str, keys};
use crate::error::AppStateError;
use crate::signals::{
    IncomingSignal, Signal, SignalAction, SignalId, SignalInsert, SignalRecord, SignalState,
};

/// The longest raw text kept per signal, in bytes; longer text is cut at a
/// character boundary.
pub const MAX_RAW_TEXT_BYTES: usize = 16 * 1024;

impl WorkspaceState {
    /// Records a fetched message as a signal unless the store already holds
    /// it; a duplicate keeps its stored state, so a refetch never revives a
    /// dismissed signal.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn record_signal(
        &mut self,
        incoming: &IncomingSignal,
    ) -> Result<SignalInsert, AppStateError> {
        let signal = incoming.to_signal();
        if get_str(&self.doc, &self.objects.signals, signal.id.as_str()).is_some() {
            return Ok(SignalInsert::Duplicate(signal.id));
        }
        self.doc.put(
            &self.objects.signals,
            signal.id.as_str(),
            encode_signal(&signal).to_string(),
        )?;
        let raw = json!({
            "text": incoming.stored_text(),
            "full_length": incoming.full_length(),
            "source_hash": signal.source_hash,
            "compacted": Value::Null,
        });
        self.doc.put(
            &self.objects.signal_texts,
            signal.id.as_str(),
            raw.to_string(),
        )?;
        Ok(SignalInsert::Added(signal.id))
    }

    /// One signal with its state.
    #[must_use]
    pub fn signal(&self, id: &SignalId) -> Option<SignalRecord> {
        let stored = get_str(&self.doc, &self.objects.signals, id.as_str())?;
        let signal = decode_signal(id, &stored)?;
        Some(SignalRecord {
            signal,
            state: self.signal_state(id),
        })
    }

    /// Every signal, newest first.
    #[must_use]
    pub fn signals(&self) -> Vec<SignalRecord> {
        let mut records: Vec<SignalRecord> = keys(&self.doc, &self.objects.signals)
            .into_iter()
            .filter_map(|id| self.signal(&SignalId::new(id)))
            .collect();
        records.sort_by(|a, b| {
            (b.signal.received_at, &a.signal.id).cmp(&(a.signal.received_at, &b.signal.id))
        });
        records
    }

    /// The stored state of a signal; [`SignalState::New`] until one is set.
    #[must_use]
    pub fn signal_state(&self, id: &SignalId) -> SignalState {
        get_all_str(&self.doc, &self.objects.signal_states, id.as_str())
            .iter()
            .filter_map(|text| SignalState::decode(text))
            .max_by(|a, b| {
                a.finality().cmp(&b.finality()).then_with(|| match (a, b) {
                    (SignalState::Snoozed { until: x }, SignalState::Snoozed { until: y }) => {
                        x.cmp(y)
                    }
                    _ => std::cmp::Ordering::Equal,
                })
            })
            .unwrap_or(SignalState::New)
    }

    /// Moves a signal through the state machine at `now`.
    ///
    /// # Errors
    /// [`AppStateError::UnknownSignal`] when the store has no such signal,
    /// [`AppStateError::SignalTransition`] when the state machine refuses the
    /// move, or the document refused the edit.
    pub fn apply_signal_action(
        &mut self,
        id: &SignalId,
        action: &SignalAction,
        now: Timestamp,
    ) -> Result<SignalState, AppStateError> {
        if get_str(&self.doc, &self.objects.signals, id.as_str()).is_none() {
            return Err(AppStateError::UnknownSignal(id.as_str().to_owned()));
        }
        let next = self
            .signal_state(id)
            .effective(now)
            .apply(action, now)
            .map_err(|refused| AppStateError::SignalTransition {
                signal: id.as_str().to_owned(),
                from: refused.from,
                action: refused.action,
            })?;
        self.doc
            .put(&self.objects.signal_states, id.as_str(), next.encode())?;
        Ok(next)
    }

    /// The raw text kept for a signal, as fetched.
    #[must_use]
    pub fn signal_raw_text(&self, id: &SignalId) -> Option<String> {
        let stored = get_str(&self.doc, &self.objects.signal_texts, id.as_str())?;
        let value: Value = serde_json::from_str(&stored).ok()?;
        value.get("text")?.as_str().map(str::to_owned)
    }

    /// How long a signal's text was before it was cut, in bytes; `None` for
    /// an unknown signal or a record written before the length was kept.
    #[must_use]
    pub fn signal_text_full_length(&self, id: &SignalId) -> Option<usize> {
        let stored = get_str(&self.doc, &self.objects.signal_texts, id.as_str())?;
        let value: Value = serde_json::from_str(&stored).ok()?;
        usize::try_from(value.get("full_length")?.as_u64()?).ok()
    }

    /// This device's cursor for one folder of an account.
    #[must_use]
    pub fn mail_cursor(&self, account: &ConnectionId, folder: &str) -> Option<String> {
        get_str(
            &self.doc,
            &self.objects.mail_cursors,
            &self.cursor_key(account, folder),
        )
    }

    /// Stores this device's cursor for one folder of an account.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_mail_cursor(
        &mut self,
        account: &ConnectionId,
        folder: &str,
        cursor: &str,
    ) -> Result<(), AppStateError> {
        let key = self.cursor_key(account, folder);
        self.doc.put(&self.objects.mail_cursors, key, cursor)?;
        Ok(())
    }

    fn cursor_key(&self, account: &ConnectionId, folder: &str) -> String {
        composite_key(&[self.device.as_str(), account.as_str(), folder])
    }
}

fn encode_signal(signal: &Signal) -> Value {
    json!({
        "account": signal.account.as_str(),
        "folder": signal.folder,
        "provider_ref": signal.provider_ref,
        "message_id": signal.message_id,
        "thread_id": signal.thread_id,
        "sender_name": signal.sender_name,
        "sender_address": signal.sender_address,
        "received_at": signal.received_at.to_string(),
        "subject": signal.subject,
        "excerpt": signal.excerpt,
        "link": signal.link,
        "source_hash": signal.source_hash,
    })
}

fn decode_signal(id: &SignalId, stored: &str) -> Option<Signal> {
    let value: Value = serde_json::from_str(stored).ok()?;
    let fields: &Map<String, Value> = value.as_object()?;
    let text = |key: &str| fields.get(key)?.as_str().map(str::to_owned);
    Some(Signal {
        id: id.clone(),
        account: ConnectionId::new(text("account")?),
        folder: text("folder")?,
        provider_ref: text("provider_ref")?,
        message_id: text("message_id")?,
        thread_id: text("thread_id")?,
        sender_name: text("sender_name"),
        sender_address: text("sender_address")?,
        received_at: text("received_at")?.parse().ok()?,
        subject: text("subject")?,
        excerpt: text("excerpt")?,
        link: text("link")?,
        source_hash: text("source_hash")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::DeviceId;
    use domain::task::TaskId;

    fn state(device: &str) -> WorkspaceState {
        WorkspaceState::new(DeviceId::new(device).unwrap()).unwrap()
    }

    fn at(text: &str) -> Timestamp {
        text.parse().unwrap()
    }

    fn incoming(message: &str) -> IncomingSignal {
        IncomingSignal {
            account: ConnectionId::new("con-mail-1"),
            folder: "INBOX".to_owned(),
            provider_ref: "7:1".to_owned(),
            message_id: format!("<{message}@example.test>"),
            thread_id: format!("<{message}@example.test>"),
            sender_name: Some("Synthetic Sender".to_owned()),
            sender_address: "sender@example.test".to_owned(),
            received_at: at("2026-10-23T08:00:00Z"),
            subject: "Synthetic subject".to_owned(),
            excerpt: "A synthetic excerpt".to_owned(),
            link: "message:%3Cm1@example.test%3E".to_owned(),
            raw_text: "A synthetic excerpt and the rest of the synthetic body".to_owned(),
            full_text_bytes: 0,
        }
    }

    #[test]
    fn a_refetch_is_a_duplicate_and_keeps_the_state() {
        let mut state = state("mac-a");
        let message = incoming("m1");
        let SignalInsert::Added(id) = state.record_signal(&message).unwrap() else {
            panic!("the first record adds");
        };
        state
            .apply_signal_action(&id, &SignalAction::Dismiss, at("2026-10-23T09:00:00Z"))
            .unwrap();
        assert_eq!(
            state.record_signal(&message).unwrap(),
            SignalInsert::Duplicate(id.clone())
        );
        assert_eq!(state.signal_state(&id), SignalState::Dismissed);
        let record = state.signal(&id).unwrap();
        assert_eq!(record.signal, message.to_signal());
        assert_eq!(
            state.signal_raw_text(&id).as_deref(),
            Some(message.raw_text.as_str())
        );
        assert_eq!(state.signals().len(), 1);
    }

    #[test]
    fn unknown_signals_and_refused_moves_are_errors() {
        let mut state = state("mac-a");
        let now = at("2026-10-23T09:00:00Z");
        assert!(matches!(
            state.apply_signal_action(&SignalId::new("sig-0"), &SignalAction::Dismiss, now),
            Err(AppStateError::UnknownSignal(_))
        ));
        let id = incoming("m1").id();
        state.record_signal(&incoming("m1")).unwrap();
        state
            .apply_signal_action(&id, &SignalAction::Convert { task: None }, now)
            .unwrap();
        assert!(matches!(
            state.apply_signal_action(&id, &SignalAction::Dismiss, now),
            Err(AppStateError::SignalTransition { .. })
        ));
    }

    #[test]
    fn concurrent_states_resolve_to_the_most_final() {
        let mut mac = state("mac-a");
        let mut other = state("mac-b");
        let message = incoming("m1");
        let id = message.id();
        mac.record_signal(&message).unwrap();
        other.record_signal(&message).unwrap();
        let now = at("2026-10-23T09:00:00Z");
        mac.apply_signal_action(&id, &SignalAction::Dismiss, now)
            .unwrap();
        other
            .apply_signal_action(
                &id,
                &SignalAction::Snooze {
                    until: at("2026-10-24T09:00:00Z"),
                },
                now,
            )
            .unwrap();
        mac.doc.merge(&mut other.doc).unwrap();
        other.doc.merge(&mut mac.doc).unwrap();
        assert_eq!(mac.signals().len(), 1, "both devices recorded one signal");
        assert_eq!(mac.signal_state(&id), SignalState::Dismissed);
        assert_eq!(other.signal_state(&id), SignalState::Dismissed);
        other
            .apply_signal_action(
                &id,
                &SignalAction::Convert {
                    task: Some(TaskId::new("task-1")),
                },
                now,
            )
            .unwrap_err();
    }

    #[test]
    fn cursors_are_kept_per_device() {
        let mut mac = state("mac-a");
        let mut other = state("mac-b");
        let account = ConnectionId::new("con-mail-1");
        mac.set_mail_cursor(&account, "INBOX", "7:42").unwrap();
        other.doc.merge(&mut mac.doc).unwrap();
        assert_eq!(mac.mail_cursor(&account, "INBOX").as_deref(), Some("7:42"));
        assert_eq!(other.mail_cursor(&account, "INBOX"), None);
    }

    #[test]
    fn long_raw_text_is_cut_at_a_character_boundary() {
        let text = "é".repeat(MAX_RAW_TEXT_BYTES);
        let cut = crate::signals::cut_at_char_boundary(&text, MAX_RAW_TEXT_BYTES);
        assert!(cut.len() <= MAX_RAW_TEXT_BYTES);
        assert!(cut.chars().all(|c| c == 'é'));
    }

    #[test]
    fn the_hash_covers_the_stored_text_and_the_full_length_is_kept() {
        let mut state = state("mac-a");
        let mut message = incoming("long");
        message.raw_text = "a".repeat(MAX_RAW_TEXT_BYTES + 100);
        message.full_text_bytes = MAX_RAW_TEXT_BYTES + 5000;
        let SignalInsert::Added(id) = state.record_signal(&message).unwrap() else {
            panic!("the first record adds");
        };
        let stored = state.signal_raw_text(&id).unwrap();
        assert_eq!(stored.len(), MAX_RAW_TEXT_BYTES);
        let record = state.signal(&id).unwrap();
        assert_eq!(
            record.signal.source_hash,
            crate::signals::source_hash(&stored)
        );
        assert_eq!(
            state.signal_text_full_length(&id),
            Some(MAX_RAW_TEXT_BYTES + 5000)
        );
    }

    #[test]
    fn debug_output_carries_no_mail_text() {
        let message = incoming("m1");
        let printed = format!("{message:?}");
        assert!(!printed.contains("synthetic body"));
        assert!(!printed.contains("Synthetic subject"));
        assert!(printed.contains("redacted"));
    }
}
