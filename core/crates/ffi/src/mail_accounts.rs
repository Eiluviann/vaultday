//! Mail over the bridge: the accounts in the app-level document, their
//! credentials in the platform's secret store, the connection test, the
//! Microsoft device-code sign-in, the fetch into a workspace's signal store,
//! and the signals themselves with the link that opens each one.
//!
//! Every call that reaches a server blocks until it is done; the app makes
//! them off the main actor. A fetch holds the workspace document only while
//! it records what came, never while it waits on the network.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use app_state::{
    ConnectionId, MailAccount, MailAuth, MailProvider, SecretStore, SignalAction, SignalId,
    SignalRecord, SignalState,
};
use domain::task::TaskId;
use domain::workspace::WorkspaceId;
use jiff::Timestamp;
use mail::credentials;
use mail::error::MailError;
use mail::link::openable_link;
use mail::oauth::{DevicePoll, GRAPH_CLIENT_ID_PLACEHOLDER, GRAPH_SCOPES, IMAP_SCOPES};
use mail::{FolderRole, MailEnvironment, MailFolder, RetryPolicy};

use crate::CoreError;
use crate::app_state::{AppStateHandle, WorkspaceStateHandle, lock};

/// The Microsoft application's client id: set `VAULTDAY_GRAPH_CLIENT_ID`
/// when building the core, else the placeholder, under which sign-in refuses to start.
const GRAPH_CLIENT_ID: &str = match option_env!("VAULTDAY_GRAPH_CLIENT_ID") {
    Some(id) => id,
    None => GRAPH_CLIENT_ID_PLACEHOLDER,
};

/// Which protocol reaches the mailbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MailProviderView {
    /// IMAP over TLS.
    Imap,
    /// Microsoft 365 through Microsoft Graph.
    MicrosoftGraph,
}

/// How an account signs in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MailAuthView {
    /// An app password.
    AppPassword,
    /// An OAuth token over IMAP (Microsoft's IMAP servers).
    XOAuth2,
    /// The device-code flow (Microsoft 365 through Graph).
    DeviceCode,
}

/// One mail account; no secret.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MailAccountView {
    /// The connection id, chosen by the app.
    pub connection: String,
    /// What the user calls it.
    pub label: String,
    /// The workspace its signals go to.
    pub workspace: String,
    /// The address, also the IMAP user name.
    pub address: String,
    /// The protocol.
    pub provider: MailProviderView,
    /// The IMAP server; empty for Microsoft 365.
    pub host: String,
    /// The IMAP port; 0 for Microsoft 365.
    pub port: u16,
    /// How it signs in.
    pub auth: MailAuthView,
    /// The folder ids the workspace reads.
    pub folders: Vec<String>,
}

/// What a folder is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MailFolderRoleView {
    /// The inbox.
    Inbox,
    /// Sent mail.
    Sent,
    /// Anything else.
    Other,
}

/// One folder of a mailbox.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MailFolderView {
    /// The id calls use.
    pub id: String,
    /// What the user sees.
    pub name: String,
    /// What it is for.
    pub role: MailFolderRoleView,
}

/// How a connection attempt ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MailOutcomeView {
    /// Signed in and listed the folders.
    Connected,
    /// The server refused the password or token.
    AuthFailed,
    /// The server could not be reached, or kept failing.
    Unreachable,
    /// This device holds no password or token for the account.
    MissingCredential,
    /// Microsoft sign-in is not configured in this build.
    NotConfigured,
    /// The server answered something unexpected, or a folder is gone.
    ServerError,
}

/// The connection test's result.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ConnectionTestView {
    /// How it ended.
    pub outcome: MailOutcomeView,
    /// The server's or the core's words, empty when connected.
    pub message: String,
    /// The folders offered, inbox and sent mail first; empty unless connected.
    pub folders: Vec<MailFolderView>,
}

/// The code the user enters to sign in to Microsoft 365. Its `Debug` leaves
/// out the device code, which with the client id is enough to collect the grant.
#[derive(Clone, PartialEq, Eq, uniffi::Record)]
pub struct DeviceCodePromptView {
    /// What the app polls with; never shown.
    pub device_code: String,
    /// What the user types.
    pub user_code: String,
    /// Where the user types it.
    pub verification_uri: String,
    /// How long the code is valid, in seconds.
    pub expires_in_seconds: u64,
    /// How long to wait between polls, in seconds.
    pub interval_seconds: u64,
}

impl std::fmt::Debug for DeviceCodePromptView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeviceCodePromptView")
            .field("user_code", &self.user_code)
            .field("verification_uri", &self.verification_uri)
            .field("expires_in_seconds", &self.expires_in_seconds)
            .field("interval_seconds", &self.interval_seconds)
            .finish_non_exhaustive()
    }
}

/// What one poll of the sign-in found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum DeviceCodePollView {
    /// Not finished; poll again after the interval.
    Pending,
    /// Polling too fast; wait five seconds longer.
    SlowDown,
    /// Signed in; the refresh token is in the secret store.
    SignedIn,
    /// The user declined.
    Declined,
    /// The code expired.
    Expired,
}

/// One account that failed in a fetch.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MailFetchFailureView {
    /// The account.
    pub connection: String,
    /// How it failed.
    pub outcome: MailOutcomeView,
    /// The words for the user.
    pub message: String,
}

/// What a fetch did.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MailFetchView {
    /// Accounts fetched.
    pub accounts: u32,
    /// Signals new to the store.
    pub added: u32,
    /// Messages the store already held.
    pub duplicates: u32,
    /// Accounts that failed; the others were recorded.
    pub failures: Vec<MailFetchFailureView>,
}

/// Where a signal stands, as shown now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum SignalStateView {
    /// Not decided.
    New,
    /// Hidden until `snoozed_until`.
    Snoozed,
    /// Put away for good.
    Dismissed,
    /// Put away as not a signal.
    Rejected,
    /// Kept on a story without a task.
    Attached,
    /// Turned into a task.
    Converted,
}

/// One signal for the Inbox column.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SignalView {
    /// `sig-…`.
    pub id: String,
    /// The account's connection id.
    pub account: String,
    /// The folder it came from.
    pub folder: String,
    /// The sender's name.
    pub sender_name: Option<String>,
    /// The sender's address.
    pub sender_address: String,
    /// RFC 3339 arrival time.
    pub received_at: String,
    /// The subject.
    pub subject: String,
    /// A short excerpt.
    pub excerpt: String,
    /// The conversation.
    pub thread_id: String,
    /// The state as of now.
    pub state: SignalStateView,
    /// RFC 3339 end of a snooze.
    pub snoozed_until: Option<String>,
    /// The task a converted signal became.
    pub task: Option<String>,
    /// The story an attached signal is kept on.
    pub story: Option<String>,
}

/// Whether Microsoft 365 sign-in can run in this build.
#[uniffi::export]
#[must_use]
pub fn microsoft_sign_in_is_configured() -> bool {
    environment().oauth.is_configured()
}

#[uniffi::export]
impl AppStateHandle {
    /// Every mail account with its folders.
    pub fn mail_accounts(&self) -> Vec<MailAccountView> {
        self.read_state(|state| {
            state
                .mail_accounts()
                .into_iter()
                .map(|account| {
                    let folders = state.mail_folders(&account);
                    account_view(account, folders)
                })
                .collect()
        })
    }

    /// Adds or replaces a mail account and the folders its workspace reads.
    ///
    /// # Errors
    /// [`CoreError::InvalidInput`] for an incomplete account; the document
    /// refused the edit otherwise.
    pub fn put_mail_account(&self, account: MailAccountView) -> Result<(), CoreError> {
        let folders = account.folders.clone();
        let account = core_account(account);
        self.edit_state(|state| {
            state.put_mail_account(&account)?;
            state.set_mail_folders(&account, &folders)
        })
    }

    /// Removes a mail account, its folders and this device's secrets for it.
    ///
    /// # Errors
    /// The secret store failed, or the document refused the edit.
    pub fn remove_mail_account(&self, connection: String) -> Result<(), CoreError> {
        let connection = ConnectionId::new(connection);
        credentials::delete_credentials(self.secrets(), &connection)
            .map_err(|error| CoreError::SecretStore(error.to_string()))?;
        self.edit_state(|state| state.remove_connection(&connection))
    }

    /// Stores an account's app password in the secret store, on this device only.
    ///
    /// # Errors
    /// [`CoreError::SecretStore`] when the store fails.
    pub fn store_mail_password(
        &self,
        connection: String,
        password: String,
    ) -> Result<(), CoreError> {
        credentials::store_password(self.secrets(), &ConnectionId::new(connection), &password)
            .map_err(|error| CoreError::SecretStore(error.to_string()))
    }

    /// Whether this device holds the account's password or token.
    ///
    /// # Errors
    /// [`CoreError::SecretStore`] when the store fails.
    pub fn has_mail_credential(&self, account: MailAccountView) -> Result<bool, CoreError> {
        credentials::has_credential(self.secrets(), &core_account(account))
            .map_err(|error| CoreError::SecretStore(error.to_string()))
    }

    /// Deletes this device's secrets for an account that was never saved.
    ///
    /// # Errors
    /// [`CoreError::SecretStore`] when the store fails.
    pub fn forget_mail_credentials(&self, connection: String) -> Result<(), CoreError> {
        credentials::delete_credentials(self.secrets(), &ConnectionId::new(connection))
            .map_err(|error| CoreError::SecretStore(error.to_string()))
    }

    /// How often each device fetches mail, in seconds.
    pub fn mail_fetch_interval_seconds(&self) -> u64 {
        self.read_state(|state| state.mail_fetch_interval().as_secs())
    }

    /// Sets how often each device fetches mail; below a minute reads as a minute.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_mail_fetch_interval_seconds(&self, seconds: u64) -> Result<(), CoreError> {
        self.edit_state(|state| state.set_mail_fetch_interval(Duration::from_secs(seconds)))
    }

    /// Signs in with the stored secret and lists the folders. Blocks on the network.
    pub fn test_mail_connection(&self, account: MailAccountView) -> ConnectionTestView {
        let account = core_account(account);
        let outcome = account
            .validate()
            .map_err(|error| MailError::Protocol(error.to_string()))
            .and_then(|()| {
                let mut source =
                    mail::open_source(&account, self.shared_secrets(), &environment())?;
                RetryPolicy::default().run(|| source.list_folders())
            });
        match outcome {
            Ok(folders) => ConnectionTestView {
                outcome: MailOutcomeView::Connected,
                message: String::new(),
                folders: offered_folders(folders),
            },
            Err(error) => ConnectionTestView {
                outcome: outcome_view(&error),
                message: error.to_string(),
                folders: Vec::new(),
            },
        }
    }

    /// Starts the Microsoft device-code sign-in for an account signing in
    /// with `auth` (Graph, or IMAP with XOAUTH2). Blocks on the network.
    ///
    /// # Errors
    /// [`CoreError::Mail`] when sign-in is not configured or the service fails.
    pub fn start_microsoft_sign_in(
        &self,
        auth: MailAuthView,
    ) -> Result<DeviceCodePromptView, CoreError> {
        let environment = environment();
        let flow = environment.oauth.with_scopes(scopes(auth));
        let prompt = flow.start(environment.http.as_ref()).map_err(mail_error)?;
        Ok(DeviceCodePromptView {
            device_code: prompt.device_code,
            user_code: prompt.user_code,
            verification_uri: prompt.verification_uri,
            expires_in_seconds: prompt.expires_in.as_secs(),
            interval_seconds: prompt.interval.as_secs(),
        })
    }

    /// Polls the sign-in once; when granted, stores the refresh token for
    /// `connection` on this device. Blocks on the network.
    ///
    /// # Errors
    /// [`CoreError::Mail`] when the service fails; [`CoreError::SecretStore`]
    /// when the token cannot be stored.
    pub fn poll_microsoft_sign_in(
        &self,
        connection: String,
        auth: MailAuthView,
        device_code: String,
    ) -> Result<DeviceCodePollView, CoreError> {
        let environment = environment();
        let flow = environment.oauth.with_scopes(scopes(auth));
        let poll = flow
            .poll(environment.http.as_ref(), &device_code)
            .map_err(mail_error)?;
        Ok(match poll {
            DevicePoll::Pending => DeviceCodePollView::Pending,
            DevicePoll::SlowDown => DeviceCodePollView::SlowDown,
            DevicePoll::Declined => DeviceCodePollView::Declined,
            DevicePoll::Expired => DeviceCodePollView::Expired,
            DevicePoll::Granted(tokens) => {
                let Some(refresh_token) = tokens.refresh_token else {
                    return Err(CoreError::Mail(
                        "Microsoft signed in without a refresh token; sign in again".to_owned(),
                    ));
                };
                credentials::store_refresh_token(
                    self.secrets(),
                    &ConnectionId::new(connection),
                    &refresh_token,
                )
                .map_err(|error| CoreError::SecretStore(error.to_string()))?;
                DeviceCodePollView::SignedIn
            }
        })
    }

    /// Fetches the mail accounts of `workspace` (or only `connection`) into
    /// its signal store. Blocks on the network without holding `state`; an
    /// account that fails is reported and the others are still recorded.
    ///
    /// # Errors
    /// The document refused to record a fetch.
    pub fn fetch_mail(
        &self,
        state: Arc<WorkspaceStateHandle>,
        workspace: String,
        connection: Option<String>,
    ) -> Result<MailFetchView, CoreError> {
        let workspace = WorkspaceId::new(workspace);
        let accounts: Vec<(MailAccount, Vec<String>)> = self.read_state(|app| {
            app.mail_accounts()
                .into_iter()
                .filter(|account| account.workspace == workspace)
                .filter(|account| {
                    connection
                        .as_deref()
                        .is_none_or(|wanted| account.connection.as_str() == wanted)
                })
                .map(|account| {
                    let folders = app.mail_folders(&account);
                    (account, folders)
                })
                .collect()
        });
        let environment = environment();
        let mut view = MailFetchView {
            accounts: 0,
            added: 0,
            duplicates: 0,
            failures: Vec::new(),
        };
        for (account, folders) in accounts {
            if folders.is_empty() {
                continue;
            }
            view.accounts += 1;
            let held: BTreeMap<String, String> = {
                let sync = lock(&state.sync);
                mail::fetch::cursors(sync.state(), &account.connection, &folders)
            };
            let fetched = mail::open_source(&account, self.shared_secrets(), &environment)
                .and_then(|mut source| {
                    mail::fetch_account(
                        source.as_mut(),
                        &account.connection,
                        &folders,
                        &held,
                        &environment.limits,
                        &RetryPolicy::default(),
                        Timestamp::now(),
                    )
                });
            match fetched {
                Ok(fetched) => {
                    let summary = lock(&state.sync)
                        .edit(Instant::now(), |doc| mail::ingest(doc, &fetched))?;
                    view.added += count(summary.added.len());
                    view.duplicates += count(summary.duplicates);
                }
                Err(error) => view.failures.push(MailFetchFailureView {
                    connection: account.connection.as_str().to_owned(),
                    outcome: outcome_view(&error),
                    message: error.to_string(),
                }),
            }
        }
        Ok(view)
    }
}

#[uniffi::export]
impl WorkspaceStateHandle {
    /// Every signal, newest first, in its state as of now.
    pub fn signals(&self) -> Vec<SignalView> {
        let now = Timestamp::now();
        lock(&self.sync)
            .state()
            .signals()
            .into_iter()
            .map(|record| signal_view(record, now))
            .collect()
    }

    /// How many signals are new right now.
    pub fn new_signal_count(&self) -> u32 {
        let now = Timestamp::now();
        let count_new = lock(&self.sync)
            .state()
            .signals()
            .iter()
            .filter(|record| record.state.effective(now) == SignalState::New)
            .count();
        count(count_new)
    }

    /// Hides a signal until `until` (RFC 3339).
    ///
    /// # Errors
    /// [`CoreError::InvalidDate`] for a malformed time, [`CoreError::NotFound`]
    /// for an unknown signal, [`CoreError::AppState`] when it is final or the
    /// time has passed.
    pub fn snooze_signal(&self, id: String, until: String) -> Result<SignalStateView, CoreError> {
        let until: Timestamp = until
            .parse()
            .map_err(|_| CoreError::InvalidDate(format!("{until} is not an RFC 3339 time")))?;
        self.act_on_signal(&id, &SignalAction::Snooze { until })
    }

    /// Puts a signal away for good; a refetch never brings it back.
    ///
    /// # Errors
    /// [`CoreError::NotFound`] when the signal is unknown, [`CoreError::AppState`]
    /// when it is already final.
    pub fn dismiss_signal(&self, id: String) -> Result<SignalStateView, CoreError> {
        self.act_on_signal(&id, &SignalAction::Dismiss)
    }

    /// Marks a signal converted into `task` (PR 13 writes the task line).
    ///
    /// # Errors
    /// [`CoreError::NotFound`] when the signal is unknown, [`CoreError::AppState`]
    /// when it is already final.
    pub fn mark_signal_converted(
        &self,
        id: String,
        task: Option<String>,
    ) -> Result<SignalStateView, CoreError> {
        self.act_on_signal(
            &id,
            &SignalAction::Convert {
                task: task.map(TaskId::new),
            },
        )
    }

    /// The link that opens a signal's message in its account's mail app:
    /// only a `message:` or `https:` link ever comes back.
    ///
    /// # Errors
    /// [`CoreError::NotFound`] when the signal is unknown.
    pub fn signal_link(&self, id: String) -> Result<Option<String>, CoreError> {
        let record = lock(&self.sync)
            .state()
            .signal(&SignalId::new(id.as_str()))
            .ok_or_else(|| CoreError::NotFound(format!("no signal {id}")))?;
        Ok(openable_link(&record.signal.link).map(str::to_owned))
    }
}

impl WorkspaceStateHandle {
    pub(crate) fn act_on_signal(
        &self,
        id: &str,
        action: &SignalAction,
    ) -> Result<SignalStateView, CoreError> {
        let id = SignalId::new(id);
        let now = Timestamp::now();
        let state = lock(&self.sync).edit(Instant::now(), |doc| {
            doc.apply_signal_action(&id, action, now)
        })?;
        Ok(state_parts(&state).state)
    }
}

impl AppStateHandle {
    fn shared_secrets(&self) -> Arc<dyn SecretStore> {
        Arc::new(self.secrets().clone())
    }
}

fn environment() -> MailEnvironment {
    MailEnvironment::production(GRAPH_CLIENT_ID)
}

fn scopes(auth: MailAuthView) -> &'static str {
    match auth {
        MailAuthView::XOAuth2 => IMAP_SCOPES,
        MailAuthView::AppPassword | MailAuthView::DeviceCode => GRAPH_SCOPES,
    }
}

fn offered_folders(folders: Vec<MailFolder>) -> Vec<MailFolderView> {
    let mut views: Vec<MailFolderView> = folders
        .into_iter()
        .map(|folder| MailFolderView {
            id: folder.id,
            name: folder.name,
            role: match folder.role {
                FolderRole::Inbox => MailFolderRoleView::Inbox,
                FolderRole::Sent => MailFolderRoleView::Sent,
                FolderRole::Other => MailFolderRoleView::Other,
            },
        })
        .collect();
    views.sort_by_key(|folder| match folder.role {
        MailFolderRoleView::Inbox => 0,
        MailFolderRoleView::Sent => 1,
        MailFolderRoleView::Other => 2,
    });
    views
}

fn outcome_view(error: &MailError) -> MailOutcomeView {
    match error {
        MailError::Auth(_) => MailOutcomeView::AuthFailed,
        MailError::Network { .. } => MailOutcomeView::Unreachable,
        MailError::MissingCredential(_) | MailError::SecretStore(_) => {
            MailOutcomeView::MissingCredential
        }
        MailError::NotConfigured(_) => MailOutcomeView::NotConfigured,
        MailError::Protocol(_) | MailError::FolderNotFound(_) => MailOutcomeView::ServerError,
    }
}

fn mail_error(error: MailError) -> CoreError {
    CoreError::Mail(error.to_string())
}

fn account_view(account: MailAccount, folders: Vec<String>) -> MailAccountView {
    MailAccountView {
        connection: account.connection.as_str().to_owned(),
        label: account.label,
        workspace: account.workspace.as_str().to_owned(),
        address: account.address,
        provider: match account.provider {
            MailProvider::Imap => MailProviderView::Imap,
            MailProvider::MicrosoftGraph => MailProviderView::MicrosoftGraph,
        },
        host: account.host,
        port: account.port,
        auth: match account.auth {
            MailAuth::AppPassword => MailAuthView::AppPassword,
            MailAuth::XOAuth2 => MailAuthView::XOAuth2,
            MailAuth::DeviceCode => MailAuthView::DeviceCode,
        },
        folders,
    }
}

fn core_account(view: MailAccountView) -> MailAccount {
    MailAccount {
        connection: ConnectionId::new(view.connection),
        label: view.label,
        workspace: WorkspaceId::new(view.workspace),
        address: view.address,
        provider: match view.provider {
            MailProviderView::Imap => MailProvider::Imap,
            MailProviderView::MicrosoftGraph => MailProvider::MicrosoftGraph,
        },
        host: view.host,
        port: view.port,
        auth: match view.auth {
            MailAuthView::AppPassword => MailAuth::AppPassword,
            MailAuthView::XOAuth2 => MailAuth::XOAuth2,
            MailAuthView::DeviceCode => MailAuth::DeviceCode,
        },
    }
}

fn signal_view(record: SignalRecord, now: Timestamp) -> SignalView {
    let StateParts {
        state,
        snoozed_until,
        task,
        story,
    } = state_parts(&record.state.effective(now));
    let signal = record.signal;
    SignalView {
        id: signal.id.as_str().to_owned(),
        account: signal.account.as_str().to_owned(),
        folder: signal.folder,
        sender_name: signal.sender_name,
        sender_address: signal.sender_address,
        received_at: signal.received_at.to_string(),
        subject: signal.subject,
        excerpt: signal.excerpt,
        thread_id: signal.thread_id,
        state,
        snoozed_until,
        task,
        story,
    }
}

/// A stored state as the bridge's flat fields.
pub(crate) struct StateParts {
    pub(crate) state: SignalStateView,
    pub(crate) snoozed_until: Option<String>,
    pub(crate) task: Option<String>,
    pub(crate) story: Option<String>,
}

pub(crate) fn state_parts(state: &SignalState) -> StateParts {
    let bare = |state| StateParts {
        state,
        snoozed_until: None,
        task: None,
        story: None,
    };
    match state {
        SignalState::New => bare(SignalStateView::New),
        SignalState::Snoozed { until } => StateParts {
            snoozed_until: Some(until.to_string()),
            ..bare(SignalStateView::Snoozed)
        },
        SignalState::Dismissed => bare(SignalStateView::Dismissed),
        SignalState::Rejected => bare(SignalStateView::Rejected),
        SignalState::Attached { story } => StateParts {
            story: Some(story.clone()),
            ..bare(SignalStateView::Attached)
        },
        SignalState::Converted { task } => StateParts {
            task: task.as_ref().map(|task| task.as_str().to_owned()),
            ..bare(SignalStateView::Converted)
        },
    }
}

fn count(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_state::tests::Keychain;
    use crate::app_state::{create_workspace_state, open_app_state};
    use crate::files::plain_sync_files;

    #[test]
    fn placeholder_builds_refuse_microsoft_sign_in() {
        if option_env!("VAULTDAY_GRAPH_CLIENT_ID").is_none() {
            assert!(!microsoft_sign_in_is_configured());
        }
    }

    #[test]
    fn outcomes_map_to_what_the_user_is_told() {
        assert_eq!(
            outcome_view(&MailError::Auth("no".to_owned())),
            MailOutcomeView::AuthFailed
        );
        assert_eq!(
            outcome_view(&MailError::Network {
                message: "reset".to_owned(),
                retry_after: None
            }),
            MailOutcomeView::Unreachable
        );
        assert_eq!(
            outcome_view(&MailError::FolderNotFound("Gone".to_owned())),
            MailOutcomeView::ServerError
        );
        assert_eq!(
            outcome_view(&MailError::NotConfigured("x".to_owned())),
            MailOutcomeView::NotConfigured
        );
        let offered = offered_folders(vec![
            MailFolder {
                id: "Archive".to_owned(),
                name: "Archive".to_owned(),
                role: FolderRole::Other,
            },
            MailFolder {
                id: "Sent".to_owned(),
                name: "Sent".to_owned(),
                role: FolderRole::Sent,
            },
            MailFolder {
                id: "INBOX".to_owned(),
                name: "Inbox".to_owned(),
                role: FolderRole::Inbox,
            },
        ]);
        let ids: Vec<&str> = offered.iter().map(|folder| folder.id.as_str()).collect();
        assert_eq!(ids, ["INBOX", "Sent", "Archive"]);
    }

    #[test]
    fn accounts_round_trip_and_their_passwords_stay_in_the_secret_store() {
        let dir = tempfile::tempdir().unwrap();
        let port = Arc::new(Keychain::default());
        let app = open_app_state(
            plain_sync_files(),
            dir.path().join("app").to_string_lossy().into_owned(),
            port.clone(),
        )
        .unwrap();
        let account = MailAccountView {
            connection: "con-mail-1".to_owned(),
            label: "someone@example.test".to_owned(),
            workspace: "ws-1".to_owned(),
            address: "someone@example.test".to_owned(),
            provider: MailProviderView::Imap,
            host: "imap.example.test".to_owned(),
            port: 993,
            auth: MailAuthView::AppPassword,
            folders: vec!["INBOX".to_owned()],
        };
        app.store_mail_password("con-mail-1".to_owned(), "synthetic-app-password".to_owned())
            .unwrap();
        app.put_mail_account(account.clone()).unwrap();
        let snapshot = app.save().unwrap();
        assert_eq!(app.mail_accounts(), std::slice::from_ref(&account));
        assert!(app.has_mail_credential(account.clone()).unwrap());
        let bytes = std::fs::read(snapshot).unwrap();
        assert!(
            !bytes
                .windows(b"synthetic-app-password".len())
                .any(|window| window == b"synthetic-app-password"),
            "the password never reaches the app document"
        );
        assert!(matches!(
            app.put_mail_account(MailAccountView {
                host: String::new(),
                ..account.clone()
            }),
            Err(CoreError::InvalidInput(_))
        ));
        app.remove_mail_account("con-mail-1".to_owned()).unwrap();
        assert!(app.mail_accounts().is_empty());
        assert!(!app.has_mail_credential(account).unwrap());
        assert_eq!(app.mail_fetch_interval_seconds(), 300);
    }

    #[test]
    fn a_fetch_without_a_password_is_reported_per_account() {
        let dir = tempfile::tempdir().unwrap();
        let port = Arc::new(Keychain::default());
        let app = open_app_state(
            plain_sync_files(),
            dir.path().join("app").to_string_lossy().into_owned(),
            port.clone(),
        )
        .unwrap();
        let workspace = dir.path().join("vault");
        std::fs::create_dir_all(&workspace).unwrap();
        let state = create_workspace_state(
            plain_sync_files(),
            workspace.to_string_lossy().into_owned(),
            "ws-1".to_owned(),
            app.device_id(),
            port,
        )
        .unwrap();
        app.put_mail_account(MailAccountView {
            connection: "con-mail-1".to_owned(),
            label: "someone@example.test".to_owned(),
            workspace: "ws-1".to_owned(),
            address: "someone@example.test".to_owned(),
            provider: MailProviderView::Imap,
            host: "imap.example.test".to_owned(),
            port: 993,
            auth: MailAuthView::AppPassword,
            folders: vec!["INBOX".to_owned()],
        })
        .unwrap();
        let fetched = app
            .fetch_mail(state.clone(), "ws-1".to_owned(), None)
            .unwrap();
        assert_eq!(fetched.accounts, 1);
        assert_eq!(fetched.added, 0);
        assert_eq!(
            fetched.failures[0].outcome,
            MailOutcomeView::MissingCredential
        );
        assert!(state.signals().is_empty());
        assert!(matches!(
            state.dismiss_signal("sig-0000000000000000".to_owned()),
            Err(CoreError::NotFound(_))
        ));
        assert!(matches!(
            state.signal_link("sig-0000000000000000".to_owned()),
            Err(CoreError::NotFound(_))
        ));
    }
}
