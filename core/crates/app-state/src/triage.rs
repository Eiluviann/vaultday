//! What triage pre-fills from a signal, without a model: the headline is the
//! subject with its reply and forward prefixes taken off, the sender is the
//! name the message gives (else the address), and the ticket is the first
//! number the workspace's `ticket_pattern` finds in the subject, else in the
//! excerpt. Nothing is guessed: a value the mail's plain fields do not hold
//! stays empty.
//!
//! The headline is also the only text of the mail that ever reaches the
//! vault: a task line made from a signal carries the cleaned headline (or
//! what the user typed instead) and `[source:: sig-…]`, never the excerpt or
//! the body, which stay in the encrypted store.

use domain::ticket::TicketPattern;

use crate::signals::Signal;

/// The headline of a message whose subject is empty once cleaned.
pub const UNTITLED_HEADLINE: &str = "Mail without a subject";

/// Reply and forward markers taken off the front of a subject, compared
/// without case: English, German, Nordic and Dutch mail clients.
const SUBJECT_PREFIXES: [&str; 8] = ["re", "fw", "fwd", "aw", "wg", "sv", "vs", "antw"];

/// The plain values triage starts from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignalPrefill {
    /// The subject, cleaned for a task line.
    pub headline: String,
    /// The sender's name, else the address; safe as an `assigned_to` value.
    pub sender: String,
    /// The first ticket number found, subject before excerpt.
    pub ticket: Option<String>,
}

/// The pre-filled values for `signal` under the workspace's ticket pattern.
#[must_use]
pub fn prefill(signal: &Signal, pattern: Option<&TicketPattern>) -> SignalPrefill {
    SignalPrefill {
        headline: clean_headline(&signal.subject),
        sender: sender_label(signal),
        ticket: matched_ticket(signal, pattern),
    }
}

/// The subject as one task text: reply and forward prefixes off, control
/// characters and line breaks turned into single spaces, `%%` (an Obsidian
/// comment and the provenance markers' delimiter) cut to `%`, brackets
/// turned into parentheses so nothing reads as an inline field or a link.
/// An empty result is [`UNTITLED_HEADLINE`].
#[must_use]
pub fn clean_headline(subject: &str) -> String {
    let collapsed = line_text(subject);
    let mut rest = collapsed.as_str();
    while let Some(stripped) = strip_prefix_marker(rest) {
        rest = stripped;
    }
    let text = without_brackets(rest.trim());
    if text.is_empty() {
        UNTITLED_HEADLINE.to_owned()
    } else {
        text
    }
}

/// The sender as the Inbox shows it and as `assigned_to` takes it.
#[must_use]
pub fn sender_label(signal: &Signal) -> String {
    let name = signal
        .sender_name
        .as_deref()
        .map(line_text)
        .filter(|name| !name.is_empty());
    let label = name.unwrap_or_else(|| line_text(&signal.sender_address));
    without_brackets(&label)
}

/// `text` as one plain line of a note: control characters and line breaks
/// become single spaces, runs of whitespace collapse, a run of `%` is cut
/// to one, so no `%%` comment or provenance marker can open inside a task
/// line, a backtick becomes `'` so no code span swallows the fields after
/// it, and `<!--` / `-->` are broken apart (`< !--`, `-- >`) so no HTML
/// comment hides the rest of the note. Brackets are kept: the writer
/// refuses inline fields itself.
#[must_use]
pub fn line_text(text: &str) -> String {
    let spaced: String = text
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else if character == '`' {
                '\''
            } else {
                character
            }
        })
        .collect();
    let collapsed = spaced.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut cleaned = String::with_capacity(collapsed.len());
    for character in collapsed.chars() {
        if character == '%' && cleaned.ends_with('%') {
            continue;
        }
        cleaned.push(character);
    }
    cleaned.replace("<!--", "< !--").replace("-->", "-- >")
}

/// The first ticket number in the subject, else in the excerpt; `None`
/// without a pattern.
#[must_use]
pub fn matched_ticket(signal: &Signal, pattern: Option<&TicketPattern>) -> Option<String> {
    let pattern = pattern?;
    [signal.subject.as_str(), signal.excerpt.as_str()]
        .into_iter()
        .find_map(|text| {
            pattern
                .find_all(text)
                .first()
                .map(|found| (*found).to_owned())
        })
}

/// `rest` after one leading `Re:`-style marker, when it has one.
fn strip_prefix_marker(text: &str) -> Option<&str> {
    let (head, tail) = text.split_once(':')?;
    let head = head.trim();
    let is_marker = SUBJECT_PREFIXES
        .iter()
        .any(|prefix| head.eq_ignore_ascii_case(prefix));
    is_marker.then(|| tail.trim_start())
}

fn without_brackets(text: &str) -> String {
    text.replace('[', "(").replace(']', ")")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_doc::ConnectionId;
    use crate::signals::SignalId;

    fn signal(subject: &str, excerpt: &str, name: Option<&str>) -> Signal {
        Signal {
            id: SignalId::new("sig-0000000000000001"),
            account: ConnectionId::new("con-mail-1"),
            folder: "INBOX".to_owned(),
            provider_ref: "7:1".to_owned(),
            message_id: "<m1@example.test>".to_owned(),
            thread_id: "<m1@example.test>".to_owned(),
            sender_name: name.map(str::to_owned),
            sender_address: "lena@example.test".to_owned(),
            received_at: "2026-10-23T08:00:00Z".parse().unwrap(),
            subject: subject.to_owned(),
            excerpt: excerpt.to_owned(),
            link: "message:%3Cm1@example.test%3E".to_owned(),
            source_hash: String::new(),
        }
    }

    #[test]
    fn headlines_lose_reply_markers_and_brackets() {
        assert_eq!(
            clean_headline("RE: Fwd:  WEB-151 pricing page"),
            "WEB-151 pricing page"
        );
        assert_eq!(clean_headline("AW: [EXT] Invoice"), "(EXT) Invoice");
        assert_eq!(clean_headline("Re:"), UNTITLED_HEADLINE);
        assert_eq!(clean_headline("   "), UNTITLED_HEADLINE);
        assert_eq!(
            clean_headline("Ready: the staging move"),
            "Ready: the staging move",
            "only reply and forward markers come off"
        );
        assert_eq!(clean_headline("a [id:: x] b"), "a (id:: x) b");
    }

    #[test]
    fn markers_and_control_characters_never_survive_the_cleaning() {
        assert_eq!(
            clean_headline("Re: Plan %% ai:end %%\nnext\u{0}\u{1b}[31m"),
            "Plan % ai:end % next (31m"
        );
        assert_eq!(clean_headline("%%%%"), "%");
        assert_eq!(clean_headline("50% off"), "50% off");
        assert_eq!(
            sender_label(&signal("x", "", Some("Ops %% ai:end %%\u{7}"))),
            "Ops % ai:end %"
        );
        assert_eq!(line_text("a\u{2028}b %% c\td"), "a b % c d");
        assert_eq!(line_text("keeps [[links]]"), "keeps [[links]]");
        assert_eq!(
            line_text("Run `rm` <!-- hidden --> now"),
            "Run 'rm' < !-- hidden -- > now"
        );
        assert_eq!(clean_headline("Re: <!--x-->`y`"), "< !--x-- >'y'");
    }

    #[test]
    fn the_ticket_comes_from_the_subject_before_the_excerpt() {
        let pattern = TicketPattern::new(r"(WEB|ACME)-\d+").unwrap();
        let both = signal("WEB-151 tiers", "see ACME-9 too", None);
        assert_eq!(
            matched_ticket(&both, Some(&pattern)).as_deref(),
            Some("WEB-151")
        );
        let excerpt_only = signal("Tiers", "about ACME-9 and WEB-1", None);
        assert_eq!(
            matched_ticket(&excerpt_only, Some(&pattern)).as_deref(),
            Some("ACME-9")
        );
        assert_eq!(
            matched_ticket(&signal("Lunch", "", None), Some(&pattern)),
            None
        );
        assert_eq!(matched_ticket(&both, None), None, "no pattern, no guess");
    }

    #[test]
    fn the_sender_is_the_name_else_the_address() {
        assert_eq!(
            sender_label(&signal("x", "", Some("  Lena   Ortiz "))),
            "Lena Ortiz"
        );
        assert_eq!(
            sender_label(&signal("x", "", Some(" "))),
            "lena@example.test"
        );
        assert_eq!(
            sender_label(&signal("x", "", Some("Ops [bot]"))),
            "Ops (bot)"
        );
    }
}
