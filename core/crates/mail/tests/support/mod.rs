//! In-process servers for the mail tests: a fake IMAP server speaking the
//! subset the source uses, a mock HTTP server for Graph and the OAuth
//! endpoints, and an encrypted workspace document in a temporary folder.
//! Every message is synthetic; nothing here reaches a real mail server.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use app_state::{
    DataKey, DeviceId, InMemorySecretStore, PlainSyncFiles, SyncConfig, WorkspaceSync,
    create_workspace_key, workspace_key,
};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use domain::workspace::WorkspaceId;
use mail::RetryPolicy;
use mail::error::MailError;
use mail::imap::{ImapConnector, ImapStream};

// The fake IMAP server.

/// One synthetic message.
#[derive(Debug, Clone)]
pub struct FakeMessage {
    pub uid: u32,
    pub message_id: String,
    pub references: Option<String>,
    pub from_name: String,
    pub from_address: String,
    pub subject: String,
    pub internal_date: String,
    /// The body as sent on the wire, with its transfer encoding applied.
    pub body: String,
    pub encoding: String,
}

/// One mailbox.
#[derive(Debug, Clone)]
pub struct FakeFolder {
    pub name: String,
    pub attributes: String,
    pub uid_validity: u32,
    pub messages: Vec<FakeMessage>,
}

/// What the server holds and how it misbehaves.
#[derive(Debug, Default)]
pub struct ImapState {
    pub user: String,
    pub password: String,
    pub xoauth2_token: Option<String>,
    pub folders: Vec<FakeFolder>,
    /// Connections to close before the greeting, one per count.
    pub drops_left: usize,
    /// Every command received, without its tag.
    pub log: Vec<String>,
}

/// A fake IMAP server on a loopback port.
pub struct FakeImap {
    pub state: Arc<Mutex<ImapState>>,
    pub address: SocketAddr,
    pub connections: Arc<AtomicUsize>,
}

/// Connects to the fake server in plaintext, whatever host it is given.
pub struct PlainConnector {
    pub address: SocketAddr,
}

impl ImapConnector for PlainConnector {
    fn connect(&self, _host: &str, _port: u16) -> Result<Box<dyn ImapStream>, MailError> {
        let stream = TcpStream::connect(self.address).map_err(|error| MailError::Network {
            message: error.to_string(),
            retry_after: None,
        })?;
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        Ok(Box::new(stream))
    }
}

impl FakeImap {
    pub fn start(state: ImapState) -> FakeImap {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let state = Arc::new(Mutex::new(state));
        let connections = Arc::new(AtomicUsize::new(0));
        let shared = state.clone();
        let counter = connections.clone();
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { return };
                counter.fetch_add(1, Ordering::SeqCst);
                let state = shared.clone();
                thread::spawn(move || serve_imap(stream, &state));
            }
        });
        FakeImap {
            state,
            address,
            connections,
        }
    }

    pub fn connector(&self) -> Arc<dyn ImapConnector> {
        Arc::new(PlainConnector {
            address: self.address,
        })
    }

    pub fn log(&self) -> Vec<String> {
        self.state.lock().unwrap().log.clone()
    }

    pub fn add_message(&self, folder: &str, message: FakeMessage) {
        let mut state = self.state.lock().unwrap();
        let folder = state.folders.iter_mut().find(|f| f.name == folder).unwrap();
        folder.messages.push(message);
    }
}

pub fn synthetic_message(uid: u32, subject: &str) -> FakeMessage {
    FakeMessage {
        uid,
        message_id: format!("<synthetic-{uid}@mail.example.test>"),
        references: None,
        from_name: "Synthetic Sender".to_owned(),
        from_address: "sender@example.test".to_owned(),
        subject: subject.to_owned(),
        internal_date: "22-Oct-2026 09:00:00 +0200".to_owned(),
        body: format!("Hello,\r\nThis is synthetic message {uid}.\r\n> quoted\r\n"),
        encoding: "7BIT".to_owned(),
    }
}

pub fn mailbox() -> ImapState {
    let mut sent = synthetic_message(1, "Waiting on the synthetic answer");
    sent.message_id = "<synthetic-sent-1@mail.example.test>".to_owned();
    let mut reply = synthetic_message(2, "=?UTF-8?Q?Re:_Caf=C3=A9_plan?=");
    reply.references = Some("<synthetic-1@mail.example.test>".to_owned());
    reply.body = "Caf=C3=A9 is booked for the synthetic review=\r\n.".to_owned();
    reply.encoding = "QUOTED-PRINTABLE".to_owned();
    ImapState {
        user: "someone@example.test".to_owned(),
        password: "synthetic-app-password".to_owned(),
        xoauth2_token: None,
        folders: vec![
            FakeFolder {
                name: "INBOX".to_owned(),
                attributes: "\\HasNoChildren".to_owned(),
                uid_validity: 7,
                messages: vec![synthetic_message(1, "Synthetic plan"), reply],
            },
            FakeFolder {
                name: "Sent".to_owned(),
                attributes: "\\HasNoChildren \\Sent".to_owned(),
                uid_validity: 9,
                messages: vec![sent],
            },
            FakeFolder {
                name: "Archive".to_owned(),
                attributes: "\\HasNoChildren".to_owned(),
                uid_validity: 11,
                messages: vec![synthetic_message(1, "Archived synthetic note")],
            },
        ],
        ..ImapState::default()
    }
}

fn serve_imap(stream: TcpStream, state: &Mutex<ImapState>) {
    {
        let mut state = state.lock().unwrap();
        if state.drops_left > 0 {
            state.drops_left -= 1;
            return;
        }
    }
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut out = stream;
    let _ = out.write_all(b"* OK [CAPABILITY IMAP4rev1 AUTH=XOAUTH2] Synthetic IMAP ready\r\n");
    let mut selected: Option<String> = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            return;
        }
        let line = line.trim_end().to_owned();
        let Some((tag, command)) = line.split_once(' ') else {
            continue;
        };
        state.lock().unwrap().log.push(command.to_owned());
        let upper = command.to_ascii_uppercase();
        let reply = if upper.starts_with("LOGIN ") {
            let args = quoted_args(&command[6..]);
            let state = state.lock().unwrap();
            let is_valid = args.len() == 2 && args[0] == state.user && args[1] == state.password;
            if is_valid {
                format!("{tag} OK LOGIN completed\r\n")
            } else {
                format!("{tag} NO [AUTHENTICATIONFAILED] Invalid credentials\r\n")
            }
        } else if upper.starts_with("AUTHENTICATE XOAUTH2 ") {
            let payload = STANDARD.decode(command[21..].trim()).unwrap_or_default();
            let payload = String::from_utf8_lossy(&payload).into_owned();
            let expected = state.lock().unwrap().xoauth2_token.clone();
            let is_valid = expected
                .is_some_and(|token| payload.contains(&format!("auth=Bearer {token}\u{1}")));
            if is_valid {
                format!("{tag} OK AUTHENTICATE completed\r\n")
            } else {
                let _ = out.write_all(b"+ eyJzdGF0dXMiOiI0MDAifQ==\r\n");
                let mut empty = String::new();
                let _ = reader.read_line(&mut empty);
                format!("{tag} NO [AUTHENTICATIONFAILED] Invalid token\r\n")
            }
        } else if upper.starts_with("LIST ") {
            let state = state.lock().unwrap();
            let mut reply = String::new();
            for folder in &state.folders {
                reply.push_str(&format!(
                    "* LIST ({}) \"/\" \"{}\"\r\n",
                    folder.attributes, folder.name
                ));
            }
            reply.push_str(&format!("{tag} OK LIST completed\r\n"));
            reply
        } else if upper.starts_with("EXAMINE ") {
            let name = quoted_args(&command[8..]).pop().unwrap_or_default();
            let state = state.lock().unwrap();
            match state.folders.iter().find(|folder| folder.name == name) {
                Some(folder) => {
                    selected = Some(name);
                    let next = folder.messages.iter().map(|m| m.uid).max().unwrap_or(0) + 1;
                    format!(
                        "* {} EXISTS\r\n* OK [UIDVALIDITY {}] UIDs valid\r\n* OK [UIDNEXT {next}] Predicted next UID\r\n{tag} OK [READ-ONLY] EXAMINE completed\r\n",
                        folder.messages.len(),
                        folder.uid_validity
                    )
                }
                None => format!("{tag} NO Mailbox does not exist\r\n"),
            }
        } else if upper.starts_with("UID SEARCH ") {
            let state = state.lock().unwrap();
            let folder = state
                .folders
                .iter()
                .find(|f| Some(&f.name) == selected.as_ref())
                .unwrap();
            let criteria = &command[11..];
            let uids: Vec<u32> = match criteria.strip_prefix("UID ") {
                Some(range) => {
                    let start: u32 = range.trim_end_matches(":*").parse().unwrap();
                    let mut found: Vec<u32> = folder
                        .messages
                        .iter()
                        .map(|m| m.uid)
                        .filter(|uid| *uid >= start)
                        .collect();
                    // RFC 3501: `n:*` always includes the highest UID.
                    if found.is_empty()
                        && let Some(highest) = folder.messages.iter().map(|m| m.uid).max()
                    {
                        found.push(highest);
                    }
                    found
                }
                None => folder.messages.iter().map(|m| m.uid).collect(),
            };
            let list: Vec<String> = uids.iter().map(ToString::to_string).collect();
            format!(
                "* SEARCH {}\r\n{tag} OK SEARCH completed\r\n",
                list.join(" ")
            )
        } else if upper.starts_with("UID FETCH ") {
            let state = state.lock().unwrap();
            let folder = state
                .folders
                .iter()
                .find(|f| Some(&f.name) == selected.as_ref())
                .unwrap();
            let rest = &command[10..];
            let (set, items) = rest.split_once(' ').unwrap();
            let wanted: Vec<u32> = set.split(',').map(|uid| uid.parse().unwrap()).collect();
            let mut reply = String::new();
            for (index, message) in folder.messages.iter().enumerate() {
                if !wanted.contains(&message.uid) {
                    continue;
                }
                reply.push_str(&fetch_response(index + 1, message, items));
            }
            reply.push_str(&format!("{tag} OK FETCH completed\r\n"));
            reply
        } else if upper == "LOGOUT" {
            let _ = out.write_all(
                format!("* BYE Synthetic IMAP logging out\r\n{tag} OK LOGOUT completed\r\n")
                    .as_bytes(),
            );
            return;
        } else {
            format!("{tag} BAD Unknown command\r\n")
        };
        if out.write_all(reply.as_bytes()).is_err() {
            return;
        }
    }
}

fn fetch_response(sequence: usize, message: &FakeMessage, items: &str) -> String {
    let structure = format!(
        "(\"TEXT\" \"PLAIN\" (\"CHARSET\" \"utf-8\") NIL NIL \"{}\" {} 3)",
        message.encoding,
        message.body.len()
    );
    let mut parts = vec![format!("UID {}", message.uid)];
    if items.contains("ENVELOPE") {
        let (mailbox, host) = message.from_address.split_once('@').unwrap();
        let in_reply_to = match &message.references {
            Some(reference) => format!("\"{reference}\""),
            None => "NIL".to_owned(),
        };
        parts.push(format!("INTERNALDATE \"{}\"", message.internal_date));
        parts.push(format!(
            "ENVELOPE (\"Thu, 22 Oct 2026 09:00:00 +0200\" \"{}\" ((\"{}\" NIL \"{mailbox}\" \"{host}\")) NIL NIL NIL NIL NIL {in_reply_to} \"{}\")",
            message.subject, message.from_name, message.message_id
        ));
        parts.push(format!("BODYSTRUCTURE {structure}"));
        let references = match &message.references {
            Some(reference) => format!("References: {reference}\r\n\r\n"),
            None => "\r\n".to_owned(),
        };
        parts.push(format!(
            "BODY[HEADER.FIELDS (REFERENCES)] {{{}}}\r\n{references}",
            references.len()
        ));
    } else if items.contains("BODY.PEEK[1]") {
        parts.push(format!(
            "BODY[1]<0> {{{}}}\r\n{}",
            message.body.len(),
            message.body
        ));
    } else if items.contains("BODYSTRUCTURE") {
        parts.push(format!("BODYSTRUCTURE {structure}"));
    }
    format!("* {sequence} FETCH ({})\r\n", parts.join(" "))
}

/// The quoted strings of an argument list, unescaped.
fn quoted_args(text: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '"' {
            continue;
        }
        let mut arg = String::new();
        while let Some(c) = chars.next() {
            match c {
                '\\' => arg.extend(chars.next()),
                '"' => break,
                other => arg.push(other),
            }
        }
        args.push(arg);
    }
    args
}

// The mock HTTP server.

/// One request the mock server received.
#[derive(Debug, Clone)]
pub struct Received {
    pub method: String,
    pub path: String,
    pub headers: BTreeMap<String, String>,
    pub body: String,
}

/// A canned response.
#[derive(Debug, Clone)]
pub struct Canned {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

type Handler = dyn Fn(&Received) -> Canned + Send + Sync;

/// A mock HTTP/1.1 server on a loopback port: every request goes to the
/// handler, and is logged.
pub struct MockHttp {
    pub base: String,
    pub requests: Arc<Mutex<Vec<Received>>>,
}

impl Canned {
    pub fn json(status: u16, body: &str) -> Canned {
        Canned {
            status,
            headers: vec![("Content-Type".to_owned(), "application/json".to_owned())],
            body: body.to_owned(),
        }
    }
}

impl MockHttp {
    pub fn start(handler: impl Fn(&Received) -> Canned + Send + Sync + 'static) -> MockHttp {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let log = requests.clone();
        let handler: Arc<Handler> = Arc::new(handler);
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { return };
                let handler = handler.clone();
                let log = log.clone();
                thread::spawn(move || serve_http(stream, handler.as_ref(), &log));
            }
        });
        MockHttp { base, requests }
    }

    pub fn requests(&self) -> Vec<Received> {
        self.requests.lock().unwrap().clone()
    }
}

fn serve_http(stream: TcpStream, handler: &Handler, log: &Mutex<Vec<Received>>) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut out = stream;
    loop {
        let mut request_line = String::new();
        if reader.read_line(&mut request_line).unwrap_or(0) == 0 {
            return;
        }
        let mut parts = request_line.split_whitespace();
        let method = parts.next().unwrap_or_default().to_owned();
        let path = parts.next().unwrap_or_default().to_owned();
        let mut headers = BTreeMap::new();
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let line = line.trim_end();
            if line.is_empty() {
                break;
            }
            if let Some((name, value)) = line.split_once(':') {
                headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_owned());
            }
        }
        let length: usize = headers
            .get("content-length")
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        let mut body = vec![0; length];
        reader.read_exact(&mut body).unwrap();
        let received = Received {
            method,
            path,
            headers,
            body: String::from_utf8_lossy(&body).into_owned(),
        };
        log.lock().unwrap().push(received.clone());
        let canned = handler(&received);
        let mut response = format!(
            "HTTP/1.1 {} Mock\r\nContent-Length: {}\r\n",
            canned.status,
            canned.body.len()
        );
        for (name, value) in &canned.headers {
            response.push_str(&format!("{name}: {value}\r\n"));
        }
        response.push_str("\r\n");
        response.push_str(&canned.body);
        if out.write_all(response.as_bytes()).is_err() {
            return;
        }
    }
}

// The workspace document.

pub const WORKSPACE: &str = "ws-mail-test";

pub fn workspace_id() -> WorkspaceId {
    WorkspaceId::new(WORKSPACE)
}

/// A device's encrypted workspace document over `root`.
pub fn create_workspace(root: &Path, secrets: &InMemorySecretStore) -> WorkspaceSync {
    create_workspace_key(secrets, &workspace_id()).unwrap();
    WorkspaceSync::create(
        Arc::new(PlainSyncFiles),
        root,
        DeviceId::new("mac-a").unwrap(),
        key(secrets),
        SyncConfig::default(),
    )
    .unwrap()
}

/// The same device opening the document again, as after a relaunch.
pub fn reopen_workspace(root: &Path, secrets: &InMemorySecretStore, device: &str) -> WorkspaceSync {
    WorkspaceSync::open(
        Arc::new(PlainSyncFiles),
        root,
        DeviceId::new(device).unwrap(),
        key(secrets),
        SyncConfig::default(),
    )
    .unwrap()
    .0
}

pub fn key(secrets: &InMemorySecretStore) -> DataKey {
    workspace_key(secrets, &workspace_id()).unwrap().unwrap()
}

/// Retries without waiting.
pub fn instant_retries() -> RetryPolicy {
    RetryPolicy {
        sleep: |_| {},
        ..RetryPolicy::default()
    }
}
