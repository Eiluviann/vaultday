//! Signals: what a mail fetch brings in for the Inbox column, and the state
//! each one is in. The records live in the encrypted workspace document (see
//! [`crate::WorkspaceState::record_signal`]); this module holds the types and
//! the state machine, which do no IO.
//!
//! A signal's id is derived from the message, not minted: `sig-` plus the
//! first 16 hex digits of a SHA-256 over the account, the `Message-ID` and the
//! conversation id. Two fetches of one message, on one device or two, land on
//! the same id, which is how duplicates are suppressed.

use std::fmt;

use domain::task::TaskId;
use jiff::Timestamp;
use sha2::{Digest, Sha256};

use crate::app_doc::ConnectionId;

/// Identifies a signal: `sig-` and 16 hex digits.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SignalId(String);

/// One message as the Inbox column shows it. Holds no body; the raw text is
/// kept beside it in the encrypted store ([`crate::WorkspaceState::signal_raw_text`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signal {
    /// The signal's id.
    pub id: SignalId,
    /// The mail account it came from.
    pub account: ConnectionId,
    /// The folder it was fetched from (`INBOX`, or the provider's folder id).
    pub folder: String,
    /// How the provider names the message: `<uidvalidity>:<uid>` on IMAP,
    /// the message id on Microsoft Graph. What a body fetch needs.
    pub provider_ref: String,
    /// The `Message-ID` header (Graph's `internetMessageId`).
    pub message_id: String,
    /// The conversation the message belongs to.
    pub thread_id: String,
    /// The sender's display name, when the message gives one.
    pub sender_name: Option<String>,
    /// The sender's address.
    pub sender_address: String,
    /// When the message arrived.
    pub received_at: Timestamp,
    /// The subject, decoded.
    pub subject: String,
    /// A short plain-text excerpt.
    pub excerpt: String,
    /// The link that opens the message in the account's mail app.
    pub link: String,
    /// SHA-256 of the raw source text, hex. Reserved for the compacted
    /// record: when a model compacts the raw text, the hash says which text
    /// the summary was made from. v1 has no model and compacts nothing.
    pub source_hash: String,
}

/// A fetched message about to become a signal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncomingSignal {
    /// The mail account it came from.
    pub account: ConnectionId,
    /// The folder it was fetched from.
    pub folder: String,
    /// How the provider names the message.
    pub provider_ref: String,
    /// The `Message-ID` header; when the message has none, the provider's reference.
    pub message_id: String,
    /// The conversation the message belongs to.
    pub thread_id: String,
    /// The sender's display name.
    pub sender_name: Option<String>,
    /// The sender's address.
    pub sender_address: String,
    /// When the message arrived.
    pub received_at: Timestamp,
    /// The subject, decoded.
    pub subject: String,
    /// A short plain-text excerpt.
    pub excerpt: String,
    /// The link back to the message.
    pub link: String,
    /// The message text as fetched, kept only in the encrypted store.
    pub raw_text: String,
}

/// Where a signal stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignalState {
    /// Not decided yet.
    New,
    /// Hidden until a time; it reads as [`SignalState::New`] once that passes.
    Snoozed {
        /// When it comes back.
        until: Timestamp,
    },
    /// Put away for good; a refetch of the message never brings it back.
    Dismissed,
    /// Turned into a task. PR 13 writes `[source:: sig-…]` on the task line.
    Converted {
        /// The task, once it is written.
        task: Option<TaskId>,
    },
}

/// What the user does with a signal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignalAction {
    /// Hide it until `until`.
    Snooze {
        /// When it comes back; must be later than now.
        until: Timestamp,
    },
    /// Put it away for good.
    Dismiss,
    /// Turn it into a task.
    Convert {
        /// The task, when already written.
        task: Option<TaskId>,
    },
}

/// A signal with its state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignalRecord {
    /// The signal.
    pub signal: Signal,
    /// Its stored state; see [`SignalState::effective`] for what to show.
    pub state: SignalState,
}

/// What recording a fetched message did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignalInsert {
    /// A new signal.
    Added(SignalId),
    /// The store already held it; its state was left alone.
    Duplicate(SignalId),
}

/// A transition the state machine refuses: a dismissed or converted signal
/// is final, and a snooze must end in the future.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignalTransitionRefused {
    /// The state the signal was in.
    pub from: &'static str,
    /// The action asked for.
    pub action: &'static str,
}

impl SignalId {
    /// The id of the message `message_id` in conversation `thread_id` of `account`.
    #[must_use]
    pub fn for_message(account: &ConnectionId, message_id: &str, thread_id: &str) -> SignalId {
        let mut hasher = Sha256::new();
        for part in [account.as_str(), message_id, thread_id] {
            hasher.update(part.as_bytes());
            hasher.update([0x1f]);
        }
        let digest = hasher.finalize();
        SignalId(format!("sig-{}", hex(&digest[..8])))
    }

    /// Wraps a stored id.
    pub fn new(id: impl Into<String>) -> SignalId {
        SignalId(id.into())
    }

    /// The id text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SignalId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl IncomingSignal {
    /// The id this message gets.
    #[must_use]
    pub fn id(&self) -> SignalId {
        SignalId::for_message(&self.account, &self.message_id, &self.thread_id)
    }

    /// The signal record, with the hash of its raw text.
    #[must_use]
    pub fn to_signal(&self) -> Signal {
        Signal {
            id: self.id(),
            account: self.account.clone(),
            folder: self.folder.clone(),
            provider_ref: self.provider_ref.clone(),
            message_id: self.message_id.clone(),
            thread_id: self.thread_id.clone(),
            sender_name: self.sender_name.clone(),
            sender_address: self.sender_address.clone(),
            received_at: self.received_at,
            subject: self.subject.clone(),
            excerpt: self.excerpt.clone(),
            link: self.link.clone(),
            source_hash: source_hash(&self.raw_text),
        }
    }
}

impl SignalState {
    /// The state to show at `now`: a snooze that ended reads as new.
    #[must_use]
    pub fn effective(&self, now: Timestamp) -> SignalState {
        match self {
            SignalState::Snoozed { until } if *until <= now => SignalState::New,
            other => other.clone(),
        }
    }

    /// The state after `action` at `now`.
    ///
    /// # Errors
    /// [`SignalTransitionRefused`] when the signal is dismissed or converted,
    /// or a snooze would end at or before `now`.
    pub fn apply(
        &self,
        action: &SignalAction,
        now: Timestamp,
    ) -> Result<SignalState, SignalTransitionRefused> {
        let refused = || SignalTransitionRefused {
            from: self.label(),
            action: action.label(),
        };
        match (self, action) {
            (SignalState::Dismissed | SignalState::Converted { .. }, _) => Err(refused()),
            (_, SignalAction::Snooze { until }) if *until <= now => Err(refused()),
            (_, SignalAction::Snooze { until }) => Ok(SignalState::Snoozed { until: *until }),
            (_, SignalAction::Dismiss) => Ok(SignalState::Dismissed),
            (_, SignalAction::Convert { task }) => {
                Ok(SignalState::Converted { task: task.clone() })
            }
        }
    }

    /// The state's name, for messages.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            SignalState::New => "new",
            SignalState::Snoozed { .. } => "snoozed",
            SignalState::Dismissed => "dismissed",
            SignalState::Converted { .. } => "converted",
        }
    }

    /// How final a state is; when two devices set a state concurrently the
    /// more final one wins, so a dismissal is never undone by a merge.
    pub(crate) const fn finality(&self) -> u8 {
        match self {
            SignalState::New => 0,
            SignalState::Snoozed { .. } => 1,
            SignalState::Dismissed => 2,
            SignalState::Converted { .. } => 3,
        }
    }

    pub(crate) fn encode(&self) -> String {
        match self {
            SignalState::New => "new".to_owned(),
            SignalState::Snoozed { until } => format!("snoozed {until}"),
            SignalState::Dismissed => "dismissed".to_owned(),
            SignalState::Converted { task: None } => "converted".to_owned(),
            SignalState::Converted { task: Some(task) } => format!("converted {task}"),
        }
    }

    pub(crate) fn decode(text: &str) -> Option<SignalState> {
        let (name, argument) = text.split_once(' ').unwrap_or((text, ""));
        match (name, argument) {
            ("new", "") => Some(SignalState::New),
            ("snoozed", until) => Some(SignalState::Snoozed {
                until: until.parse().ok()?,
            }),
            ("dismissed", "") => Some(SignalState::Dismissed),
            ("converted", "") => Some(SignalState::Converted { task: None }),
            ("converted", task) => Some(SignalState::Converted {
                task: Some(TaskId::new(task)),
            }),
            _ => None,
        }
    }
}

impl SignalAction {
    /// The action's name, for messages.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            SignalAction::Snooze { .. } => "snooze",
            SignalAction::Dismiss => "dismiss",
            SignalAction::Convert { .. } => "convert",
        }
    }
}

impl fmt::Display for SignalTransitionRefused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "a {} signal cannot {}", self.from, self.action)
    }
}

/// SHA-256 of `text`, hex.
#[must_use]
pub fn source_hash(text: &str) -> String {
    hex(&Sha256::digest(text.as_bytes()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> Timestamp {
        text.parse().unwrap()
    }

    #[test]
    fn ids_follow_the_message_and_not_the_fetch() {
        let account = ConnectionId::new("con-mail-1");
        let id = SignalId::for_message(&account, "<m1@example.test>", "t1");
        assert_eq!(
            id,
            SignalId::for_message(&account, "<m1@example.test>", "t1")
        );
        assert!(id.as_str().starts_with("sig-"));
        assert_eq!(id.as_str().len(), 4 + 16);
        assert_ne!(
            id,
            SignalId::for_message(&ConnectionId::new("con-mail-2"), "<m1@example.test>", "t1"),
            "the same message in another account is another signal"
        );
        assert_ne!(
            id,
            SignalId::for_message(&account, "<m1@example.test>", "t2")
        );
    }

    #[test]
    fn the_state_machine_allows_only_forward_moves() {
        let now = at("2026-10-23T09:00:00Z");
        let later = at("2026-10-24T09:00:00Z");
        let snoozed = SignalState::New
            .apply(&SignalAction::Snooze { until: later }, now)
            .unwrap();
        assert_eq!(snoozed, SignalState::Snoozed { until: later });
        assert_eq!(snoozed.effective(now), snoozed);
        assert_eq!(snoozed.effective(later), SignalState::New);
        assert!(
            SignalState::New
                .apply(&SignalAction::Snooze { until: now }, now)
                .is_err(),
            "a snooze must end in the future"
        );
        let dismissed = snoozed.apply(&SignalAction::Dismiss, now).unwrap();
        assert_eq!(dismissed, SignalState::Dismissed);
        for action in [
            SignalAction::Dismiss,
            SignalAction::Snooze { until: later },
            SignalAction::Convert { task: None },
        ] {
            assert_eq!(
                dismissed.apply(&action, now),
                Err(SignalTransitionRefused {
                    from: "dismissed",
                    action: action.label()
                })
            );
        }
        let converted = SignalState::New
            .apply(
                &SignalAction::Convert {
                    task: Some(TaskId::new("task-7")),
                },
                now,
            )
            .unwrap();
        assert!(converted.apply(&SignalAction::Dismiss, now).is_err());
    }

    #[test]
    fn states_round_trip_through_their_encoding() {
        for state in [
            SignalState::New,
            SignalState::Snoozed {
                until: at("2026-10-24T09:00:00Z"),
            },
            SignalState::Dismissed,
            SignalState::Converted { task: None },
            SignalState::Converted {
                task: Some(TaskId::new("task-7")),
            },
        ] {
            assert_eq!(SignalState::decode(&state.encode()), Some(state));
        }
        assert_eq!(SignalState::decode("archived"), None);
    }
}
