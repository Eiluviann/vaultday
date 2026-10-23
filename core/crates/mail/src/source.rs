//! The [`MailSource`] trait, what it returns, and how an account's source is opened.

use std::sync::Arc;

use app_state::{MailAccount, MailAuth, MailProvider, SecretStore};
use jiff::Timestamp;

use crate::error::MailError;
use crate::fetch::FetchLimits;
use crate::graph::{GRAPH_BASE_URL, GraphSource};
use crate::http::{HttpTransport, UreqTransport};
use crate::imap::{ImapAuthSource, ImapConnector, ImapSource, TlsConnector};
use crate::oauth::{DeviceCodeFlow, IMAP_SCOPES, RefreshingTokens};

/// What a folder is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FolderRole {
    /// The inbox.
    Inbox,
    /// Sent mail (Sent Items).
    Sent,
    /// Any other folder.
    Other,
}

/// One folder of a mailbox.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailFolder {
    /// How the source names it in calls: the IMAP mailbox name, or Graph's
    /// well-known name (`inbox`, `sentitems`) or folder id.
    pub id: String,
    /// What the user sees.
    pub name: String,
    /// What it is for.
    pub role: FolderRole,
}

/// One fetched message, before it becomes a signal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchedMessage {
    /// How the provider names it: `<uidvalidity>:<uid>` on IMAP, the message id on Graph.
    pub provider_ref: String,
    /// The `Message-ID` header; the provider reference when the message has none.
    pub message_id: String,
    /// The conversation: Graph's `conversationId`, or on IMAP the first
    /// `References` id, else `In-Reply-To`, else the message's own id.
    pub thread_id: String,
    /// The sender's display name.
    pub sender_name: Option<String>,
    /// The sender's address.
    pub sender_address: String,
    /// When it arrived.
    pub received_at: Timestamp,
    /// The subject, decoded.
    pub subject: String,
    /// A short plain-text excerpt.
    pub excerpt: String,
    /// The text fetched with it, at most [`FetchLimits::max_raw_bytes`].
    pub raw_text: String,
    /// The link that opens it in the account's mail app.
    pub link: String,
}

/// What one fetch of a folder brought.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchPage {
    /// The new messages, oldest first, at most [`FetchLimits::max_messages`].
    pub messages: Vec<FetchedMessage>,
    /// The cursor the next fetch of the folder starts from.
    pub cursor: String,
}

/// A mailbox. Each call opens what it needs and blocks on the network.
pub trait MailSource: Send {
    /// The folders that can be read.
    ///
    /// # Errors
    /// Auth, network or protocol failures.
    fn list_folders(&mut self) -> Result<Vec<MailFolder>, MailError>;

    /// The messages new in `folder` since `cursor`; without a cursor, those
    /// received since `since`.
    ///
    /// # Errors
    /// Auth, network or protocol failures, or [`MailError::FolderNotFound`].
    fn fetch_new(
        &mut self,
        folder: &str,
        cursor: Option<&str>,
        since: Timestamp,
    ) -> Result<FetchPage, MailError>;

    /// The plain text of one message, as far as it can be decoded.
    ///
    /// # Errors
    /// Auth, network or protocol failures.
    fn fetch_body(&mut self, folder: &str, provider_ref: &str) -> Result<String, MailError>;

    /// The link back to a message in the account's mail app: a `message:`
    /// URL for IMAP, Graph's `webLink` for Microsoft 365.
    fn message_link(&self, message_id: &str, web_link: Option<&str>) -> String;
}

/// What sources need from the outside world; tests swap every part.
#[derive(Clone)]
pub struct MailEnvironment {
    /// HTTP for Graph and the OAuth endpoints.
    pub http: Arc<dyn HttpTransport>,
    /// Opens IMAP connections.
    pub imap: Arc<dyn ImapConnector>,
    /// Graph's base URL.
    pub graph_base: String,
    /// The Microsoft sign-in flow (authority and client id); its scopes are
    /// Graph's, and IMAP's for XOAUTH2.
    pub oauth: DeviceCodeFlow,
    /// How much one fetch brings.
    pub limits: FetchLimits,
}

impl std::fmt::Debug for MailEnvironment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MailEnvironment")
            .field("graph_base", &self.graph_base)
            .field("oauth", &self.oauth)
            .field("limits", &self.limits)
            .finish_non_exhaustive()
    }
}

impl MailEnvironment {
    /// The real network: TLS IMAP, HTTPS Graph, Microsoft's authority, under `client_id`.
    #[must_use]
    pub fn production(client_id: &str) -> MailEnvironment {
        MailEnvironment {
            http: Arc::new(UreqTransport::new()),
            imap: Arc::new(TlsConnector::new()),
            graph_base: GRAPH_BASE_URL.to_owned(),
            oauth: DeviceCodeFlow::microsoft(client_id),
            limits: FetchLimits::default(),
        }
    }
}

/// The source for `account`, reading its secret from `secrets` when it signs in.
///
/// # Errors
/// [`MailError::MissingCredential`] when this device holds no password or
/// token for the account.
pub fn open_source(
    account: &MailAccount,
    secrets: Arc<dyn SecretStore>,
    environment: &MailEnvironment,
) -> Result<Box<dyn MailSource>, MailError> {
    match account.provider {
        MailProvider::Imap => {
            let auth = match account.auth {
                MailAuth::XOAuth2 => ImapAuthSource::XOAuth2(RefreshingTokens::new(
                    environment.oauth.with_scopes(IMAP_SCOPES),
                    environment.http.clone(),
                    secrets,
                    account.connection.clone(),
                )),
                MailAuth::AppPassword | MailAuth::DeviceCode => ImapAuthSource::Password(
                    crate::credentials::password(secrets.as_ref(), &account.connection)?,
                ),
            };
            Ok(Box::new(ImapSource::new(
                environment.imap.clone(),
                account.host.clone(),
                account.port,
                account.address.clone(),
                auth,
                environment.limits,
            )))
        }
        MailProvider::MicrosoftGraph => Ok(Box::new(GraphSource::new(
            environment.http.clone(),
            environment.graph_base.clone(),
            RefreshingTokens::new(
                environment.oauth.clone(),
                environment.http.clone(),
                secrets,
                account.connection.clone(),
            ),
            environment.limits,
        ))),
    }
}
