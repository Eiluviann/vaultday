//! Links back to a message in the account's mail app, and the check every
//! stored link passes before the app opens it: a signal synced from another
//! device is data, so only `message:` and `https:` links ever leave the core.

/// The `message:` URL Apple Mail (and other clients that register the
/// scheme) opens: the `Message-ID` with its angle brackets, percent-encoded.
#[must_use]
pub fn imap_message_link(message_id: &str) -> String {
    let id = message_id.trim();
    let bracketed = if id.starts_with('<') && id.ends_with('>') {
        id.to_owned()
    } else {
        format!("<{id}>")
    };
    format!("message:{}", percent_encode(&bracketed))
}

/// The link if it is one the app may open: `message:` or `https:`, with no
/// whitespace or control characters.
#[must_use]
pub fn openable_link(link: &str) -> Option<&str> {
    let is_allowed_scheme = link.starts_with("message:") || link.starts_with("https://");
    let is_clean = !link.chars().any(|c| c.is_whitespace() || c.is_control());
    (is_allowed_scheme && is_clean && link.len() <= 4096).then_some(link)
}

fn percent_encode(text: &str) -> String {
    text.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'.'
            | b'_'
            | b'~'
            | b'@'
            | b'!'
            | b'$'
            | b'\''
            | b'('
            | b')'
            | b'*'
            | b'+'
            | b','
            | b';'
            | b'='
            | b':' => char::from(byte).to_string(),
            other => format!("%{other:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_links_encode_the_brackets() {
        assert_eq!(
            imap_message_link("<abc.123@example.test>"),
            "message:%3Cabc.123@example.test%3E"
        );
        assert_eq!(
            imap_message_link("abc 1@example.test"),
            "message:%3Cabc%201@example.test%3E"
        );
    }

    #[test]
    fn only_message_and_https_links_open() {
        assert!(openable_link("message:%3Cabc@example.test%3E").is_some());
        assert!(openable_link("https://outlook.office365.com/owa/?ItemID=x").is_some());
        for refused in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "http://example.test",
            "https://example.test/a b",
            "x-apple.systempreferences:",
        ] {
            assert!(openable_link(refused).is_none(), "{refused}");
        }
    }
}
