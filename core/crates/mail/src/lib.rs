//! Vaultday's mail sources. Baseline mail reaches a mailbox two ways, both
//! behind the one [`MailSource`] trait:
//!
//! - [`imap::ImapSource`]: IMAP over implicit TLS (rustls with the Mozilla
//!   roots), signing in with an app password (`LOGIN`) or an OAuth access
//!   token (`AUTHENTICATE XOAUTH2`) where the provider demands it. Folders are
//!   only ever `EXAMINE`d, so a fetch never marks mail read.
//! - [`graph::GraphSource`]: Microsoft 365 through Microsoft Graph, signed in
//!   with the OAuth device-code flow ([`oauth::DeviceCodeFlow`]) under a
//!   configurable client id.
//!
//! [`fetch::fetch_account`] runs one account's selected folders with bounded
//! retries on transient failures, and [`fetch::ingest`] records what it
//! brought into the encrypted signal store of the workspace document. Every
//! call here blocks on the network: callers run it off the main thread.
//! Passwords and tokens come only from the secret store
//! ([`credentials`]); none is ever written to the vault, `settings.json`
//! or the app document.

// The workspace lints forbid `unwrap`/`expect` in shipped code; tests may use them.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod credentials;
pub mod error;
pub mod fetch;
pub mod graph;
pub mod http;
pub mod imap;
pub mod link;
pub mod mime;
pub mod oauth;
pub mod retry;
mod source;

pub use error::MailError;
pub use fetch::{AccountFetch, FetchLimits, FolderFetch, IngestSummary, fetch_account, ingest};
pub use retry::RetryPolicy;
pub use source::{
    FetchPage, FetchedMessage, FolderRole, MailEnvironment, MailFolder, MailSource, open_source,
};
