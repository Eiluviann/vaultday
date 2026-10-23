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
//!
//! # Why there is no registry inbox-source slot
//!
//! Mail is baseline: every install reads its accounts, with or without a
//! feature set, so this crate is called by the core directly instead of
//! contributing through a [`domain::registry`] slot. The registry's slots
//! exist for what a feature set adds after v1 (ticket providers, time sinks,
//! rail tabs); a slot for inbox sources would have exactly one,
//! always-present contributor and would only add an enablement check that
//! can never be off. A later source that does belong to a feature set (a
//! chat or ticket feed) gets a slot then, beside mail rather than over it.
//!
//! # Why XOAUTH2 only for Microsoft
//!
//! IMAP signs in with an app password everywhere except where the provider
//! no longer accepts one. Microsoft turned basic authentication off for
//! Exchange Online, so an IMAP account there needs an OAuth token through
//! `AUTHENTICATE XOAUTH2`, and the device-code flow this crate already runs
//! for Graph ([`oauth::DeviceCodeFlow`]) mints it under the same client id.
//! Other providers either still take app passwords (iCloud, Fastmail, most
//! hosts) or need their own OAuth client registration and consent screen
//! (Google), which the base version does not ship: adding one is a client
//! registration and a scope list, not a change to the IMAP code.

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
