//! `Leave.md`: the leave and public holidays of the user, one note in the
//! personal workspace (the life workspace, or the one a work workspace names
//! as `leave_source`).
//!
//! One date or inclusive range per line in ISO form, optionally as a list
//! item and optionally followed by a label:
//!
//! ```text
//! ## Leave
//! - 2026-10-20 Dentist
//! - 2026-12-21..2026-12-24 Winter break
//!
//! ## Holidays
//! - 2026-12-25 Christmas Day
//! ```
//!
//! Lines under a heading that says "holiday" are public holidays; every other
//! line is leave. Headings, blank lines, frontmatter and code are skipped;
//! any other line is ignored and reported as a warning so the user can fix
//! it. The note is only read: [`LeaveNote::render`] gives it back byte for byte.

use crate::date::Date;
use crate::scan::{Document, LineKind};

/// The note's file name at the workspace root.
pub const LEAVE_NOTE_NAME: &str = "Leave.md";

/// The longest range one line may span, in days.
pub const MAX_RANGE_DAYS: i64 = 366;

/// Whether an entry is leave or a public holiday.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LeaveKind {
    /// The user's own leave.
    Leave,
    /// A public holiday.
    Holiday,
}

/// One line of dates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaveEntry {
    /// The first day.
    pub from: Date,
    /// The last day, included; the same as `from` for a single date.
    pub to: Date,
    /// Leave or holiday.
    pub kind: LeaveKind,
    /// The text after the dates, `None` when there is none.
    pub label: Option<String>,
    /// The 1-based line number.
    pub line: usize,
}

/// A line the parser could not read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaveWarning {
    /// The 1-based line number.
    pub line: usize,
    /// The line as written.
    pub text: String,
    /// Why it was ignored.
    pub reason: String,
}

/// A parsed `Leave.md`.
#[derive(Debug, Clone)]
pub struct LeaveNote {
    document: Document,
    entries: Vec<LeaveEntry>,
    warnings: Vec<LeaveWarning>,
}

impl LeaveNote {
    /// Reads the note's text.
    #[must_use]
    pub fn parse(text: &str) -> LeaveNote {
        let document = Document::parse(text);
        let mut kind = LeaveKind::Leave;
        let mut entries = Vec::new();
        let mut warnings = Vec::new();
        for (index, line) in document.lines().iter().enumerate() {
            let number = index + 1;
            match &line.kind {
                LineKind::Heading(heading) => {
                    kind = if heading.title.to_lowercase().contains("holiday") {
                        LeaveKind::Holiday
                    } else {
                        LeaveKind::Leave
                    };
                }
                LineKind::ListItem(_) | LineKind::Text | LineKind::Task(_) => {
                    match parse_entry(&line.text, kind, number) {
                        Ok(entry) => entries.push(entry),
                        Err(reason) => warnings.push(LeaveWarning {
                            line: number,
                            text: line.text.clone(),
                            reason,
                        }),
                    }
                }
                _ => {}
            }
        }
        LeaveNote {
            document,
            entries,
            warnings,
        }
    }

    /// Every line of dates, in note order.
    #[must_use]
    pub fn entries(&self) -> &[LeaveEntry] {
        &self.entries
    }

    /// The lines that were ignored, in note order.
    #[must_use]
    pub fn warnings(&self) -> &[LeaveWarning] {
        &self.warnings
    }

    /// Every day of leave, by date, each once.
    #[must_use]
    pub fn leave_days(&self) -> Vec<Date> {
        self.days_of(LeaveKind::Leave)
    }

    /// Every public holiday, by date, each once.
    #[must_use]
    pub fn holidays(&self) -> Vec<Date> {
        self.days_of(LeaveKind::Holiday)
    }

    /// The note as it was read, byte for byte.
    #[must_use]
    pub fn render(&self) -> String {
        self.document.render()
    }

    fn days_of(&self, kind: LeaveKind) -> Vec<Date> {
        let mut days: Vec<Date> = self
            .entries
            .iter()
            .filter(|entry| entry.kind == kind)
            .flat_map(|entry| {
                (entry.from.days_since_epoch()..=entry.to.days_since_epoch())
                    .map(Date::from_days_since_epoch)
            })
            .collect();
        days.sort();
        days.dedup();
        days
    }
}

/// One line as an entry, or why it is not one.
fn parse_entry(text: &str, kind: LeaveKind, line: usize) -> Result<LeaveEntry, String> {
    let body = text.trim();
    let body = ["- ", "* ", "+ "]
        .iter()
        .find_map(|marker| body.strip_prefix(marker))
        .unwrap_or(body)
        .trim_start();
    let (dates, rest) = body.split_once(char::is_whitespace).unwrap_or((body, ""));
    let (from, to) = match dates.split_once("..") {
        Some((from, to)) => (from, to),
        None => (dates, dates),
    };
    let (Some(from), Some(to)) = (Date::parse(from), Date::parse(to)) else {
        return Err("expected a date or a range such as 2026-12-21..2026-12-24".to_owned());
    };
    if to < from {
        return Err("the range ends before it starts".to_owned());
    }
    if to.days_since_epoch() - from.days_since_epoch() >= MAX_RANGE_DAYS {
        return Err(format!("a range spans at most {MAX_RANGE_DAYS} days"));
    }
    let label = rest
        .trim()
        .trim_start_matches(['-', '–', '—', '·', ':'])
        .trim();
    Ok(LeaveEntry {
        from,
        to,
        kind,
        label: (!label.is_empty()).then(|| label.to_owned()),
        line,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(text: &str) -> Date {
        Date::parse(text).unwrap()
    }

    const NOTE: &str = "---\ntags: [leave]\n---\n# Leave\n\nOne date or range per line.\n\n## Leave\n- 2026-10-20 Dentist\n- 2026-12-21..2026-12-23 — Winter break\r\n* 2026-11-02\n- 2026-13-01 typo\n2026-11-05..2026-11-04 backwards\n\n## Public holidays\n- 2026-12-25 Christmas Day\n- 2026-12-21 overlaps the break\n```\n2026-01-01 in code\n```\n";

    #[test]
    fn reads_leave_holidays_and_labels() {
        let note = LeaveNote::parse(NOTE);
        let entries: Vec<(String, LeaveKind, Option<&str>)> = note
            .entries()
            .iter()
            .map(|entry| (entry.from.to_string(), entry.kind, entry.label.as_deref()))
            .collect();
        let expected = [
            ("2026-10-20", LeaveKind::Leave, Some("Dentist")),
            ("2026-12-21", LeaveKind::Leave, Some("Winter break")),
            ("2026-11-02", LeaveKind::Leave, None),
            ("2026-12-25", LeaveKind::Holiday, Some("Christmas Day")),
            ("2026-12-21", LeaveKind::Holiday, Some("overlaps the break")),
        ]
        .map(|(from, kind, label)| (from.to_owned(), kind, label));
        assert_eq!(entries, expected);
        assert_eq!(note.entries()[1].to, day("2026-12-23"));
        assert_eq!(
            note.leave_days(),
            [
                day("2026-10-20"),
                day("2026-11-02"),
                day("2026-12-21"),
                day("2026-12-22"),
                day("2026-12-23"),
            ]
        );
        assert_eq!(note.holidays(), [day("2026-12-21"), day("2026-12-25")]);
    }

    #[test]
    fn unknown_lines_are_ignored_with_a_warning() {
        let note = LeaveNote::parse(NOTE);
        let warned: Vec<(usize, &str)> = note
            .warnings()
            .iter()
            .map(|warning| (warning.line, warning.text.as_str()))
            .collect();
        assert_eq!(
            warned,
            [
                (6, "One date or range per line."),
                (12, "- 2026-13-01 typo"),
                (13, "2026-11-05..2026-11-04 backwards"),
            ]
        );
        assert!(note.warnings()[2].reason.contains("ends before"));
        let too_long = LeaveNote::parse("- 2026-01-01..2027-06-01 sabbatical\n");
        assert!(too_long.entries().is_empty());
        assert_eq!(too_long.warnings().len(), 1);
    }

    #[test]
    fn the_note_renders_back_byte_for_byte() {
        for text in [
            NOTE,
            "",
            "- 2026-10-20",
            "\n\n- 2026-10-20 x\r\n# Holidays\n- 2026-12-25",
        ] {
            assert_eq!(LeaveNote::parse(text).render(), text);
        }
    }
}
