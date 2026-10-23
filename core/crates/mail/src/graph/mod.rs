//! Microsoft 365 mail through Microsoft Graph. A folder's cursor is the
//! `@odata.deltaLink` of its message delta query; the first fetch filters
//! the delta to messages received since the first-fetch window. Every URL
//! the source follows (a stored cursor, a `nextLink`) must lie under the
//! Graph base it was opened with, so a cursor synced from elsewhere can
//! never send the access token to another host.

use std::fmt;
use std::sync::Arc;

use jiff::Timestamp;
use serde::Deserialize;

use crate::error::MailError;
use crate::fetch::FetchLimits;
use crate::http::{HttpRequest, HttpResponse, HttpTransport};
use crate::link::openable_link;
use crate::mime;
use crate::oauth::RefreshingTokens;
use crate::source::{FetchPage, FetchedMessage, FolderRole, MailFolder, MailSource};

/// Graph's v1.0 endpoint.
pub const GRAPH_BASE_URL: &str = "https://graph.microsoft.com/v1.0";
const MESSAGE_FIELDS: &str =
    "id,internetMessageId,conversationId,subject,from,receivedDateTime,bodyPreview,webLink";
/// The well-known folders the source offers, with their roles.
const WELL_KNOWN: [(&str, &str, FolderRole); 2] = [
    ("inbox", "Inbox", FolderRole::Inbox),
    ("sentitems", "Sent Items", FolderRole::Sent),
];
/// The most delta pages one fetch follows.
const MAX_PAGES: usize = 40;

/// A Microsoft 365 mailbox.
pub struct GraphSource {
    http: Arc<dyn HttpTransport>,
    base: String,
    tokens: RefreshingTokens,
    limits: FetchLimits,
}

#[derive(Deserialize)]
struct Page<T> {
    #[serde(default = "Vec::new")]
    value: Vec<T>,
    #[serde(rename = "@odata.nextLink", default)]
    next_link: Option<String>,
    #[serde(rename = "@odata.deltaLink", default)]
    delta_link: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Folder {
    #[serde(default)]
    display_name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Message {
    id: String,
    #[serde(default)]
    internet_message_id: Option<String>,
    #[serde(default)]
    conversation_id: Option<String>,
    #[serde(default)]
    subject: Option<String>,
    #[serde(default)]
    from: Option<Recipient>,
    #[serde(default)]
    received_date_time: Option<String>,
    #[serde(default)]
    body_preview: Option<String>,
    #[serde(default)]
    web_link: Option<String>,
    #[serde(rename = "@removed", default)]
    removed: Option<serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Recipient {
    email_address: EmailAddress,
}

#[derive(Deserialize)]
struct EmailAddress {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    address: Option<String>,
}

#[derive(Deserialize)]
struct Body {
    body: BodyContent,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BodyContent {
    #[serde(default)]
    content_type: String,
    #[serde(default)]
    content: String,
}

impl GraphSource {
    /// A source over `base` (normally [`GRAPH_BASE_URL`]).
    pub fn new(
        http: Arc<dyn HttpTransport>,
        base: String,
        tokens: RefreshingTokens,
        limits: FetchLimits,
    ) -> GraphSource {
        GraphSource {
            http,
            base: base.trim_end_matches('/').to_owned(),
            tokens,
            limits,
        }
    }

    /// A signed-in `GET` of `url`; a refused token is refreshed once.
    fn get(&mut self, url: &str, prefer: Option<&str>) -> Result<HttpResponse, MailError> {
        if !self.is_under_base(url) {
            return Err(MailError::Protocol(format!(
                "refusing to follow a link outside {}",
                self.base
            )));
        }
        let mut is_retry = false;
        loop {
            let token = self.tokens.access_token()?;
            let mut request =
                HttpRequest::get(url).header("Authorization", format!("Bearer {token}"));
            if let Some(prefer) = prefer {
                request = request.header("Prefer", prefer);
            }
            let response = self.http.send(&request)?;
            match response.status {
                200..=299 => return Ok(response),
                401 if !is_retry => {
                    self.tokens.invalidate();
                    is_retry = true;
                }
                401 | 403 => {
                    return Err(MailError::Auth(
                        "Microsoft 365 refused the sign-in; sign in to the account again"
                            .to_owned(),
                    ));
                }
                404 => return Err(MailError::FolderNotFound(url.to_owned())),
                429 | 500..=599 => {
                    return Err(MailError::Network {
                        message: format!("Microsoft Graph answered {}", response.status),
                        retry_after: response.retry_after,
                    });
                }
                status => {
                    return Err(MailError::Protocol(format!(
                        "Microsoft Graph answered {status}"
                    )));
                }
            }
        }
    }

    fn get_json<T: for<'de> Deserialize<'de>>(
        &mut self,
        url: &str,
        prefer: Option<&str>,
    ) -> Result<T, MailError> {
        let response = self.get(url, prefer)?;
        serde_json::from_str(&response.body)
            .map_err(|error| MailError::Protocol(format!("Microsoft Graph sent {error}")))
    }

    fn is_under_base(&self, url: &str) -> bool {
        url.strip_prefix(&self.base)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/') || rest.starts_with('?'))
    }

    fn body_text(&mut self, message_id: &str, max_bytes: usize) -> Result<String, MailError> {
        let url = format!(
            "{}/me/messages/{}?$select=body",
            self.base,
            path_segment(message_id)
        );
        let body: Body = self.get_json(&url, Some("outlook.body-content-type=\"text\""))?;
        let text = if body.body.content_type.eq_ignore_ascii_case("html") {
            mime::html_to_text(&body.body.content)
        } else {
            body.body.content
        };
        Ok(truncate(text, max_bytes))
    }

    fn message(&mut self, message: Message) -> Result<FetchedMessage, MailError> {
        let raw_text = self.body_text(&message.id, self.limits.max_raw_bytes)?;
        let message_id = message
            .internet_message_id
            .clone()
            .unwrap_or_else(|| message.id.clone());
        let (sender_name, sender_address) = message
            .from
            .map(|from| (from.email_address.name, from.email_address.address))
            .unwrap_or_default();
        let excerpt = match message.body_preview {
            Some(preview) if !preview.trim().is_empty() => mime::excerpt(&preview),
            _ => mime::excerpt(&raw_text),
        };
        Ok(FetchedMessage {
            link: self.message_link(&message_id, message.web_link.as_deref()),
            thread_id: message
                .conversation_id
                .unwrap_or_else(|| message_id.clone()),
            provider_ref: message.id,
            message_id,
            sender_name: sender_name.filter(|name| !name.trim().is_empty()),
            sender_address: sender_address.unwrap_or_default(),
            received_at: message
                .received_date_time
                .and_then(|at| at.parse().ok())
                .unwrap_or(Timestamp::UNIX_EPOCH),
            subject: message.subject.unwrap_or_default(),
            excerpt,
            raw_text,
        })
    }
}

impl fmt::Debug for GraphSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GraphSource")
            .field("base", &self.base)
            .field("tokens", &self.tokens)
            .finish_non_exhaustive()
    }
}

impl MailSource for GraphSource {
    fn list_folders(&mut self) -> Result<Vec<MailFolder>, MailError> {
        let mut folders = Vec::new();
        for (well_known, name, role) in WELL_KNOWN {
            // Graph accepts the well-known name wherever a folder id goes.
            let url = format!(
                "{}/me/mailFolders/{well_known}?$select=displayName",
                self.base
            );
            let folder: Folder = self.get_json(&url, None)?;
            folders.push(MailFolder {
                id: well_known.to_owned(),
                name: if folder.display_name.is_empty() {
                    name.to_owned()
                } else {
                    folder.display_name
                },
                role,
            });
        }
        Ok(folders)
    }

    fn fetch_new(
        &mut self,
        folder: &str,
        cursor: Option<&str>,
        since: Timestamp,
    ) -> Result<FetchPage, MailError> {
        let first = format!(
            "{}/me/mailFolders/{}/messages/delta?$select={MESSAGE_FIELDS}&$filter=receivedDateTime+ge+{}",
            self.base,
            path_segment(folder),
            since.strftime("%Y-%m-%dT%H:%M:%SZ")
        );
        let mut url = cursor
            .filter(|cursor| self.is_under_base(cursor))
            .map_or(first, str::to_owned);
        let max_messages = self.limits.max_messages.max(1);
        let mut collected: Vec<Message> = Vec::new();
        for _ in 0..MAX_PAGES {
            // Each page asks for no more than the fetch may still bring, so
            // nothing is ever cut from a page; a full fetch stops at the
            // `nextLink`, which is a cursor the next fetch resumes from.
            let room = max_messages.saturating_sub(collected.len());
            let prefer = format!("odata.maxpagesize={room}");
            let page: Page<Message> = self.get_json(&url, Some(&prefer))?;
            collected.extend(
                page.value
                    .into_iter()
                    .filter(|message| message.removed.is_none()),
            );
            match (page.next_link, page.delta_link) {
                (Some(next), _) => {
                    url = next;
                    if collected.len() >= max_messages {
                        break;
                    }
                }
                (None, Some(delta)) => {
                    url = delta;
                    break;
                }
                (None, None) => {
                    return Err(MailError::Protocol(
                        "a delta page holds neither a next nor a delta link".to_owned(),
                    ));
                }
            }
        }
        collected.sort_by(|a, b| a.received_date_time.cmp(&b.received_date_time));
        let mut messages = Vec::with_capacity(collected.len());
        for message in collected {
            messages.push(self.message(message)?);
        }
        Ok(FetchPage {
            messages,
            cursor: url,
        })
    }

    fn fetch_body(&mut self, _folder: &str, provider_ref: &str) -> Result<String, MailError> {
        self.body_text(provider_ref, 1024 * 1024)
    }

    fn message_link(&self, message_id: &str, web_link: Option<&str>) -> String {
        web_link.and_then(openable_link).map_or_else(
            || {
                format!(
                    "https://outlook.office.com/mail/search?q={}",
                    query_value(message_id)
                )
            },
            str::to_owned,
        )
    }
}

/// Graph ids are URL-safe base64 except for `/`, `+` and `=`.
fn path_segment(id: &str) -> String {
    id.replace('/', "%2F")
        .replace('+', "%2B")
        .replace('=', "%3D")
}

fn query_value(text: &str) -> String {
    text.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'@' => {
                char::from(byte).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

fn truncate(mut text: String, max: usize) -> String {
    if text.len() > max {
        let mut end = max;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
    }
    text
}
