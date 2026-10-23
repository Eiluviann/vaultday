//! The day log: formatting one `Log/YYYY-MM-DD.md` line. Reading today's
//! lines is [`crate::Vault::log_lines_on`]; appending one is
//! [`crate::VaultWriter::append_log`], which uses the functions here.
//!
//! A line reads `- HH:MM <verb> · <text> [story:: …] [kind:: …] [id:: …] [task:: …] [agent:: …]`
//! with only the fields that were given. Lines the app writes are facts and
//! carry no provenance marker; a line a model wrote names its author in
//! `[agent:: …]`, which is how the provenance tooling attributes day-log lines.

use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::date::{Date, Time};
use crate::note::LogKind;
use crate::scan::inline_fields;

/// The separator between the verb and the text.
pub const VERB_SEPARATOR: &str = "·";

/// One line to append to a day log. Build it with [`LogAppend::new`] and the
/// `with_*` methods.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogAppend {
    /// The time prefix.
    pub time: Time,
    /// The kind, written as `[kind:: …]` and used as the verb when none is set.
    pub kind: Option<LogKind>,
    /// The verb in front of the separator; defaults to the kind's token.
    pub verb: Option<String>,
    /// What happened, on one line, without inline fields or pipes.
    pub text: String,
    /// `[story:: …]`: a story number such as `WEB-142`.
    pub story: Option<String>,
    /// `[id:: …]`: an id for the line itself.
    pub id: Option<String>,
    /// `[task:: …]`: the task id the line is about.
    pub task: Option<String>,
    /// `[agent:: …]`: the model that wrote the line. The provenance tooling
    /// attributes a day-log line by this field instead of by markers.
    pub agent: Option<String>,
}

/// Why a log line could not be formatted.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LogFormatError {
    /// The text is empty or spans lines.
    #[error("log text must be one non-empty line")]
    InvalidText,
    /// The text holds `[key:: value]` syntax that would parse as a field.
    #[error("log text must not contain inline fields such as [key:: value]")]
    FieldInText,
    /// The text holds a pipe, which the log grammar forbids.
    #[error("log text must not contain a pipe")]
    PipeInText,
    /// The verb is empty, spans lines or holds the separator.
    #[error("log verb must be one word or phrase without `·`")]
    InvalidVerb,
    /// Only the six kinds of the day-log grammar are written.
    #[error("log kind {0:?} is not one of session, done, decision, blocker, status, note")]
    UnknownKind(String),
    /// A field value is empty, spans lines or holds a closing bracket.
    #[error("log field {0} must be a non-empty single-line value without `]`")]
    InvalidField(&'static str),
}

impl LogAppend {
    /// A line with a time and a text; add the rest with the `with_*` methods.
    #[must_use]
    pub fn new(time: Time, text: impl Into<String>) -> LogAppend {
        LogAppend {
            time,
            kind: None,
            verb: None,
            text: text.into(),
            story: None,
            id: None,
            task: None,
            agent: None,
        }
    }

    /// Sets the kind.
    #[must_use]
    pub fn with_kind(mut self, kind: LogKind) -> LogAppend {
        self.kind = Some(kind);
        self
    }

    /// Sets the verb shown before the separator.
    #[must_use]
    pub fn with_verb(mut self, verb: impl Into<String>) -> LogAppend {
        self.verb = Some(verb.into());
        self
    }

    /// Sets the story reference.
    #[must_use]
    pub fn with_story(mut self, story: impl Into<String>) -> LogAppend {
        self.story = Some(story.into());
        self
    }

    /// Sets the line's own id.
    #[must_use]
    pub fn with_id(mut self, id: impl Into<String>) -> LogAppend {
        self.id = Some(id.into());
        self
    }

    /// Sets the task reference.
    #[must_use]
    pub fn with_task(mut self, task: impl Into<String>) -> LogAppend {
        self.task = Some(task.into());
        self
    }

    /// Marks the line as written by the model `agent`.
    #[must_use]
    pub fn with_agent(mut self, agent: impl Into<String>) -> LogAppend {
        self.agent = Some(agent.into());
        self
    }
}

/// Formats the line, without a terminator.
///
/// # Errors
/// [`LogFormatError`] when the text, verb, kind or a field value breaks the
/// day-log grammar.
pub fn format_log_line(entry: &LogAppend) -> Result<String, LogFormatError> {
    validate_text(&entry.text)?;
    let kind_token = match &entry.kind {
        None => None,
        Some(LogKind::Other(other)) => return Err(LogFormatError::UnknownKind(other.clone())),
        Some(kind) => Some(kind.as_str()),
    };
    let verb = entry.verb.as_deref().map(str::trim).or(kind_token);
    if let Some(verb) = verb
        && (verb.is_empty() || verb.contains('\n') || verb.contains(VERB_SEPARATOR))
    {
        return Err(LogFormatError::InvalidVerb);
    }
    let mut line = format!("- {}", entry.time);
    match verb {
        Some(verb) => line.push_str(&format!(" {verb} {VERB_SEPARATOR} {}", entry.text.trim())),
        None => line.push_str(&format!(" {}", entry.text.trim())),
    }
    let fields = [
        ("story", entry.story.as_deref()),
        ("kind", kind_token),
        ("id", entry.id.as_deref()),
        ("task", entry.task.as_deref()),
        ("agent", entry.agent.as_deref()),
    ];
    for (key, value) in fields {
        let Some(value) = value else {
            continue;
        };
        let value = value.trim();
        if value.is_empty() || value.contains(['\n', '\r', ']']) {
            return Err(LogFormatError::InvalidField(key));
        }
        line.push_str(&format!(" [{key}:: {value}]"));
    }
    Ok(line)
}

/// What a typed log line says: its kind and the text left once the kind word is read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogShorthand {
    /// The kind the line starts with, `Note` when it names none.
    pub kind: LogKind,
    /// The text after the kind word, trimmed.
    pub text: String,
}

/// Reads `decision: keep the redirect group dev only`: a leading kind word and
/// a colon set the kind (`session`, `done`, `decision`, `blocker` or
/// `blocked`, `status`, `note`; case ignored). Anything else is a note and
/// the whole input is the text.
#[must_use]
pub fn parse_log_shorthand(input: &str) -> LogShorthand {
    let trimmed = input.trim();
    let kind = trimmed.split_once(':').and_then(|(word, rest)| {
        let kind = match word.trim().to_ascii_lowercase().as_str() {
            "session" => LogKind::Session,
            "done" => LogKind::Done,
            "decision" => LogKind::Decision,
            "blocker" | "blocked" => LogKind::Blocker,
            "status" => LogKind::Status,
            "note" => LogKind::Note,
            _ => return None,
        };
        Some((kind, rest.trim()))
    });
    match kind {
        Some((kind, text)) => LogShorthand {
            kind,
            text: text.to_owned(),
        },
        None => LogShorthand {
            kind: LogKind::Note,
            text: trimmed.to_owned(),
        },
    }
}

/// The path of a day's log relative to the workspace root.
#[must_use]
pub fn day_log_path(log_folder: &str, date: Date) -> PathBuf {
    Path::new(log_folder).join(format!("{date}.md"))
}

/// The content of a day log that has no lines yet, ending after `## Log`.
#[must_use]
pub fn new_day_log_header(date: Date) -> String {
    format!("---\ntype: log\ndate: {date}\n---\n# {date}\n\n## Log\n")
}

fn validate_text(text: &str) -> Result<(), LogFormatError> {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.contains(['\n', '\r']) {
        return Err(LogFormatError::InvalidText);
    }
    if trimmed.contains('|') {
        return Err(LogFormatError::PipeInText);
    }
    if !inline_fields(trimmed).is_empty() {
        return Err(LogFormatError::FieldInText);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        LogAppend, LogFormatError, LogShorthand, day_log_path, format_log_line, new_day_log_header,
        parse_log_shorthand,
    };
    use crate::date::{Date, Time};
    use crate::note::LogKind;
    use std::path::Path;

    fn at(hour: u8, minute: u8) -> Time {
        Time::new(hour, minute).unwrap()
    }

    #[test]
    fn formats_every_field_in_the_fixed_order() {
        let entry = LogAppend::new(at(9, 5), "Use a single-page checkout")
            .with_kind(LogKind::Decision)
            .with_story("WEB-142")
            .with_id("log-7")
            .with_task("task-5");
        assert_eq!(
            format_log_line(&entry).unwrap(),
            "- 09:05 decision · Use a single-page checkout [story:: WEB-142] [kind:: decision] [id:: log-7] [task:: task-5]"
        );
    }

    #[test]
    fn a_model_line_names_its_agent_last() {
        let entry = LogAppend::new(at(9, 5), "Use a single-page checkout")
            .with_kind(LogKind::Decision)
            .with_task("task-5")
            .with_agent("claude-fable-5-1");
        assert_eq!(
            format_log_line(&entry).unwrap(),
            "- 09:05 decision · Use a single-page checkout [kind:: decision] [task:: task-5] [agent:: claude-fable-5-1]"
        );
        let broken = entry.with_agent("claude]");
        assert_eq!(
            format_log_line(&broken),
            Err(LogFormatError::InvalidField("agent"))
        );
    }

    #[test]
    fn writes_only_the_fields_given() {
        let kind_only = LogAppend::new(at(14, 30), "Asked Lee again").with_kind(LogKind::Note);
        assert_eq!(
            format_log_line(&kind_only).unwrap(),
            "- 14:30 note · Asked Lee again [kind:: note]"
        );
        let story_and_task = LogAppend::new(at(23, 59), "Gateway keys arrived")
            .with_verb("unblocked")
            .with_story("ACME-9")
            .with_task("task-12");
        assert_eq!(
            format_log_line(&story_and_task).unwrap(),
            "- 23:59 unblocked · Gateway keys arrived [story:: ACME-9] [task:: task-12]"
        );
        let bare = LogAppend::new(at(0, 0), "Plain fact");
        assert_eq!(format_log_line(&bare).unwrap(), "- 00:00 Plain fact");
    }

    #[test]
    fn an_explicit_verb_wins_over_the_kind_token() {
        let entry = LogAppend::new(at(8, 0), "Shipped")
            .with_verb("released")
            .with_kind(LogKind::Done);
        assert_eq!(
            format_log_line(&entry).unwrap(),
            "- 08:00 released · Shipped [kind:: done]"
        );
    }

    #[test]
    fn every_kind_token_formats() {
        for (kind, token) in [
            (LogKind::Session, "session"),
            (LogKind::Done, "done"),
            (LogKind::Decision, "decision"),
            (LogKind::Blocker, "blocker"),
            (LogKind::Status, "status"),
            (LogKind::Note, "note"),
        ] {
            let line = format_log_line(&LogAppend::new(at(10, 0), "x").with_kind(kind)).unwrap();
            assert_eq!(line, format!("- 10:00 {token} · x [kind:: {token}]"));
        }
    }

    #[test]
    fn rejects_text_and_values_that_break_the_grammar() {
        let rejected = |entry: LogAppend| format_log_line(&entry).unwrap_err();
        assert_eq!(
            rejected(LogAppend::new(at(1, 0), "  ")),
            LogFormatError::InvalidText
        );
        assert_eq!(
            rejected(LogAppend::new(at(1, 0), "a\nb")),
            LogFormatError::InvalidText
        );
        assert_eq!(
            rejected(LogAppend::new(at(1, 0), "a | b")),
            LogFormatError::PipeInText
        );
        assert_eq!(
            rejected(LogAppend::new(at(1, 0), "see [due:: 2026-01-01]")),
            LogFormatError::FieldInText
        );
        assert_eq!(
            rejected(LogAppend::new(at(1, 0), "x").with_verb("a · b")),
            LogFormatError::InvalidVerb
        );
        assert_eq!(
            rejected(LogAppend::new(at(1, 0), "x").with_kind(LogKind::Other("idea".to_owned()))),
            LogFormatError::UnknownKind("idea".to_owned())
        );
        assert_eq!(
            rejected(LogAppend::new(at(1, 0), "x").with_story("A]B")),
            LogFormatError::InvalidField("story")
        );
        assert_eq!(
            rejected(LogAppend::new(at(1, 0), "x").with_task("")),
            LogFormatError::InvalidField("task")
        );
    }

    #[test]
    fn the_day_log_path_and_header_follow_the_convention() {
        let date = Date::new(2026, 10, 23).unwrap();
        assert_eq!(day_log_path("Log", date), Path::new("Log/2026-10-23.md"));
        assert_eq!(
            day_log_path("Journal/Days", date),
            Path::new("Journal/Days/2026-10-23.md")
        );
        assert_eq!(
            new_day_log_header(date),
            "---\ntype: log\ndate: 2026-10-23\n---\n# 2026-10-23\n\n## Log\n"
        );
    }

    #[test]
    fn a_leading_kind_word_sets_the_kind() {
        assert_eq!(
            parse_log_shorthand("decision: keep the QA redirect group dev and test only"),
            LogShorthand {
                kind: LogKind::Decision,
                text: "keep the QA redirect group dev and test only".to_owned(),
            }
        );
        assert_eq!(
            parse_log_shorthand("Blocked:  waiting on Dana ").kind,
            LogKind::Blocker
        );
        assert_eq!(parse_log_shorthand("blocker: x").kind, LogKind::Blocker);
        assert_eq!(parse_log_shorthand("done: shipped").kind, LogKind::Done);
        assert_eq!(parse_log_shorthand("status: ready").kind, LogKind::Status);
        assert_eq!(
            parse_log_shorthand("session: pairing").kind,
            LogKind::Session
        );
    }

    #[test]
    fn any_other_text_is_a_note_and_keeps_its_colons() {
        assert_eq!(
            parse_log_shorthand("  call at 10:30 about X  "),
            LogShorthand {
                kind: LogKind::Note,
                text: "call at 10:30 about X".to_owned(),
            }
        );
        assert_eq!(
            parse_log_shorthand("meeting: notes follow"),
            LogShorthand {
                kind: LogKind::Note,
                text: "meeting: notes follow".to_owned(),
            }
        );
        assert_eq!(parse_log_shorthand("note: x"), parse_log_shorthand("x"));
        assert_eq!(parse_log_shorthand("decision:").text, "");
    }

    #[test]
    fn shorthand_feeds_the_formatter_byte_for_byte() {
        let shorthand =
            parse_log_shorthand("decision: keep the QA redirect group dev and test only");
        let line = format_log_line(
            &LogAppend::new(at(9, 48), shorthand.text)
                .with_kind(shorthand.kind)
                .with_story("WEB-142")
                .with_task("task-2"),
        )
        .unwrap();
        assert_eq!(
            line,
            "- 09:48 decision · keep the QA redirect group dev and test only [story:: WEB-142] [kind:: decision] [task:: task-2]"
        );
    }
}
