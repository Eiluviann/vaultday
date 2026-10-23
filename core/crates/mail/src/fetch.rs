//! One account's fetch: its selected folders from their cursors, with
//! bounded retries, and the recording of what came into the signal store.
//!
//! The two halves are apart on purpose: [`fetch_account`] does all the
//! network work and touches no document, so a caller holds the workspace
//! document only for [`ingest`], which does no IO.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use app_state::{
    AppStateError, ConnectionId, IncomingSignal, SignalId, SignalInsert, WorkspaceState,
};
use jiff::{SignedDuration, Timestamp};

use crate::error::MailError;
use crate::retry::RetryPolicy;
use crate::source::{FetchedMessage, MailSource};

/// How much one fetch brings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FetchLimits {
    /// The most messages one folder brings per fetch, oldest first; the
    /// cursor stops at the last one brought, so the rest come next time.
    pub max_messages: usize,
    /// The most text kept per message, in bytes.
    pub max_raw_bytes: usize,
    /// How far back the first fetch of a folder reaches.
    pub first_fetch_window: Duration,
    /// The longest one account's fetch may run, retries and their waits
    /// included. It is checked before each folder and each attempt; a call
    /// already on the wire is bounded by the transport's own timeouts.
    pub overall_deadline: Duration,
}

/// What one folder brought.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderFetch {
    /// The folder id.
    pub folder: String,
    /// The new messages.
    pub messages: Vec<FetchedMessage>,
    /// The folder's next cursor.
    pub cursor: String,
}

/// What one account brought.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountFetch {
    /// The account.
    pub account: ConnectionId,
    /// Each selected folder that fetched.
    pub folders: Vec<FolderFetch>,
}

/// What recording a fetch did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IngestSummary {
    /// Signals new to the store.
    pub added: Vec<SignalId>,
    /// Messages the store already held; their state was left alone.
    pub duplicates: usize,
}

impl Default for FetchLimits {
    fn default() -> Self {
        FetchLimits {
            max_messages: 50,
            max_raw_bytes: app_state::MAX_RAW_TEXT_BYTES,
            first_fetch_window: Duration::from_secs(14 * 24 * 60 * 60),
            overall_deadline: Duration::from_secs(120),
        }
    }
}

/// Fetches every folder in `folders` from its cursor in `cursors` (folder id
/// to cursor), retrying transient failures per folder under `policy`.
///
/// # Errors
/// The first folder's error that retries did not fix, or
/// [`MailError::DeadlineExceeded`] once [`FetchLimits::overall_deadline`]
/// has passed; folders fetched before it are lost with it, and their cursors
/// are not advanced.
pub fn fetch_account(
    source: &mut dyn MailSource,
    account: &ConnectionId,
    folders: &[String],
    cursors: &BTreeMap<String, String>,
    limits: &FetchLimits,
    policy: &RetryPolicy,
    now: Timestamp,
) -> Result<AccountFetch, MailError> {
    let window =
        SignedDuration::try_from(limits.first_fetch_window).unwrap_or(SignedDuration::ZERO);
    let since = now.checked_sub(window).unwrap_or(now);
    let started = Instant::now();
    let check_deadline = || {
        if started.elapsed() >= limits.overall_deadline {
            Err(MailError::DeadlineExceeded(limits.overall_deadline))
        } else {
            Ok(())
        }
    };
    let mut fetched = Vec::with_capacity(folders.len());
    for folder in folders {
        let cursor = cursors.get(folder).map(String::as_str);
        let page = policy.run(|| {
            check_deadline()?;
            source.fetch_new(folder, cursor, since)
        })?;
        fetched.push(FolderFetch {
            folder: folder.clone(),
            messages: page.messages,
            cursor: page.cursor,
        });
    }
    Ok(AccountFetch {
        account: account.clone(),
        folders: fetched,
    })
}

/// Records a fetch: each message becomes a signal unless the store holds it,
/// and each folder's cursor moves on.
///
/// # Errors
/// The document refused an edit.
pub fn ingest(
    state: &mut WorkspaceState,
    fetch: &AccountFetch,
) -> Result<IngestSummary, AppStateError> {
    let mut summary = IngestSummary::default();
    for folder in &fetch.folders {
        for message in &folder.messages {
            match state.record_signal(&incoming(&fetch.account, &folder.folder, message))? {
                SignalInsert::Added(id) => summary.added.push(id),
                SignalInsert::Duplicate(_) => summary.duplicates += 1,
            }
        }
        state.set_mail_cursor(&fetch.account, &folder.folder, &folder.cursor)?;
    }
    Ok(summary)
}

/// The cursors this device holds for an account's folders.
#[must_use]
pub fn cursors(
    state: &WorkspaceState,
    account: &ConnectionId,
    folders: &[String],
) -> BTreeMap<String, String> {
    folders
        .iter()
        .filter_map(|folder| Some((folder.clone(), state.mail_cursor(account, folder)?)))
        .collect()
}

fn incoming(account: &ConnectionId, folder: &str, message: &FetchedMessage) -> IncomingSignal {
    IncomingSignal {
        account: account.clone(),
        folder: folder.to_owned(),
        provider_ref: message.provider_ref.clone(),
        message_id: message.message_id.clone(),
        thread_id: message.thread_id.clone(),
        sender_name: message.sender_name.clone(),
        sender_address: message.sender_address.clone(),
        received_at: message.received_at,
        subject: message.subject.clone(),
        excerpt: message.excerpt.clone(),
        link: message.link.clone(),
        raw_text: message.raw_text.clone(),
        full_text_bytes: message.full_text_bytes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::{FetchPage, MailFolder};

    struct CountingSource {
        calls: usize,
    }

    impl MailSource for CountingSource {
        fn list_folders(&mut self) -> Result<Vec<MailFolder>, MailError> {
            Ok(Vec::new())
        }

        fn fetch_new(
            &mut self,
            _folder: &str,
            _cursor: Option<&str>,
            _since: Timestamp,
        ) -> Result<FetchPage, MailError> {
            self.calls += 1;
            Ok(FetchPage {
                messages: Vec::new(),
                cursor: "7:1".to_owned(),
            })
        }

        fn fetch_body(&mut self, _folder: &str, _provider_ref: &str) -> Result<String, MailError> {
            Ok(String::new())
        }

        fn message_link(&self, message_id: &str, _web_link: Option<&str>) -> String {
            message_id.to_owned()
        }
    }

    #[test]
    fn a_fetch_past_its_deadline_stops_before_the_next_call() {
        let mut source = CountingSource { calls: 0 };
        let limits = FetchLimits {
            overall_deadline: Duration::ZERO,
            ..FetchLimits::default()
        };
        let outcome = fetch_account(
            &mut source,
            &ConnectionId::new("con-mail-1"),
            &["INBOX".to_owned()],
            &BTreeMap::new(),
            &limits,
            &RetryPolicy::default(),
            Timestamp::UNIX_EPOCH,
        );
        assert!(matches!(outcome, Err(MailError::DeadlineExceeded(_))));
        assert_eq!(source.calls, 0);
    }

    #[test]
    fn the_default_deadline_is_two_minutes() {
        assert_eq!(
            FetchLimits::default().overall_deadline,
            Duration::from_secs(120)
        );
        let mut source = CountingSource { calls: 0 };
        let fetched = fetch_account(
            &mut source,
            &ConnectionId::new("con-mail-1"),
            &["INBOX".to_owned(), "Sent".to_owned()],
            &BTreeMap::new(),
            &FetchLimits::default(),
            &RetryPolicy::default(),
            Timestamp::UNIX_EPOCH,
        )
        .unwrap();
        assert_eq!(fetched.folders.len(), 2);
    }
}
