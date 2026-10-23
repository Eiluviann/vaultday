//! IMAP over implicit TLS. Each call opens a connection, signs in, does its
//! work and logs out; folders are `EXAMINE`d (read-only). The cursor of a
//! folder is `<UIDVALIDITY>:<highest UID seen>`; when the server's
//! UIDVALIDITY changes the cursor is dropped and the folder fetched afresh,
//! and duplicates from that are suppressed by the signal store.

mod protocol;

use std::fmt;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

use app_state::SecretBytes;
use jiff::Timestamp;
use jiff::tz::TimeZone;
use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};

use self::protocol::{Client, Item, Mailbox};
use crate::error::MailError;
use crate::fetch::FetchLimits;
use crate::link::imap_message_link;
use crate::mime;
use crate::oauth::RefreshingTokens;
use crate::source::{FetchPage, FetchedMessage, FolderRole, MailFolder, MailSource};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const IO_TIMEOUT: Duration = Duration::from_secs(60);
/// Names that mean sent mail on servers without the `\Sent` attribute.
const SENT_NAMES: [&str; 4] = ["sent", "sent items", "sent messages", "sent mail"];
const HEADER_ITEMS: &str =
    "(UID INTERNALDATE ENVELOPE BODYSTRUCTURE BODY.PEEK[HEADER.FIELDS (REFERENCES)])";
/// The most of a body [`MailSource::fetch_body`] reads.
const MAX_BODY_BYTES: usize = 1024 * 1024;

/// A connection the protocol reads and writes.
pub trait ImapStream: Read + Write + Send {}

impl<T: Read + Write + Send> ImapStream for T {}

/// Opens connections to IMAP servers.
pub trait ImapConnector: Send + Sync {
    /// A connection to `host:port`, ready for the greeting.
    ///
    /// # Errors
    /// [`MailError::Network`] when the host cannot be reached.
    fn connect(&self, host: &str, port: u16) -> Result<Box<dyn ImapStream>, MailError>;
}

/// Implicit TLS (port 993) with rustls and the Mozilla root store. There is
/// no plaintext connector outside the tests.
pub struct TlsConnector {
    config: Result<Arc<ClientConfig>, String>,
}

/// How the source signs in.
pub enum ImapAuthSource {
    /// `LOGIN` with an app password.
    Password(SecretBytes),
    /// `AUTHENTICATE XOAUTH2` with an access token from a refresh token.
    XOAuth2(RefreshingTokens),
}

/// An IMAP mailbox.
pub struct ImapSource {
    connector: Arc<dyn ImapConnector>,
    host: String,
    port: u16,
    user: String,
    auth: ImapAuthSource,
    limits: FetchLimits,
}

/// The parts of one `FETCH` response the source uses.
struct Header {
    uid: u32,
    internal_date: Option<Timestamp>,
    envelope_date: Option<Timestamp>,
    subject: String,
    sender_name: Option<String>,
    sender_address: String,
    in_reply_to: Option<String>,
    message_id: Option<String>,
    references: Vec<String>,
    text_part: Option<TextPart>,
}

/// Where the readable text of a message is.
#[derive(Debug, Clone, PartialEq, Eq)]
struct TextPart {
    section: String,
    encoding: String,
    charset: String,
    is_html: bool,
}

impl TlsConnector {
    /// A connector over the Mozilla roots with the `ring` provider.
    #[must_use]
    pub fn new() -> TlsConnector {
        let roots = RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
        };
        let config =
            ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_safe_default_protocol_versions()
                .map(|builder| {
                    Arc::new(builder.with_root_certificates(roots).with_no_client_auth())
                })
                .map_err(|error| error.to_string());
        TlsConnector { config }
    }
}

impl Default for TlsConnector {
    fn default() -> Self {
        TlsConnector::new()
    }
}

impl fmt::Debug for TlsConnector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TlsConnector")
            .field("is_ready", &self.config.is_ok())
            .finish()
    }
}

impl ImapConnector for TlsConnector {
    fn connect(&self, host: &str, port: u16) -> Result<Box<dyn ImapStream>, MailError> {
        let config = self
            .config
            .clone()
            .map_err(|error| MailError::Protocol(format!("TLS is unavailable: {error}")))?;
        let name = ServerName::try_from(host.to_owned())
            .map_err(|_| MailError::Protocol(format!("{host:?} is not a server name")))?;
        let addresses = (host, port)
            .to_socket_addrs()
            .map_err(|error| MailError::network(format!("{host} cannot be found: {error}")))?;
        let mut last_error = None;
        for address in addresses {
            match TcpStream::connect_timeout(&address, CONNECT_TIMEOUT) {
                Ok(tcp) => {
                    tcp.set_read_timeout(Some(IO_TIMEOUT))
                        .and_then(|()| tcp.set_write_timeout(Some(IO_TIMEOUT)))
                        .map_err(|error| MailError::network(error.to_string()))?;
                    let connection = ClientConnection::new(config, name)
                        .map_err(|error| MailError::Protocol(format!("TLS failed: {error}")))?;
                    return Ok(Box::new(StreamOwned::new(connection, tcp)));
                }
                Err(error) => last_error = Some(error),
            }
        }
        Err(MailError::network(match last_error {
            Some(error) => format!("{host}:{port} cannot be reached: {error}"),
            None => format!("{host} has no address"),
        }))
    }
}

impl fmt::Debug for ImapAuthSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ImapAuthSource::Password(_) => f.write_str("Password(…)"),
            ImapAuthSource::XOAuth2(tokens) => f.debug_tuple("XOAuth2").field(tokens).finish(),
        }
    }
}

impl ImapSource {
    /// A source for `user` at `host:port`.
    pub fn new(
        connector: Arc<dyn ImapConnector>,
        host: String,
        port: u16,
        user: String,
        auth: ImapAuthSource,
        limits: FetchLimits,
    ) -> ImapSource {
        ImapSource {
            connector,
            host,
            port,
            user,
            auth,
            limits,
        }
    }

    /// Connects and signs in.
    fn session(&mut self) -> Result<Client, MailError> {
        let stream = self.connector.connect(&self.host, self.port)?;
        let mut client = Client::greet(stream)?;
        match &mut self.auth {
            ImapAuthSource::Password(password) => client.login(&self.user, password.expose())?,
            ImapAuthSource::XOAuth2(tokens) => {
                let token = tokens.access_token()?;
                if let Err(error) = client.authenticate_xoauth2(&self.user, &token) {
                    tokens.invalidate();
                    return Err(error);
                }
            }
        }
        Ok(client)
    }

    /// Runs `work` in a signed-in session and logs out whatever it returned.
    fn with_session<T>(
        &mut self,
        work: impl FnOnce(&mut Client, &FetchLimits) -> Result<T, MailError>,
    ) -> Result<T, MailError> {
        let mut client = self.session()?;
        let outcome = work(&mut client, &self.limits);
        let _ = client.logout();
        outcome
    }
}

impl fmt::Debug for ImapSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImapSource")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("user", &self.user)
            .field("auth", &self.auth)
            .finish_non_exhaustive()
    }
}

impl MailSource for ImapSource {
    fn list_folders(&mut self) -> Result<Vec<MailFolder>, MailError> {
        self.with_session(|client, _| {
            let listed = client.list()?;
            Ok(listed
                .into_iter()
                .filter(|folder| {
                    !folder
                        .attributes
                        .iter()
                        .any(|attribute| attribute.eq_ignore_ascii_case("\\Noselect"))
                })
                .map(|folder| {
                    let role = folder_role(&folder.name, &folder.attributes);
                    let name = match role {
                        FolderRole::Inbox => "Inbox".to_owned(),
                        _ => folder.name.clone(),
                    };
                    MailFolder {
                        id: folder.name,
                        name,
                        role,
                    }
                })
                .collect())
        })
    }

    fn fetch_new(
        &mut self,
        folder: &str,
        cursor: Option<&str>,
        since: Timestamp,
    ) -> Result<FetchPage, MailError> {
        self.with_session(|client, limits| {
            let mailbox = client.examine(folder)?;
            let last_seen = cursor
                .and_then(parse_cursor)
                .filter(|(validity, _)| *validity == mailbox.uid_validity)
                .map(|(_, uid)| uid);
            let mut uids = match last_seen {
                Some(last) => client
                    .uid_search(&format!("UID {}:*", last.saturating_add(1)))?
                    .into_iter()
                    .filter(|uid| *uid > last)
                    .collect(),
                None => client.uid_search(&format!("SINCE {}", imap_date(since)))?,
            };
            uids.sort_unstable();
            uids.dedup();
            // The oldest first: the cursor stops at the last UID fetched, so
            // what the limit leaves out comes with the next fetch.
            let kept: Vec<u32> = uids
                .iter()
                .take(limits.max_messages.max(1))
                .copied()
                .collect();
            let highest = kept
                .last()
                .copied()
                .or(last_seen)
                .unwrap_or_else(|| mailbox.uid_next.saturating_sub(1));
            let messages = if kept.is_empty() {
                Vec::new()
            } else {
                fetch_messages(client, &mailbox, &kept, limits)?
            };
            Ok(FetchPage {
                messages,
                cursor: format!("{}:{highest}", mailbox.uid_validity),
            })
        })
    }

    fn fetch_body(&mut self, folder: &str, provider_ref: &str) -> Result<String, MailError> {
        let (validity, uid) = parse_cursor(provider_ref).ok_or_else(|| {
            MailError::Protocol(format!("{provider_ref:?} is not an IMAP reference"))
        })?;
        self.with_session(|client, _| {
            let mailbox = client.examine(folder)?;
            if mailbox.uid_validity != validity {
                return Err(MailError::Protocol(format!(
                    "{folder} was renumbered; the message must be fetched again"
                )));
            }
            let fetched = client.uid_fetch(&uid.to_string(), "(UID BODYSTRUCTURE)")?;
            let Some(part) = fetched
                .first()
                .and_then(|attributes| attribute(attributes, "BODYSTRUCTURE"))
                .and_then(text_part)
            else {
                return Ok(String::new());
            };
            read_text(client, uid, &part, MAX_BODY_BYTES)
        })
    }

    fn message_link(&self, message_id: &str, _web_link: Option<&str>) -> String {
        imap_message_link(message_id)
    }
}

fn fetch_messages(
    client: &mut Client,
    mailbox: &Mailbox,
    uids: &[u32],
    limits: &FetchLimits,
) -> Result<Vec<FetchedMessage>, MailError> {
    let set: Vec<String> = uids.iter().map(ToString::to_string).collect();
    let mut headers: Vec<Header> = client
        .uid_fetch(&set.join(","), HEADER_ITEMS)?
        .iter()
        .filter_map(|attributes| header(attributes))
        .collect();
    headers.sort_by_key(|header| header.uid);
    let mut messages = Vec::with_capacity(headers.len());
    for header in headers {
        let raw_text = match &header.text_part {
            Some(part) => read_text(client, header.uid, part, limits.max_raw_bytes)?,
            None => String::new(),
        };
        let provider_ref = format!("{}:{}", mailbox.uid_validity, header.uid);
        let message_id = header
            .message_id
            .clone()
            .unwrap_or_else(|| provider_ref.clone());
        let thread_id = header
            .references
            .first()
            .cloned()
            .or_else(|| header.in_reply_to.clone())
            .unwrap_or_else(|| message_id.clone());
        let received_at = header
            .internal_date
            .or(header.envelope_date)
            .unwrap_or(Timestamp::UNIX_EPOCH);
        messages.push(FetchedMessage {
            link: imap_message_link(&message_id),
            provider_ref,
            message_id,
            thread_id,
            sender_name: header.sender_name,
            sender_address: header.sender_address,
            received_at,
            subject: header.subject,
            excerpt: mime::excerpt(&raw_text),
            raw_text,
        });
    }
    Ok(messages)
}

fn read_text(
    client: &mut Client,
    uid: u32,
    part: &TextPart,
    max_bytes: usize,
) -> Result<String, MailError> {
    let items = format!("(UID BODY.PEEK[{}]<0.{max_bytes}>)", part.section);
    let fetched = client.uid_fetch(&uid.to_string(), &items)?;
    let key = format!("BODY[{}]", part.section);
    let raw = fetched
        .first()
        .and_then(|attributes| attribute(attributes, &key))
        .and_then(Item::bytes)
        .unwrap_or_default();
    let text = mime::decode_body(raw, &part.encoding, &part.charset);
    Ok(if part.is_html {
        mime::html_to_text(&text)
    } else {
        text
    })
}

fn header(attributes: &[Item]) -> Option<Header> {
    let uid: u32 = attribute(attributes, "UID")?.text()?.parse().ok()?;
    let envelope = attribute(attributes, "ENVELOPE")?.list()?;
    let field = |index: usize| envelope.get(index);
    let (sender_name, sender_address) = field(2)
        .and_then(Item::list)
        .and_then(|addresses| addresses.first())
        .and_then(Item::list)
        .map(address)
        .unwrap_or_default();
    let references = attribute(attributes, "BODY[HEADER.FIELDS")
        .and_then(Item::bytes)
        .map(message_ids)
        .unwrap_or_default();
    Some(Header {
        uid,
        internal_date: attribute(attributes, "INTERNALDATE")
            .and_then(Item::text)
            .and_then(|date| parse_internal_date(&date)),
        envelope_date: field(0)
            .and_then(Item::text)
            .and_then(|date| parse_rfc2822(&date)),
        subject: field(1)
            .and_then(Item::bytes)
            .map(mime::decode_header)
            .unwrap_or_default(),
        sender_name,
        sender_address,
        in_reply_to: field(8)
            .and_then(Item::bytes)
            .and_then(|bytes| message_ids(bytes).into_iter().next()),
        message_id: field(9)
            .and_then(Item::bytes)
            .and_then(|bytes| message_ids(bytes).into_iter().next()),
        references,
        text_part: attribute(attributes, "BODYSTRUCTURE").and_then(text_part),
    })
}

/// The value after the attribute named `name` (or, for a name ending in
/// `[…`, starting with it), its `<origin>` ignored.
fn attribute<'a>(attributes: &'a [Item], name: &str) -> Option<&'a Item> {
    attributes.chunks(2).find_map(|pair| match pair {
        [Item::Atom(key), value] => {
            let key = key.split('<').next().unwrap_or_default();
            let is_match = if name.ends_with(']') || !name.contains('[') {
                key.eq_ignore_ascii_case(name)
            } else {
                key.to_ascii_uppercase().starts_with(name)
            };
            is_match.then_some(value)
        }
        _ => None,
    })
}

/// `(name adl mailbox host)` as a display name and an address.
fn address(parts: &[Item]) -> (Option<String>, String) {
    let name = parts
        .first()
        .and_then(Item::bytes)
        .map(mime::decode_header)
        .filter(|name| !name.trim().is_empty());
    let mailbox = parts.get(2).and_then(Item::text).unwrap_or_default();
    let host = parts.get(3).and_then(Item::text).unwrap_or_default();
    let address = if host.is_empty() {
        mailbox
    } else {
        format!("{mailbox}@{host}")
    };
    (name, address)
}

/// Every `<…>` message id in a header value, in order.
fn message_ids(bytes: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(bytes);
    let mut ids = Vec::new();
    let mut rest = text.as_ref();
    while let Some(start) = rest.find('<') {
        let Some(end) = rest[start..].find('>') else {
            break;
        };
        ids.push(rest[start..=start + end].to_owned());
        rest = &rest[start + end + 1..];
    }
    ids
}

/// The section of the first `text/plain` part, else the first `text/html`
/// one, from a `BODYSTRUCTURE`.
fn text_part(structure: &Item) -> Option<TextPart> {
    let parts = structure.list()?;
    find_text(parts, "", "PLAIN").or_else(|| find_text(parts, "", "HTML"))
}

fn find_text(parts: &[Item], prefix: &str, subtype: &str) -> Option<TextPart> {
    let is_multipart = matches!(parts.first(), Some(Item::List(_)));
    if !is_multipart {
        let media_type = parts.first()?.text()?;
        let media_subtype = parts.get(1)?.text()?;
        if !media_type.eq_ignore_ascii_case("TEXT") || !media_subtype.eq_ignore_ascii_case(subtype)
        {
            return None;
        }
        let charset = parts
            .get(2)
            .and_then(Item::list)
            .and_then(|parameters| {
                parameters.chunks(2).find_map(|pair| match pair {
                    [key, value] if key.text()?.eq_ignore_ascii_case("CHARSET") => value.text(),
                    _ => None,
                })
            })
            .unwrap_or_else(|| "us-ascii".to_owned());
        let section = if prefix.is_empty() {
            "1".to_owned()
        } else {
            prefix.to_owned()
        };
        return Some(TextPart {
            section,
            encoding: parts.get(5).and_then(Item::text).unwrap_or_default(),
            charset,
            is_html: subtype == "HTML",
        });
    }
    parts
        .iter()
        .take_while(|part| matches!(part, Item::List(_)))
        .enumerate()
        .find_map(|(index, part)| {
            let section = if prefix.is_empty() {
                (index + 1).to_string()
            } else {
                format!("{prefix}.{}", index + 1)
            };
            find_text(part.list()?, &section, subtype)
        })
}

fn folder_role(name: &str, attributes: &[String]) -> FolderRole {
    if name.eq_ignore_ascii_case("INBOX") {
        return FolderRole::Inbox;
    }
    if attributes
        .iter()
        .any(|attribute| attribute.eq_ignore_ascii_case("\\Sent"))
    {
        return FolderRole::Sent;
    }
    let leaf = name.rsplit(['/', '.']).next().unwrap_or(name);
    if SENT_NAMES
        .iter()
        .any(|sent| leaf.eq_ignore_ascii_case(sent))
    {
        return FolderRole::Sent;
    }
    FolderRole::Other
}

fn parse_cursor(cursor: &str) -> Option<(u32, u32)> {
    let (validity, uid) = cursor.split_once(':')?;
    Some((validity.parse().ok()?, uid.parse().ok()?))
}

/// `16-Oct-2026`, the date form `SEARCH SINCE` takes.
fn imap_date(at: Timestamp) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let date = at.to_zoned(TimeZone::UTC).date();
    let month = usize::try_from(date.month() - 1)
        .ok()
        .and_then(|index| MONTHS.get(index))
        .copied()
        .unwrap_or("Jan");
    format!("{}-{month}-{}", date.day(), date.year())
}

/// `INTERNALDATE`: `22-Oct-2026 09:00:00 +0200`.
fn parse_internal_date(text: &str) -> Option<Timestamp> {
    jiff::fmt::strtime::parse("%d-%b-%Y %H:%M:%S %z", text.trim())
        .ok()?
        .to_timestamp()
        .ok()
}

/// An envelope date in RFC 2822 form.
fn parse_rfc2822(text: &str) -> Option<Timestamp> {
    jiff::fmt::rfc2822::DateTimeParser::new()
        .parse_timestamp(text.trim())
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn structure(text: &[u8]) -> Item {
        protocol::parse(text).unwrap().remove(0)
    }

    #[test]
    fn the_text_part_is_found_in_single_and_nested_structures() {
        let single = structure(
            b"(\"TEXT\" \"PLAIN\" (\"CHARSET\" \"utf-8\") NIL NIL \"QUOTED-PRINTABLE\" 120 4)",
        );
        assert_eq!(
            text_part(&single),
            Some(TextPart {
                section: "1".to_owned(),
                encoding: "QUOTED-PRINTABLE".to_owned(),
                charset: "utf-8".to_owned(),
                is_html: false,
            })
        );
        let nested = structure(
            b"(((\"TEXT\" \"PLAIN\" (\"CHARSET\" \"iso-8859-1\") NIL NIL \"BASE64\" 10 1)(\"TEXT\" \"HTML\" NIL NIL NIL \"7BIT\" 20 1) \"ALTERNATIVE\")(\"APPLICATION\" \"PDF\" NIL NIL NIL \"BASE64\" 99) \"MIXED\")",
        );
        let part = text_part(&nested).unwrap();
        assert_eq!(part.section, "1.1");
        assert_eq!(part.charset, "iso-8859-1");
        let html_only = structure(
            b"((\"IMAGE\" \"PNG\" NIL NIL NIL \"BASE64\" 9)(\"TEXT\" \"HTML\" NIL NIL NIL \"7BIT\" 20 1) \"MIXED\")",
        );
        let part = text_part(&html_only).unwrap();
        assert_eq!((part.section.as_str(), part.is_html), ("2", true));
    }

    #[test]
    fn folder_roles_follow_names_and_attributes() {
        assert_eq!(folder_role("INBOX", &[]), FolderRole::Inbox);
        assert_eq!(
            folder_role("Gesendet", &["\\Sent".to_owned()]),
            FolderRole::Sent
        );
        assert_eq!(folder_role("[Gmail]/Sent Mail", &[]), FolderRole::Sent);
        assert_eq!(folder_role("Archive", &[]), FolderRole::Other);
    }

    #[test]
    fn dates_parse_and_format() {
        let at: Timestamp = "2026-10-22T07:00:00Z".parse().unwrap();
        assert_eq!(parse_internal_date("22-Oct-2026 09:00:00 +0200"), Some(at));
        assert_eq!(parse_rfc2822("Thu, 22 Oct 2026 09:00:00 +0200"), Some(at));
        assert_eq!(imap_date(at), "22-Oct-2026");
        assert_eq!(parse_cursor("7:42"), Some((7, 42)));
        assert_eq!(parse_cursor("seven"), None);
    }

    #[test]
    fn message_ids_are_read_in_order() {
        assert_eq!(
            message_ids(b"References: <a@x.test>\r\n <b@x.test>"),
            ["<a@x.test>", "<b@x.test>"]
        );
    }
}
