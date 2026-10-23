//! Mail accounts in the app-level document. An account is a connection of
//! kind [`ConnectionKind::MailAccount`] whose record also holds where the
//! server is and how to sign in; the folders a workspace reads from it are
//! sources (`subpath` is the folder id). Passwords and tokens never come
//! here: they live in the secret store under the connection id.

use std::time::Duration;

use automerge::transaction::Transactable;
use domain::workspace::WorkspaceId;

use super::{AppState, Connection, ConnectionId, ConnectionKind, Source};
use crate::doc::{get_map, get_str, get_u64};
use crate::error::AppStateError;

/// How often each device fetches mail until the user picks another interval.
pub const DEFAULT_MAIL_FETCH_INTERVAL: Duration = Duration::from_secs(5 * 60);
/// The shortest interval the setting accepts.
const MIN_MAIL_FETCH_INTERVAL: Duration = Duration::from_secs(60);
const FETCH_INTERVAL_KEY: &str = "mail_fetch_interval_secs";

/// Which protocol reaches the mailbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MailProvider {
    /// IMAP over TLS.
    Imap,
    /// Microsoft 365 through Microsoft Graph.
    MicrosoftGraph,
}

/// How the account signs in; the secret itself is in the secret store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MailAuth {
    /// An app password (IMAP `LOGIN`).
    AppPassword,
    /// An OAuth access token over IMAP (`AUTHENTICATE XOAUTH2`), from a
    /// refresh token the device-code flow stored.
    XOAuth2,
    /// The OAuth device-code flow (Microsoft Graph).
    DeviceCode,
}

/// One mail account. Holds no secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailAccount {
    /// The account's connection id; its secrets are stored under it.
    pub connection: ConnectionId,
    /// What the user calls it, usually the address.
    pub label: String,
    /// The workspace its signals go to.
    pub workspace: WorkspaceId,
    /// The address, also the IMAP user name.
    pub address: String,
    /// The protocol.
    pub provider: MailProvider,
    /// The IMAP host; empty for Graph.
    pub host: String,
    /// The IMAP port (993 for implicit TLS); 0 for Graph.
    pub port: u16,
    /// How it signs in.
    pub auth: MailAuth,
}

impl MailProvider {
    const fn label(self) -> &'static str {
        match self {
            MailProvider::Imap => "imap",
            MailProvider::MicrosoftGraph => "microsoft-graph",
        }
    }

    fn parse(label: &str) -> Option<MailProvider> {
        [MailProvider::Imap, MailProvider::MicrosoftGraph]
            .into_iter()
            .find(|provider| provider.label() == label)
    }
}

impl MailAuth {
    const fn label(self) -> &'static str {
        match self {
            MailAuth::AppPassword => "app-password",
            MailAuth::XOAuth2 => "xoauth2",
            MailAuth::DeviceCode => "device-code",
        }
    }

    fn parse(label: &str) -> Option<MailAuth> {
        [
            MailAuth::AppPassword,
            MailAuth::XOAuth2,
            MailAuth::DeviceCode,
        ]
        .into_iter()
        .find(|auth| auth.label() == label)
    }
}

impl MailAccount {
    /// Checks what the user typed: an address, and for IMAP a host and a
    /// port; Graph signs in only through the device-code flow.
    ///
    /// # Errors
    /// [`AppStateError::InvalidMailAccount`] naming the first problem.
    pub fn validate(&self) -> Result<(), AppStateError> {
        let invalid = |message: &str| Err(AppStateError::InvalidMailAccount(message.to_owned()));
        if self.address.trim().is_empty() {
            return invalid("an account needs an address");
        }
        match self.provider {
            MailProvider::Imap => {
                if self.host.trim().is_empty() {
                    return invalid("an IMAP account needs a server");
                }
                if self.port == 0 {
                    return invalid("an IMAP account needs a port");
                }
                if self.auth == MailAuth::DeviceCode {
                    return invalid("an IMAP account signs in with an app password or XOAUTH2");
                }
            }
            MailProvider::MicrosoftGraph => {
                if self.auth != MailAuth::DeviceCode {
                    return invalid("a Microsoft 365 account signs in with the device code");
                }
            }
        }
        Ok(())
    }
}

impl AppState {
    /// Every mail account, by connection id.
    #[must_use]
    pub fn mail_accounts(&self) -> Vec<MailAccount> {
        self.connections()
            .into_iter()
            .filter(|connection| connection.kind == ConnectionKind::MailAccount)
            .filter_map(|connection| self.mail_account(&connection.id))
            .collect()
    }

    /// One mail account.
    #[must_use]
    pub fn mail_account(&self, id: &ConnectionId) -> Option<MailAccount> {
        let connection = self.connection(id)?;
        if connection.kind != ConnectionKind::MailAccount {
            return None;
        }
        let record = get_map(&self.doc, &self.objects.connections, id.as_str())?;
        let field = |key: &str| get_str(&self.doc, &record, key);
        Some(MailAccount {
            connection: id.clone(),
            label: connection.label,
            workspace: WorkspaceId::new(field("mail_workspace")?),
            address: field("mail_address")?,
            provider: MailProvider::parse(&field("mail_provider")?)?,
            host: field("mail_host").unwrap_or_default(),
            port: get_u64(&self.doc, &record, "mail_port")
                .and_then(|port| u16::try_from(port).ok())
                .unwrap_or(0),
            auth: MailAuth::parse(&field("mail_auth")?)?,
        })
    }

    /// Adds a mail account or replaces the one with its connection id.
    ///
    /// # Errors
    /// [`AppStateError::InvalidMailAccount`] for an incomplete account, or
    /// the document refused the edit.
    pub fn put_mail_account(&mut self, account: &MailAccount) -> Result<(), AppStateError> {
        account.validate()?;
        self.put_connection(&Connection {
            id: account.connection.clone(),
            kind: ConnectionKind::MailAccount,
            label: account.label.clone(),
        })?;
        let Some(record) = get_map(
            &self.doc,
            &self.objects.connections,
            account.connection.as_str(),
        ) else {
            return Err(AppStateError::MissingSchemaObject(format!(
                "connections/{}",
                account.connection
            )));
        };
        let doc = &mut self.doc;
        doc.put(&record, "mail_workspace", account.workspace.as_str())?;
        doc.put(&record, "mail_address", account.address.trim())?;
        doc.put(&record, "mail_provider", account.provider.label())?;
        doc.put(&record, "mail_host", account.host.trim())?;
        doc.put(&record, "mail_port", u64::from(account.port))?;
        doc.put(&record, "mail_auth", account.auth.label())?;
        Ok(())
    }

    /// The folder ids an account's workspace reads, sorted.
    #[must_use]
    pub fn mail_folders(&self, account: &MailAccount) -> Vec<String> {
        self.sources(&account.workspace)
            .into_iter()
            .filter(|source| source.vault == account.connection)
            .map(|source| source.subpath)
            .collect()
    }

    /// Replaces the folders an account's workspace reads.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_mail_folders(
        &mut self,
        account: &MailAccount,
        folders: &[String],
    ) -> Result<(), AppStateError> {
        for source in self
            .all_sources()
            .into_iter()
            .filter(|source| source.vault == account.connection)
        {
            self.remove_source(&source)?;
        }
        for folder in folders {
            self.add_source(&Source {
                workspace: account.workspace.clone(),
                vault: account.connection.clone(),
                subpath: folder.clone(),
            })?;
        }
        Ok(())
    }

    /// How often each device fetches mail.
    #[must_use]
    pub fn mail_fetch_interval(&self) -> Duration {
        get_u64(&self.doc, &self.objects.settings, FETCH_INTERVAL_KEY)
            .map_or(DEFAULT_MAIL_FETCH_INTERVAL, Duration::from_secs)
            .max(MIN_MAIL_FETCH_INTERVAL)
    }

    /// Sets how often each device fetches mail; below a minute reads as a minute.
    ///
    /// # Errors
    /// The document refused the edit.
    pub fn set_mail_fetch_interval(&mut self, interval: Duration) -> Result<(), AppStateError> {
        let seconds = interval.max(MIN_MAIL_FETCH_INTERVAL).as_secs();
        self.doc
            .put(&self.objects.settings, FETCH_INTERVAL_KEY, seconds)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::DeviceId;

    fn state() -> AppState {
        AppState::new(DeviceId::new("mac-a").unwrap()).unwrap()
    }

    fn imap() -> MailAccount {
        MailAccount {
            connection: ConnectionId::new("con-mail-1"),
            label: "someone@example.test".to_owned(),
            workspace: WorkspaceId::new("ws-1"),
            address: "someone@example.test".to_owned(),
            provider: MailProvider::Imap,
            host: "imap.example.test".to_owned(),
            port: 993,
            auth: MailAuth::AppPassword,
        }
    }

    #[test]
    fn accounts_and_folders_round_trip() {
        let mut state = state();
        let account = imap();
        state.put_mail_account(&account).unwrap();
        assert_eq!(state.mail_accounts(), std::slice::from_ref(&account));
        assert_eq!(state.connections()[0].kind, ConnectionKind::MailAccount);
        state
            .set_mail_folders(&account, &["INBOX".to_owned(), "Sent".to_owned()])
            .unwrap();
        assert_eq!(state.mail_folders(&account), ["INBOX", "Sent"]);
        state
            .set_mail_folders(&account, &["INBOX".to_owned()])
            .unwrap();
        assert_eq!(state.mail_folders(&account), ["INBOX"]);
        state.remove_connection(&account.connection).unwrap();
        assert!(state.mail_accounts().is_empty());
        assert!(state.all_sources().is_empty());
    }

    #[test]
    fn incomplete_accounts_are_refused() {
        let mut state = state();
        for broken in [
            MailAccount {
                host: String::new(),
                ..imap()
            },
            MailAccount { port: 0, ..imap() },
            MailAccount {
                address: " ".to_owned(),
                ..imap()
            },
            MailAccount {
                provider: MailProvider::MicrosoftGraph,
                ..imap()
            },
        ] {
            assert!(matches!(
                state.put_mail_account(&broken),
                Err(AppStateError::InvalidMailAccount(_))
            ));
        }
        assert!(state.connections().is_empty());
    }

    #[test]
    fn the_fetch_interval_defaults_to_five_minutes() {
        let mut state = state();
        assert_eq!(state.mail_fetch_interval(), DEFAULT_MAIL_FETCH_INTERVAL);
        state
            .set_mail_fetch_interval(Duration::from_secs(10))
            .unwrap();
        assert_eq!(state.mail_fetch_interval(), Duration::from_secs(60));
        state
            .set_mail_fetch_interval(Duration::from_secs(900))
            .unwrap();
        assert_eq!(state.mail_fetch_interval(), Duration::from_secs(900));
    }
}
