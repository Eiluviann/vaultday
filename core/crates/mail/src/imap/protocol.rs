//! The IMAP4rev1 subset the source speaks (RFC 3501): greeting, `LOGIN`,
//! `AUTHENTICATE XOAUTH2`, `LIST`, `EXAMINE`, `UID SEARCH`, `UID FETCH` and
//! `LOGOUT`, over any byte stream. Responses are read whole, literals
//! included, and parsed into [`Item`] trees.

use std::io::{self, BufRead, BufReader, Read, Write};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use super::ImapStream;
use crate::error::MailError;

/// The largest response accepted, literals included.
const MAX_RESPONSE_BYTES: usize = 32 * 1024 * 1024;

/// One parsed element of a response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Item {
    /// An atom or number.
    Atom(String),
    /// A quoted string or a literal.
    Str(Vec<u8>),
    /// `NIL`.
    Nil,
    /// A parenthesised list.
    List(Vec<Item>),
}

/// One argument of a command.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Arg<'a> {
    /// Sent as is.
    Raw(&'a str),
    /// Sent quoted, or as a literal when it cannot be quoted.
    Str(&'a [u8]),
}

/// A selected mailbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Mailbox {
    pub(crate) uid_validity: u32,
    pub(crate) uid_next: u32,
}

/// A mailbox from `LIST`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Listed {
    pub(crate) attributes: Vec<String>,
    pub(crate) name: String,
}

/// An IMAP connection after the greeting.
pub(crate) struct Client {
    stream: BufReader<Box<dyn ImapStream>>,
    next_tag: u32,
}

#[derive(Debug, PartialEq, Eq)]
enum Status {
    Ok,
    No,
    Bad,
}

struct Completion {
    untagged: Vec<Vec<u8>>,
    status: Status,
    text: String,
}

impl Item {
    pub(crate) fn text(&self) -> Option<String> {
        match self {
            Item::Atom(atom) => Some(atom.clone()),
            Item::Str(bytes) => Some(String::from_utf8_lossy(bytes).into_owned()),
            Item::Nil | Item::List(_) => None,
        }
    }

    pub(crate) fn bytes(&self) -> Option<&[u8]> {
        match self {
            Item::Str(bytes) => Some(bytes),
            Item::Atom(atom) => Some(atom.as_bytes()),
            Item::Nil | Item::List(_) => None,
        }
    }

    pub(crate) fn list(&self) -> Option<&[Item]> {
        match self {
            Item::List(items) => Some(items),
            _ => None,
        }
    }
}

impl Client {
    /// Reads the server's greeting.
    pub(crate) fn greet(stream: Box<dyn ImapStream>) -> Result<Client, MailError> {
        let mut client = Client {
            stream: BufReader::new(stream),
            next_tag: 1,
        };
        let greeting = client.read_response()?;
        if greeting.starts_with(b"* OK") || greeting.starts_with(b"* PREAUTH") {
            return Ok(client);
        }
        if greeting.starts_with(b"* BYE") {
            return Err(MailError::network(format!(
                "the server refused the connection: {}",
                String::from_utf8_lossy(&greeting)
            )));
        }
        Err(MailError::Protocol(
            "the server did not greet as an IMAP server".to_owned(),
        ))
    }

    /// `LOGIN`; a refusal is [`MailError::Auth`].
    pub(crate) fn login(&mut self, user: &str, password: &[u8]) -> Result<(), MailError> {
        let done = self.run(&[
            Arg::Raw("LOGIN"),
            Arg::Str(user.as_bytes()),
            Arg::Str(password),
        ])?;
        match done.status {
            Status::Ok => Ok(()),
            _ => Err(MailError::Auth(format!(
                "the server refused the password: {}",
                done.text
            ))),
        }
    }

    /// `AUTHENTICATE XOAUTH2` with the initial response; a refusal is [`MailError::Auth`].
    pub(crate) fn authenticate_xoauth2(
        &mut self,
        user: &str,
        token: &str,
    ) -> Result<(), MailError> {
        let payload = STANDARD.encode(format!("user={user}\u{1}auth=Bearer {token}\u{1}\u{1}"));
        let tag = self.tag();
        self.write_all(format!("{tag} AUTHENTICATE XOAUTH2 {payload}\r\n").as_bytes())?;
        loop {
            let line = self.read_response()?;
            if line.starts_with(b"+") {
                // The server's error challenge: an empty line ends the exchange.
                self.write_all(b"\r\n")?;
                continue;
            }
            if let Some(status) = tagged_status(&line, &tag) {
                return match status.0 {
                    Status::Ok => Ok(()),
                    _ => Err(MailError::Auth(format!(
                        "the server refused the token: {}",
                        status.1
                    ))),
                };
            }
        }
    }

    /// `LIST "" "*"`.
    pub(crate) fn list(&mut self) -> Result<Vec<Listed>, MailError> {
        let done = self.run_ok(&[Arg::Raw("LIST"), Arg::Str(b""), Arg::Str(b"*")])?;
        Ok(done
            .iter()
            .filter_map(|line| line.strip_prefix(b"* LIST "))
            .filter_map(|rest| {
                let items = parse(rest).ok()?;
                let attributes = items
                    .first()?
                    .list()?
                    .iter()
                    .filter_map(Item::text)
                    .collect();
                let name = items.get(2)?.text()?;
                Some(Listed { attributes, name })
            })
            .collect())
    }

    /// `EXAMINE`: selects read-only, so nothing fetched is marked read.
    pub(crate) fn examine(&mut self, mailbox: &str) -> Result<Mailbox, MailError> {
        let done = self.run(&[Arg::Raw("EXAMINE"), Arg::Str(mailbox.as_bytes())])?;
        if done.status != Status::Ok {
            return Err(MailError::FolderNotFound(mailbox.to_owned()));
        }
        let code = |name: &str| {
            done.untagged
                .iter()
                .find_map(|line| response_code_number(line, name))
        };
        let uid_validity = code("UIDVALIDITY")
            .ok_or_else(|| MailError::Protocol(format!("{mailbox} has no UIDVALIDITY")))?;
        Ok(Mailbox {
            uid_validity,
            uid_next: code("UIDNEXT").unwrap_or(0),
        })
    }

    /// `UID SEARCH <criteria>`.
    pub(crate) fn uid_search(&mut self, criteria: &str) -> Result<Vec<u32>, MailError> {
        let done = self.run_ok(&[Arg::Raw("UID SEARCH"), Arg::Raw(criteria)])?;
        Ok(done
            .iter()
            .filter_map(|line| line.strip_prefix(b"* SEARCH"))
            .flat_map(|rest| {
                String::from_utf8_lossy(rest)
                    .split_whitespace()
                    .filter_map(|uid| uid.parse().ok())
                    .collect::<Vec<u32>>()
            })
            .collect())
    }

    /// `UID FETCH <set> <items>`: the attribute list of every `FETCH` response.
    pub(crate) fn uid_fetch(
        &mut self,
        set: &str,
        items: &str,
    ) -> Result<Vec<Vec<Item>>, MailError> {
        let done = self.run_ok(&[Arg::Raw("UID FETCH"), Arg::Raw(set), Arg::Raw(items)])?;
        let mut fetched = Vec::new();
        for line in &done {
            let Some(rest) = line.strip_prefix(b"* ") else {
                continue;
            };
            let items = parse(rest)?;
            if let [Item::Atom(_), Item::Atom(kind), Item::List(attributes)] = items.as_slice()
                && kind.eq_ignore_ascii_case("FETCH")
            {
                fetched.push(attributes.clone());
            }
        }
        Ok(fetched)
    }

    /// `LOGOUT`; the connection closes after it.
    pub(crate) fn logout(&mut self) -> Result<(), MailError> {
        self.run(&[Arg::Raw("LOGOUT")]).map(|_| ())
    }

    fn run_ok(&mut self, args: &[Arg<'_>]) -> Result<Vec<Vec<u8>>, MailError> {
        let done = self.run(args)?;
        match done.status {
            Status::Ok => Ok(done.untagged),
            Status::No => Err(MailError::Protocol(format!(
                "the server refused the command: {}",
                done.text
            ))),
            Status::Bad => Err(MailError::Protocol(format!(
                "the server did not understand the command: {}",
                done.text
            ))),
        }
    }

    fn run(&mut self, args: &[Arg<'_>]) -> Result<Completion, MailError> {
        let tag = self.tag();
        let mut pending = tag.clone().into_bytes();
        for arg in args {
            pending.push(b' ');
            match arg {
                Arg::Raw(text) => pending.extend_from_slice(text.as_bytes()),
                Arg::Str(bytes) if can_quote(bytes) => {
                    pending.push(b'"');
                    for &byte in *bytes {
                        if byte == b'"' || byte == b'\\' {
                            pending.push(b'\\');
                        }
                        pending.push(byte);
                    }
                    pending.push(b'"');
                }
                Arg::Str(bytes) => {
                    pending.extend_from_slice(format!("{{{}}}\r\n", bytes.len()).as_bytes());
                    self.write_all(&pending)?;
                    self.await_continuation(&tag)?;
                    pending = bytes.to_vec();
                }
            }
        }
        pending.extend_from_slice(b"\r\n");
        self.write_all(&pending)?;
        let mut untagged = Vec::new();
        loop {
            let line = self.read_response()?;
            if let Some((status, text)) = tagged_status(&line, &tag) {
                return Ok(Completion {
                    untagged,
                    status,
                    text,
                });
            }
            if line.starts_with(b"* BYE")
                && !args.iter().any(|arg| matches!(arg, Arg::Raw("LOGOUT")))
            {
                return Err(MailError::network(format!(
                    "the server closed the connection: {}",
                    String::from_utf8_lossy(&line)
                )));
            }
            untagged.push(line);
        }
    }

    fn await_continuation(&mut self, tag: &str) -> Result<(), MailError> {
        loop {
            let line = self.read_response()?;
            if line.starts_with(b"+") {
                return Ok(());
            }
            if let Some((_, text)) = tagged_status(&line, tag) {
                return Err(MailError::Protocol(format!(
                    "the server refused a literal: {text}"
                )));
            }
        }
    }

    fn tag(&mut self) -> String {
        let tag = format!("v{}", self.next_tag);
        self.next_tag += 1;
        tag
    }

    fn write_all(&mut self, bytes: &[u8]) -> Result<(), MailError> {
        let stream = self.stream.get_mut();
        stream
            .write_all(bytes)
            .and_then(|()| stream.flush())
            .map_err(io_error)
    }

    /// One whole response: a line, and every literal it announces with the
    /// lines that follow them, without the final CRLF.
    fn read_response(&mut self) -> Result<Vec<u8>, MailError> {
        let mut response = Vec::new();
        loop {
            let start = response.len();
            // One byte past the budget, so a line that never ends is caught
            // without reading (or holding) more than the budget.
            let budget = MAX_RESPONSE_BYTES.saturating_sub(start).saturating_add(1);
            let read = (&mut self.stream)
                .take(u64::try_from(budget).unwrap_or(u64::MAX))
                .read_until(b'\n', &mut response)
                .map_err(io_error)?;
            if read == 0 {
                return Err(MailError::network("the server closed the connection"));
            }
            if response.len() > MAX_RESPONSE_BYTES {
                return Err(too_large());
            }
            match literal_length(&response[start..]) {
                Some(length) => {
                    // The announced length is the server's word: check it
                    // before allocating, never after.
                    if length > MAX_RESPONSE_BYTES - response.len() {
                        return Err(too_large());
                    }
                    let mut literal = vec![0; length];
                    self.stream.read_exact(&mut literal).map_err(io_error)?;
                    response.extend_from_slice(&literal);
                }
                None => {
                    while matches!(response.last(), Some(b'\n' | b'\r')) {
                        response.pop();
                    }
                    return Ok(response);
                }
            }
        }
    }
}

/// Parses a response (after `* `) into items.
pub(crate) fn parse(bytes: &[u8]) -> Result<Vec<Item>, MailError> {
    let mut parser = Parser { bytes, position: 0 };
    let mut items = Vec::new();
    while let Some(item) = parser.item()? {
        items.push(item);
    }
    Ok(items)
}

struct Parser<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl Parser<'_> {
    fn item(&mut self) -> Result<Option<Item>, MailError> {
        self.skip_spaces();
        let Some(&byte) = self.bytes.get(self.position) else {
            return Ok(None);
        };
        match byte {
            b'(' => {
                self.position += 1;
                let mut items = Vec::new();
                loop {
                    self.skip_spaces();
                    match self.bytes.get(self.position) {
                        Some(b')') => {
                            self.position += 1;
                            return Ok(Some(Item::List(items)));
                        }
                        None => return Err(malformed("an unclosed list")),
                        Some(_) => match self.item()? {
                            Some(item) => items.push(item),
                            None => return Err(malformed("an unclosed list")),
                        },
                    }
                }
            }
            b')' => Err(malformed("an unopened list")),
            b'"' => self.quoted().map(Some),
            b'{' => self.literal().map(Some),
            _ => {
                let atom = self.atom();
                if atom.eq_ignore_ascii_case("NIL") {
                    Ok(Some(Item::Nil))
                } else {
                    Ok(Some(Item::Atom(atom)))
                }
            }
        }
    }

    fn quoted(&mut self) -> Result<Item, MailError> {
        self.position += 1;
        let mut text = Vec::new();
        loop {
            match self.bytes.get(self.position) {
                Some(b'"') => {
                    self.position += 1;
                    return Ok(Item::Str(text));
                }
                Some(b'\\') => {
                    let escaped = self
                        .bytes
                        .get(self.position + 1)
                        .ok_or_else(|| malformed("an unclosed string"))?;
                    text.push(*escaped);
                    self.position += 2;
                }
                Some(&byte) => {
                    text.push(byte);
                    self.position += 1;
                }
                None => return Err(malformed("an unclosed string")),
            }
        }
    }

    fn literal(&mut self) -> Result<Item, MailError> {
        let close = self.bytes[self.position..]
            .iter()
            .position(|&byte| byte == b'}')
            .ok_or_else(|| malformed("an unclosed literal length"))?;
        let digits = String::from_utf8_lossy(&self.bytes[self.position + 1..self.position + close]);
        let length: usize = digits
            .trim_end_matches('+')
            .parse()
            .map_err(|_| malformed("a literal length"))?;
        let mut start = self.position + close + 1;
        if self.bytes.get(start) == Some(&b'\r') {
            start += 1;
        }
        if self.bytes.get(start) == Some(&b'\n') {
            start += 1;
        }
        let literal = start
            .checked_add(length)
            .and_then(|end| self.bytes.get(start..end))
            .ok_or_else(|| malformed("a short literal"))?;
        self.position = start + literal.len();
        Ok(Item::Str(literal.to_vec()))
    }

    /// An atom; a `[…]` section (which may hold spaces and parentheses) and a
    /// `<…>` origin belong to it, as in `BODY[HEADER.FIELDS (REFERENCES)]`.
    fn atom(&mut self) -> String {
        let start = self.position;
        let mut depth = 0_u32;
        while let Some(&byte) = self.bytes.get(self.position) {
            match byte {
                b'[' => depth += 1,
                b']' => depth = depth.saturating_sub(1),
                b' ' | b'(' | b')' | b'\r' | b'\n' if depth == 0 => break,
                _ => {}
            }
            self.position += 1;
        }
        String::from_utf8_lossy(&self.bytes[start..self.position]).into_owned()
    }

    fn skip_spaces(&mut self) {
        while matches!(self.bytes.get(self.position), Some(b' ' | b'\r' | b'\n')) {
            self.position += 1;
        }
    }
}

/// The length of the literal a line ends by announcing (`{12}` or `{12+}`).
fn literal_length(line: &[u8]) -> Option<usize> {
    let line = line
        .strip_suffix(b"\r\n")
        .or_else(|| line.strip_suffix(b"\n"))?;
    let line = line.strip_suffix(b"}")?;
    let open = line.iter().rposition(|&byte| byte == b'{')?;
    let digits = std::str::from_utf8(&line[open + 1..]).ok()?;
    digits.trim_end_matches('+').parse().ok()
}

fn tagged_status(line: &[u8], tag: &str) -> Option<(Status, String)> {
    let rest = line.strip_prefix(tag.as_bytes())?.strip_prefix(b" ")?;
    let text = String::from_utf8_lossy(rest);
    let (word, message) = text.split_once(' ').unwrap_or((&text, ""));
    let status = match word.to_ascii_uppercase().as_str() {
        "OK" => Status::Ok,
        "NO" => Status::No,
        "BAD" => Status::Bad,
        _ => return None,
    };
    Some((status, message.to_owned()))
}

/// The number in a `* OK [NAME n]` response code.
fn response_code_number(line: &[u8], name: &str) -> Option<u32> {
    let text = String::from_utf8_lossy(line);
    let start = text.find(&format!("[{name} "))? + name.len() + 2;
    let end = text[start..].find(']')? + start;
    text[start..end].trim().parse().ok()
}

fn can_quote(bytes: &[u8]) -> bool {
    bytes
        .iter()
        .all(|&byte| byte.is_ascii() && byte != b'\r' && byte != b'\n' && byte != 0)
}

fn io_error(error: io::Error) -> MailError {
    match error.kind() {
        io::ErrorKind::InvalidData => {
            MailError::Protocol(format!("the secure connection failed: {error}"))
        }
        _ => MailError::network(format!("the connection failed: {error}")),
    }
}

fn too_large() -> MailError {
    MailError::Protocol("a response is too large".to_owned())
}

fn malformed(what: &str) -> MailError {
    MailError::Protocol(format!("the server sent {what}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fetch_responses_parse_with_sections_and_literals() {
        let line = b"12 FETCH (UID 5 FLAGS (\\Seen) BODY[HEADER.FIELDS (REFERENCES)] {11}\r\nReferences:\r\n BODY[1]<0> NIL ENVELOPE (\"date\" \"subj \\\"q\\\"\"))";
        let items = parse(line).unwrap();
        let [
            Item::Atom(number),
            Item::Atom(fetch),
            Item::List(attributes),
        ] = items.as_slice()
        else {
            panic!("{items:?}");
        };
        assert_eq!((number.as_str(), fetch.as_str()), ("12", "FETCH"));
        assert_eq!(attributes[0], Item::Atom("UID".to_owned()));
        assert_eq!(
            attributes[4],
            Item::Atom("BODY[HEADER.FIELDS (REFERENCES)]".to_owned())
        );
        assert_eq!(attributes[5], Item::Str(b"References:".to_vec()));
        assert_eq!(attributes[6], Item::Atom("BODY[1]<0>".to_owned()));
        assert_eq!(attributes[7], Item::Nil);
        let envelope = attributes[9].list().unwrap();
        assert_eq!(envelope[1], Item::Str(b"subj \"q\"".to_vec()));
    }

    #[test]
    fn literal_announcements_and_codes_are_recognised() {
        assert_eq!(literal_length(b"* 1 FETCH (BODY[] {42}\r\n"), Some(42));
        assert_eq!(literal_length(b"a LOGIN {3+}\r\n"), Some(3));
        assert_eq!(literal_length(b"* OK done\r\n"), None);
        assert_eq!(
            response_code_number(b"* OK [UIDVALIDITY 3857529045] UIDs valid", "UIDVALIDITY"),
            Some(3_857_529_045)
        );
        assert_eq!(
            tagged_status(b"v3 NO [AUTHENTICATIONFAILED] Invalid", "v3"),
            Some((Status::No, "[AUTHENTICATIONFAILED] Invalid".to_owned()))
        );
        assert!(tagged_status(b"v31 OK", "v3").is_none());
    }

    #[test]
    fn malformed_responses_are_protocol_errors() {
        assert!(matches!(parse(b"(UID 5"), Err(MailError::Protocol(_))));
        assert!(matches!(parse(b"\"open"), Err(MailError::Protocol(_))));
        assert!(matches!(parse(b"{9}\r\nab"), Err(MailError::Protocol(_))));
        assert!(matches!(
            parse(b"(BODY[] {18446744073709551615}\r\nab)"),
            Err(MailError::Protocol(_))
        ));
    }

    /// A server that sends `input` and swallows whatever the client writes.
    struct Scripted<R> {
        input: R,
    }

    impl<R: io::Read> io::Read for Scripted<R> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.input.read(buf)
        }
    }

    impl<R> io::Write for Scripted<R> {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn client(input: &[u8]) -> Result<Client, MailError> {
        Client::greet(Box::new(Scripted {
            input: io::Cursor::new(input.to_vec()),
        }))
    }

    const FETCH_SCRIPT: &[u8] = b"* OK ready\r\n* 1 FETCH (UID 4 ENVELOPE (\"Thu, 22 Oct 2026 09:00:00 +0200\" \"Plan\" ((\"A\" NIL \"a\" \"example.test\")) NIL NIL NIL NIL NIL NIL \"<m@example.test>\") BODY[1]<0> {12}\r\nHello there.)\r\nv1 OK done\r\n";

    #[test]
    fn a_whole_script_fetches() {
        let fetched = client(FETCH_SCRIPT)
            .unwrap()
            .uid_fetch("4", "(UID ENVELOPE)")
            .unwrap();
        assert_eq!(fetched.len(), 1);
        assert_eq!(fetched[0][5], Item::Str(b"Hello there.".to_vec()));
    }

    #[test]
    fn every_truncation_of_a_response_is_an_error_not_a_panic() {
        // Once `v1 OK` has arrived the command did complete; any cut before it must fail.
        let completed = FETCH_SCRIPT
            .windows(5)
            .position(|window| window == b"v1 OK")
            .unwrap()
            + 5;
        for cut in 0..completed {
            let outcome = client(&FETCH_SCRIPT[..cut])
                .and_then(|mut client| client.uid_fetch("4", "(UID ENVELOPE)"));
            assert!(outcome.is_err(), "a script cut at {cut} fetched");
        }
    }

    #[test]
    fn mangled_responses_never_panic() {
        // Deterministic byte flips across the script: any outcome but a panic.
        for position in 0..FETCH_SCRIPT.len() {
            for replacement in [
                b'{', b'}', b'(', b')', b'"', b'\\', b'\r', b'\n', b'9', 0xff,
            ] {
                let mut mangled = FETCH_SCRIPT.to_vec();
                mangled[position] = replacement;
                let _ = client(&mangled).and_then(|mut client| client.uid_fetch("4", "(UID)"));
            }
        }
    }

    #[test]
    fn an_oversized_literal_is_refused_before_it_is_allocated() {
        let outcome = client(b"* OK ready\r\n* 1 FETCH (BODY[] {18446744073709551615}\r\nab")
            .and_then(|mut client| client.uid_fetch("1", "(BODY[])"));
        assert!(
            matches!(&outcome, Err(MailError::Protocol(text)) if text.contains("too large")),
            "{outcome:?}",
            outcome = outcome.map(|_| ())
        );
    }

    #[test]
    fn a_line_that_never_ends_is_refused_at_the_limit() {
        let endless = io::Read::chain(&b"* OK "[..], io::repeat(b'a'));
        let outcome = Client::greet(Box::new(Scripted { input: endless }));
        assert!(matches!(
            outcome.map(|_| ()),
            Err(MailError::Protocol(text)) if text.contains("too large")
        ));
    }
}
