//! The quick-add grammar: one typed line becomes a [`ParsedEntry`] for the
//! confirmation popover, which shows what was understood (kind, cleaned text,
//! story, day, estimate, section, the id it will get) before anything is written.
//!
//! The grammar is local and deterministic, with no model behind it. It works
//! on whitespace-separated tokens and takes recognised ones out of the text:
//!
//! - `/workspace` routes to a workspace by name or id; an unknown one stays in
//!   the text with a warning;
//! - a token matching any workspace's `ticket_pattern` (`WEB-142`) routes to
//!   that story, known or not; otherwise a story name or alias found in the
//!   text routes by name and the text keeps it;
//! - `15:00-16:30`, `15:00 – 16:30`, `from 15:00 to 16:30`, `at 9-10`, `at 15:00`,
//!   `3pm` or a bare `15:00` set the time and make the entry an event; a range
//!   of bare hours (`9-10`) counts only after `at` or `from`, so chapter and
//!   page ranges stay text;
//! - `30m`, `45min`, `1h30`, `1h30m`, `2h`, `1.5h` set the estimate;
//! - `today`, `tomorrow`, a weekday name (the coming one), `next <weekday>`
//!   (in next week), `next week` (its Monday) or an ISO date set the day, with
//!   an `on` before them taken too;
//! - a trailing `waiting on <name>` or `waiting for <name>`, or `@name`
//!   anywhere, routes to Pending Input with that name; a `waiting on <name>`
//!   in the middle of the line routes the same way and stays in the text.
//!
//! The first value of each kind wins; a later one is dropped with a
//! [`ParseWarning`]. A line left without text is refused.

use std::sync::LazyLock;
use std::time::Duration;

use jiff::civil::{Date, Time, Weekday};
use regex::Regex;

use crate::error::{DomainError, Result};
use crate::estimate::Estimate;
use crate::task::{Section, StoryNumber, TaskId, TaskStatus, VaultTask};
use crate::ticket::TicketPattern;
use crate::workspace::WorkspaceId;

/// Whether the line adds a task to a story or an event to the calendar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntryKind {
    /// A task line in a story note.
    Task,
    /// A calendar event; a time range or an `at` time makes one.
    Event,
}

/// The time an event was given.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TimeRange {
    /// When it starts.
    pub start: Time,
    /// When it ends: the range's end, or the start plus the estimate, or unknown.
    pub end: Option<Time>,
}

/// How the story was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StoryMatch {
    /// A ticket number in the line.
    TicketNumber,
    /// The story's name or an alias in the text.
    Name,
}

/// The story the entry routes to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoryRoute {
    /// The story's number.
    pub number: StoryNumber,
    /// How it was found.
    pub matched_by: StoryMatch,
    /// Whether the number is in the supplied story list.
    pub is_known: bool,
}

/// A story the grammar may route to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoryRef {
    /// The story's number.
    pub number: StoryNumber,
    /// Its title.
    pub name: String,
    /// Other names it goes by.
    pub aliases: Vec<String>,
    /// The workspace it lives in, when known.
    pub workspace: Option<WorkspaceId>,
}

/// A workspace `/name` may route to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceRef {
    /// The id.
    pub id: WorkspaceId,
    /// The display name.
    pub name: String,
}

/// What the grammar needs to know about the vault to route a line.
#[derive(Debug, Clone)]
pub struct QuickAddContext<'a> {
    /// The current date, for day words.
    pub today: Date,
    /// The `ticket_pattern` of every workspace.
    pub ticket_patterns: &'a [TicketPattern],
    /// The stories a line may route to.
    pub stories: &'a [StoryRef],
    /// The workspaces `/name` may route to.
    pub workspaces: &'a [WorkspaceRef],
    /// The id the entry gets when saved, from the caller's counter.
    pub next_id: TaskId,
}

/// Something the grammar understood but could not use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseWarning {
    /// A ticket number matched a pattern but no story in the list.
    UnknownTicket {
        /// The number.
        number: StoryNumber,
    },
    /// A `/workspace` token named no workspace in the list.
    UnknownWorkspace {
        /// The name after the slash.
        name: String,
    },
    /// A second `/workspace`; the first one won.
    ExtraWorkspaceDropped {
        /// The name after the slash.
        name: String,
    },
    /// A second estimate; the first one won.
    ExtraEstimateDropped {
        /// The token as typed.
        token: String,
    },
    /// A second time; the first one won.
    ExtraTimeDropped {
        /// The token as typed.
        token: String,
    },
    /// A second day word; the first one won.
    ExtraDayDropped {
        /// The token as typed.
        token: String,
    },
}

/// The popover's contents: what one line was understood as.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedEntry {
    /// Task or event.
    pub kind: EntryKind,
    /// The text with every recognised token taken out and whitespace collapsed.
    pub text: String,
    /// The story the entry routes to, if any.
    pub story: Option<StoryRoute>,
    /// The workspace: from `/workspace`, else from the matched story, else none.
    pub workspace: Option<WorkspaceId>,
    /// The day: a day word in the line, else today.
    pub day: Date,
    /// The time, present for events only.
    pub time: Option<TimeRange>,
    /// The estimate, if one was given.
    pub estimate: Option<Estimate>,
    /// Next Steps unless the line names someone to wait on.
    pub section: Section,
    /// Who the entry waits on, when the section is Pending Input.
    pub waiting_on: Option<String>,
    /// The id the entry gets when saved.
    pub id: TaskId,
    /// What was understood but not used.
    pub warnings: Vec<ParseWarning>,
}

impl ParsedEntry {
    /// The task line this entry saves as, created on `created`; `None` for an event.
    pub fn vault_task(&self, created: Date) -> Option<VaultTask> {
        if self.kind == EntryKind::Event {
            return None;
        }
        Some(VaultTask {
            id: self.id.clone(),
            text: self.text.clone(),
            status: TaskStatus::Open,
            section: self.section,
            story: self.story.as_ref().map(|route| route.number.clone()),
            created: Some(created),
            scheduled: Some(self.day),
            due: None,
            estimate: self.estimate,
            assigned_to: self.waiting_on.clone(),
            completion: None,
            parent: None,
        })
    }
}

/// A token of the line and whether a rule has taken it out of the text.
#[derive(Debug)]
struct Token<'a> {
    text: &'a str,
    is_taken: bool,
}

/// The values the passes collect before the entry is assembled.
#[derive(Debug, Default)]
struct Collected {
    workspace: Option<WorkspaceId>,
    story: Option<StoryRoute>,
    time: Option<TimeRange>,
    estimate: Option<Estimate>,
    day: Option<Date>,
    waiting_on: Option<String>,
    warnings: Vec<ParseWarning>,
}

static TIME: LazyLock<Regex> = LazyLock::new(|| literal(r"^(\d{1,2})(?::(\d{2}))?(am|pm)?$"));
static TIME_RANGE: LazyLock<Regex> = LazyLock::new(|| {
    literal(r"^(\d{1,2}(?::\d{2})?(?:am|pm)?)[-–](\d{1,2}(?::\d{2})?(?:am|pm)?)$")
});
static HOURS_ESTIMATE: LazyLock<Regex> =
    LazyLock::new(|| literal(r"^(\d+)(?:\.(\d+))?h(?:(\d{1,2})m?)?$"));
static MINUTES_ESTIMATE: LazyLock<Regex> = LazyLock::new(|| literal(r"^(\d+)(?:m|min)$"));
static ISO_DATE: LazyLock<Regex> = LazyLock::new(|| literal(r"^\d{4}-\d{2}-\d{2}$"));

const RANGE_JOINERS: [&str; 3] = ["-", "–", "to"];
const PUNCTUATION: [char; 7] = [',', '.', ';', ':', '!', '?', ')'];

/// Parses one quick-add line.
///
/// # Errors
/// [`DomainError::EmptyEntry`] when nothing is left as text once the
/// recognised tokens are taken out, including for an empty line or a time alone.
/// [`DomainError::TimeOutOfRange`] when a day word leaves the calendar.
pub fn parse(line: &str, context: &QuickAddContext<'_>) -> Result<ParsedEntry> {
    let mut tokens: Vec<Token<'_>> = line
        .split_whitespace()
        .map(|text| Token {
            text,
            is_taken: false,
        })
        .collect();
    let mut collected = Collected::default();

    take_workspace(&mut tokens, context, &mut collected);
    // Times and estimates have a fixed shape and go first, so a loose
    // user-supplied `ticket_pattern` (`\d+`) cannot swallow `at 9` or `30m`.
    take_times(&mut tokens, &mut collected);
    take_estimates(&mut tokens, &mut collected);
    take_ticket(&mut tokens, context, &mut collected);
    take_days(&mut tokens, context.today, &mut collected)?;
    take_waiting_on(&mut tokens, &mut collected);

    let text = remaining_text(&tokens);
    if text.is_empty() {
        return Err(DomainError::EmptyEntry);
    }
    if collected.story.is_none() {
        collected.story = story_by_name(&text, context.stories);
    }
    let workspace = collected.workspace.or_else(|| {
        let route = collected.story.as_ref()?;
        context
            .stories
            .iter()
            .find(|story| story.number == route.number)?
            .workspace
            .clone()
    });
    let time = collected.time.map(|range| TimeRange {
        end: range
            .end
            .or_else(|| end_from_estimate(range.start, collected.estimate)),
        ..range
    });

    Ok(ParsedEntry {
        kind: if time.is_some() {
            EntryKind::Event
        } else {
            EntryKind::Task
        },
        text,
        story: collected.story,
        workspace,
        day: collected.day.unwrap_or(context.today),
        time,
        estimate: collected.estimate,
        section: if collected.waiting_on.is_some() {
            Section::PendingInput
        } else {
            Section::NextSteps
        },
        waiting_on: collected.waiting_on,
        id: context.next_id.clone(),
        warnings: collected.warnings,
    })
}

fn take_workspace(tokens: &mut [Token<'_>], context: &QuickAddContext<'_>, out: &mut Collected) {
    for token in tokens.iter_mut().filter(|token| !token.is_taken) {
        let Some(name) = token.text.strip_prefix('/') else {
            continue;
        };
        let name = trim_punctuation(name);
        if name.is_empty() || name.contains('/') {
            continue;
        }
        let found = context.workspaces.iter().find(|workspace| {
            workspace.name.eq_ignore_ascii_case(name)
                || workspace.id.as_str().eq_ignore_ascii_case(name)
        });
        let Some(workspace) = found else {
            out.warnings.push(ParseWarning::UnknownWorkspace {
                name: name.to_owned(),
            });
            continue;
        };
        token.is_taken = true;
        if out.workspace.is_some() {
            out.warnings.push(ParseWarning::ExtraWorkspaceDropped {
                name: name.to_owned(),
            });
        } else {
            out.workspace = Some(workspace.id.clone());
        }
    }
}

fn take_ticket(tokens: &mut [Token<'_>], context: &QuickAddContext<'_>, out: &mut Collected) {
    let Some(token) = tokens
        .iter_mut()
        .filter(|token| !token.is_taken)
        .find(|token| {
            let candidate = trim_punctuation(token.text);
            context
                .ticket_patterns
                .iter()
                .any(|pattern| pattern.matches(candidate))
        })
    else {
        return;
    };
    token.is_taken = true;
    let typed = trim_punctuation(token.text);
    let known = context
        .stories
        .iter()
        .find(|story| story.number.as_str().eq_ignore_ascii_case(typed));
    // A known story lends its own spelling, so the route matches the list exactly.
    let number = known.map_or_else(|| StoryNumber::new(typed), |story| story.number.clone());
    if known.is_none() {
        out.warnings.push(ParseWarning::UnknownTicket {
            number: number.clone(),
        });
    }
    out.story = Some(StoryRoute {
        number,
        matched_by: StoryMatch::TicketNumber,
        is_known: known.is_some(),
    });
}

fn take_times(tokens: &mut [Token<'_>], out: &mut Collected) {
    let mut index = 0;
    while index < tokens.len() {
        let Some((range, used)) = time_at(tokens, index) else {
            index += 1;
            continue;
        };
        let typed: Vec<&str> = tokens[index..index + used]
            .iter()
            .map(|token| token.text)
            .collect();
        for token in &mut tokens[index..index + used] {
            token.is_taken = true;
        }
        if out.time.is_some() {
            out.warnings.push(ParseWarning::ExtraTimeDropped {
                token: typed.join(" "),
            });
        } else {
            out.time = Some(range);
        }
        index += used;
    }
}

/// The time starting at `index`, with how many tokens it spans: `[at|from] T [-|–|to T]`.
fn time_at(tokens: &[Token<'_>], index: usize) -> Option<(TimeRange, usize)> {
    let text_at = |offset: usize| {
        tokens
            .get(index + offset)
            .filter(|token| !token.is_taken)
            .map(|token| token.text)
    };
    let first = text_at(0)?;
    let has_preposition = matches!(first.to_ascii_lowercase().as_str(), "at" | "from");
    let start_offset = usize::from(has_preposition);
    let start_text = trim_punctuation(text_at(start_offset)?);

    if let Some(range) = parse_time_range_token(start_text, has_preposition) {
        return Some((range, start_offset + 1));
    }
    let start = parse_time(start_text, has_preposition)?;
    let is_joined = text_at(start_offset + 1)
        .is_some_and(|joiner| RANGE_JOINERS.contains(&joiner.to_ascii_lowercase().as_str()));
    if is_joined
        && let Some(range) = text_at(start_offset + 2)
            .and_then(|text| range_between(start_text, trim_punctuation(text)))
    {
        return Some((range, start_offset + 3));
    }
    Some((TimeRange { start, end: None }, start_offset + 1))
}

/// `15:00-16:30`, `9-10:30`, `9am-11am`; two bare hours (`9-10`) only when `allow_bare_hours`.
fn parse_time_range_token(text: &str, allow_bare_hours: bool) -> Option<TimeRange> {
    let captures = TIME_RANGE.captures(text)?;
    let start_text = captures.get(1)?.as_str();
    let end_text = captures.get(2)?.as_str();
    if !allow_bare_hours && !has_time_marker(start_text) && !has_time_marker(end_text) {
        return None;
    }
    range_between(start_text, end_text)
}

/// The range from `start_text` to `end_text`, which must run forwards. A start
/// without am or pm takes the end's when that keeps it first: `2-4pm` is 14:00
/// to 16:00, while `11-1pm` stays 11:00 to 13:00.
fn range_between(start_text: &str, end_text: &str) -> Option<TimeRange> {
    let end = parse_time(end_text, true)?;
    let start_with_end_meridiem = meridiem_of(end_text)
        .filter(|_| meridiem_of(start_text).is_none())
        .and_then(|meridiem| parse_time(&format!("{start_text}{meridiem}"), true))
        .filter(|start| *start < end);
    let start = start_with_end_meridiem.or_else(|| parse_time(start_text, true))?;
    (end > start).then_some(TimeRange {
        start,
        end: Some(end),
    })
}

/// Whether a time token carries minutes or am/pm, which a bare hour lacks.
fn has_time_marker(text: &str) -> bool {
    text.contains(':') || meridiem_of(text).is_some()
}

/// The `am` or `pm` a time token ends with.
fn meridiem_of(text: &str) -> Option<&'static str> {
    let lowered = text.to_ascii_lowercase();
    if lowered.ends_with("am") {
        Some("am")
    } else if lowered.ends_with("pm") {
        Some("pm")
    } else {
        None
    }
}

/// `15:00`, `9:30`, `3pm`, `12:15am`; a bare hour only when `allow_bare_hour`.
fn parse_time(text: &str, allow_bare_hour: bool) -> Option<Time> {
    let lowered = text.to_ascii_lowercase();
    let captures = TIME.captures(&lowered)?;
    let hour: i8 = captures.get(1)?.as_str().parse().ok()?;
    let minute: i8 = captures
        .get(2)
        .map_or(Some(0), |found| found.as_str().parse().ok())?;
    let meridiem = captures.get(3).map(|found| found.as_str().to_owned());
    if captures.get(2).is_none() && meridiem.is_none() && !allow_bare_hour {
        return None;
    }
    let hour = match meridiem.as_deref() {
        None => (0..=23).contains(&hour).then_some(hour)?,
        Some(_) if !(1..=12).contains(&hour) => return None,
        Some("am") => hour % 12,
        Some(_) => hour % 12 + 12,
    };
    Time::new(hour, minute, 0, 0).ok()
}

fn take_estimates(tokens: &mut [Token<'_>], out: &mut Collected) {
    for token in tokens.iter_mut().filter(|token| !token.is_taken) {
        let Some(estimate) = parse_estimate(trim_punctuation(token.text)) else {
            continue;
        };
        token.is_taken = true;
        if out.estimate.is_some() {
            out.warnings.push(ParseWarning::ExtraEstimateDropped {
                token: token.text.to_owned(),
            });
        } else {
            out.estimate = Some(estimate);
        }
    }
}

/// Reads an estimate as typed or as written on a task line: `30m`, `45min`,
/// `2h`, `1h30`, `1h30m`, `1.5h`. `None` for anything else.
pub fn parse_estimate(text: &str) -> Option<Estimate> {
    let text = text.to_ascii_lowercase();
    if let Some(captures) = MINUTES_ESTIMATE.captures(&text) {
        let minutes: u64 = captures.get(1)?.as_str().parse().ok()?;
        return Some(Estimate::from_minutes(minutes));
    }
    let captures = HOURS_ESTIMATE.captures(&text)?;
    let hours: u64 = captures.get(1)?.as_str().parse().ok()?;
    let fraction_minutes = match captures.get(2) {
        Some(fraction) => {
            let digits = fraction.as_str();
            let numerator: u64 = digits.parse().ok()?;
            let denominator = 10_u64.checked_pow(u32::try_from(digits.len()).ok()?)?;
            numerator.checked_mul(60)? / denominator
        }
        None => 0,
    };
    let minutes: u64 = captures
        .get(3)
        .map_or(Some(0), |found| found.as_str().parse().ok())?;
    if minutes >= 60 || (captures.get(2).is_some() && captures.get(3).is_some()) {
        return None;
    }
    let total = hours
        .checked_mul(60)?
        .checked_add(fraction_minutes)?
        .checked_add(minutes)?;
    Some(Estimate::from_minutes(total))
}

fn take_days(tokens: &mut [Token<'_>], today: Date, out: &mut Collected) -> Result<()> {
    let mut index = 0;
    while index < tokens.len() {
        let Some((day, used)) = day_at(tokens, index, today)? else {
            index += 1;
            continue;
        };
        let typed: Vec<&str> = tokens[index..index + used]
            .iter()
            .map(|token| token.text)
            .collect();
        for token in &mut tokens[index..index + used] {
            token.is_taken = true;
        }
        if out.day.is_some() {
            out.warnings.push(ParseWarning::ExtraDayDropped {
                token: typed.join(" "),
            });
        } else {
            out.day = Some(day);
        }
        index += used;
    }
    Ok(())
}

/// The day word starting at `index`, with how many tokens it spans: `[on] <day>`.
fn day_at(tokens: &[Token<'_>], index: usize, today: Date) -> Result<Option<(Date, usize)>> {
    let text_at = |offset: usize| {
        tokens
            .get(index + offset)
            .filter(|token| !token.is_taken)
            .map(|token| trim_punctuation(token.text).to_ascii_lowercase())
    };
    let Some(first) = text_at(0) else {
        return Ok(None);
    };
    let has_preposition = first == "on";
    let offset = usize::from(has_preposition);
    let Some(word) = text_at(offset) else {
        return Ok(None);
    };
    let day = match word.as_str() {
        "today" => Some((today, 1)),
        "tomorrow" => Some((days_after(today, 1)?, 1)),
        "next" => match text_at(offset + 1).as_deref() {
            Some("week") => Some((next_monday(today)?, 2)),
            Some(second) => match weekday_named(second) {
                Some(weekday) => Some((weekday_in_next_week(today, weekday)?, 2)),
                None => None,
            },
            None => None,
        },
        _ if ISO_DATE.is_match(&word) => word.parse::<Date>().ok().map(|date| (date, 1)),
        _ => match weekday_named(&word) {
            Some(weekday) => Some((coming_weekday(today, weekday)?, 1)),
            None => None,
        },
    };
    Ok(day.map(|(date, used)| (date, used + offset)))
}

fn weekday_named(word: &str) -> Option<Weekday> {
    match word {
        "monday" => Some(Weekday::Monday),
        "tuesday" => Some(Weekday::Tuesday),
        "wednesday" => Some(Weekday::Wednesday),
        "thursday" => Some(Weekday::Thursday),
        "friday" => Some(Weekday::Friday),
        "saturday" => Some(Weekday::Saturday),
        "sunday" => Some(Weekday::Sunday),
        _ => None,
    }
}

/// The next `weekday` strictly after today: one to seven days ahead.
fn coming_weekday(today: Date, weekday: Weekday) -> Result<Date> {
    let ahead =
        (weekday.to_monday_zero_offset() - today.weekday().to_monday_zero_offset()).rem_euclid(7);
    let ahead = if ahead == 0 { 7 } else { ahead };
    days_after(today, u32::try_from(ahead).unwrap_or(7))
}

/// The Monday of the week after today's.
fn next_monday(today: Date) -> Result<Date> {
    let ahead = 7 - today.weekday().to_monday_zero_offset();
    days_after(today, u32::try_from(ahead).unwrap_or(7))
}

fn weekday_in_next_week(today: Date, weekday: Weekday) -> Result<Date> {
    let monday = next_monday(today)?;
    days_after(
        monday,
        u32::try_from(weekday.to_monday_zero_offset()).unwrap_or(0),
    )
}

fn days_after(date: Date, days: u32) -> Result<Date> {
    (0..days).try_fold(date, |current, _| {
        current.tomorrow().map_err(|_| DomainError::TimeOutOfRange)
    })
}

fn take_waiting_on(tokens: &mut [Token<'_>], out: &mut Collected) {
    let untaken: Vec<usize> = tokens
        .iter()
        .enumerate()
        .filter(|(_, token)| !token.is_taken)
        .map(|(index, _)| index)
        .collect();

    // `@name` anywhere: taken out of the text.
    for &index in &untaken {
        let text = tokens[index].text;
        let Some(name) = text.strip_prefix('@').or_else(|| text.strip_prefix('＠')) else {
            continue;
        };
        let name = trim_punctuation(name);
        if name.is_empty() {
            continue;
        }
        tokens[index].is_taken = true;
        if out.waiting_on.is_none() {
            out.waiting_on = Some(name.to_owned());
        }
    }

    // `waiting on <name>` / `waiting for <name>`: taken out only when it ends the line
    // once the `@name` tokens are gone.
    let untaken: Vec<usize> = tokens
        .iter()
        .enumerate()
        .filter(|(_, token)| !token.is_taken)
        .map(|(index, _)| index)
        .collect();
    let phrase = untaken.windows(3).find(|window| {
        let word = |position: usize| tokens[window[position]].text.to_ascii_lowercase();
        word(0) == "waiting" && matches!(word(1).as_str(), "on" | "for")
    });
    if let Some(window) = phrase {
        let name = trim_punctuation(tokens[window[2]].text).to_owned();
        if name.is_empty() {
            return;
        }
        let is_trailing = untaken.last() == Some(&window[2]);
        if is_trailing {
            for &index in window {
                tokens[index].is_taken = true;
            }
        }
        if out.waiting_on.is_none() {
            out.waiting_on = Some(name);
        }
    }
}

fn remaining_text(tokens: &[Token<'_>]) -> String {
    tokens
        .iter()
        .filter(|token| !token.is_taken)
        .map(|token| token.text)
        .collect::<Vec<&str>>()
        .join(" ")
}

/// The story whose name or alias appears in the text, longest name first.
fn story_by_name(text: &str, stories: &[StoryRef]) -> Option<StoryRoute> {
    let haystack = text.to_lowercase();
    stories
        .iter()
        .flat_map(|story| {
            std::iter::once(&story.name)
                .chain(&story.aliases)
                .map(move |name| (story, name.to_lowercase()))
        })
        .filter(|(_, name)| !name.is_empty() && contains_word(&haystack, name))
        .max_by_key(|(_, name)| name.chars().count())
        .map(|(story, _)| StoryRoute {
            number: story.number.clone(),
            matched_by: StoryMatch::Name,
            is_known: true,
        })
}

/// Whether `needle` occurs in `haystack` with no letter or digit touching it.
fn contains_word(haystack: &str, needle: &str) -> bool {
    haystack.match_indices(needle).any(|(start, found)| {
        let before = haystack[..start].chars().next_back();
        let after = haystack[start + found.len()..].chars().next();
        !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
    })
}

fn end_from_estimate(start: Time, estimate: Option<Estimate>) -> Option<Time> {
    let duration: Duration = estimate?.as_duration();
    if duration.is_zero() {
        return None;
    }
    let end = start.checked_add(duration).ok()?;
    (end > start).then_some(end)
}

fn trim_punctuation(token: &str) -> &str {
    token.trim_matches(PUNCTUATION).trim_start_matches('(')
}

/// Compiles a pattern written in this file; failing is a contract violation.
fn literal(pattern: &str) -> Regex {
    match Regex::new(pattern) {
        Ok(regex) => regex,
        Err(error) => panic!("the literal pattern `{pattern}` does not compile: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::{date, time};

    // A Thursday.
    const TODAY: Date = date(2026, 10, 22);

    fn patterns() -> Vec<TicketPattern> {
        vec![TicketPattern::new(r"(?i)(WEB|ACME)-\d+").unwrap()]
    }

    fn stories() -> Vec<StoryRef> {
        let acme = Some(WorkspaceId::new("acme"));
        vec![
            StoryRef {
                number: StoryNumber::new("WEB-142"),
                name: "Checkout redesign".to_owned(),
                aliases: vec!["Checkout v2".to_owned(), "Cart redesign".to_owned()],
                workspace: acme.clone(),
            },
            StoryRef {
                number: StoryNumber::new("WEB-157"),
                name: "Newsletter signup".to_owned(),
                aliases: Vec::new(),
                workspace: acme.clone(),
            },
            StoryRef {
                number: StoryNumber::new("ACME-9"),
                name: "Onboarding emails".to_owned(),
                aliases: Vec::new(),
                workspace: acme,
            },
        ]
    }

    fn workspaces() -> Vec<WorkspaceRef> {
        vec![
            WorkspaceRef {
                id: WorkspaceId::new("acme"),
                name: "Acme".to_owned(),
            },
            WorkspaceRef {
                id: WorkspaceId::new("personal"),
                name: "Personal".to_owned(),
            },
        ]
    }

    fn parse_line(line: &str) -> Result<ParsedEntry> {
        let patterns = patterns();
        let stories = stories();
        let workspaces = workspaces();
        parse(
            line,
            &QuickAddContext {
                today: TODAY,
                ticket_patterns: &patterns,
                stories: &stories,
                workspaces: &workspaces,
                next_id: TaskId::new("task-15"),
            },
        )
    }

    /// The expected entry: a task on today in Next Steps with the given text, unless overridden.
    fn expected(text: &str) -> ParsedEntry {
        ParsedEntry {
            kind: EntryKind::Task,
            text: text.to_owned(),
            story: None,
            workspace: None,
            day: TODAY,
            time: None,
            estimate: None,
            section: Section::NextSteps,
            waiting_on: None,
            id: TaskId::new("task-15"),
            warnings: Vec::new(),
        }
    }

    fn ticket(number: &str, is_known: bool) -> Option<StoryRoute> {
        Some(StoryRoute {
            number: StoryNumber::new(number),
            matched_by: StoryMatch::TicketNumber,
            is_known,
        })
    }

    fn by_name(number: &str) -> Option<StoryRoute> {
        Some(StoryRoute {
            number: StoryNumber::new(number),
            matched_by: StoryMatch::Name,
            is_known: true,
        })
    }

    fn event(start: (i8, i8), end: Option<(i8, i8)>) -> Option<TimeRange> {
        Some(TimeRange {
            start: time(start.0, start.1, 0, 0),
            end: end.map(|(hour, minute)| time(hour, minute, 0, 0)),
        })
    }

    fn acme() -> Option<WorkspaceId> {
        Some(WorkspaceId::new("acme"))
    }

    #[test]
    fn the_design_note_examples_parse_as_the_popover_shows_them() {
        let cases: Vec<(&str, ParsedEntry)> = vec![
            (
                "Wire the payment form WEB-142 2h tomorrow",
                ParsedEntry {
                    story: ticket("WEB-142", true),
                    workspace: acme(),
                    day: date(2026, 10, 23),
                    estimate: Some(Estimate::from_minutes(120)),
                    ..expected("Wire the payment form")
                },
            ),
            (
                "Sprint review 15:00-16:00",
                ParsedEntry {
                    kind: EntryKind::Event,
                    time: event((15, 0), Some((16, 0))),
                    ..expected("Sprint review")
                },
            ),
            (
                "Call Dana at 15:00 30m",
                ParsedEntry {
                    kind: EntryKind::Event,
                    time: event((15, 0), Some((15, 30))),
                    estimate: Some(Estimate::from_minutes(30)),
                    ..expected("Call Dana")
                },
            ),
            (
                "Review the newsletter signup copy 45m on friday",
                ParsedEntry {
                    story: by_name("WEB-157"),
                    workspace: acme(),
                    day: date(2026, 10, 23),
                    estimate: Some(Estimate::from_minutes(45)),
                    ..expected("Review the newsletter signup copy")
                },
            ),
            (
                "Final copy for the order summary waiting on Dana",
                ParsedEntry {
                    section: Section::PendingInput,
                    waiting_on: Some("Dana".to_owned()),
                    ..expected("Final copy for the order summary")
                },
            ),
            (
                "Confirm the tax rounding rule @Lee",
                ParsedEntry {
                    section: Section::PendingInput,
                    waiting_on: Some("Lee".to_owned()),
                    ..expected("Confirm the tax rounding rule")
                },
            ),
            (
                "Fix the cart redesign bug 1h30 /Personal",
                ParsedEntry {
                    story: by_name("WEB-142"),
                    workspace: Some(WorkspaceId::new("personal")),
                    estimate: Some(Estimate::from_minutes(90)),
                    ..expected("Fix the cart redesign bug")
                },
            ),
            (
                "Groceries next monday",
                ParsedEntry {
                    day: date(2026, 10, 26),
                    ..expected("Groceries")
                },
            ),
            (
                "Dentist 2026-10-30 at 9:30",
                ParsedEntry {
                    kind: EntryKind::Event,
                    day: date(2026, 10, 30),
                    time: event((9, 30), None),
                    ..expected("Dentist")
                },
            ),
            (
                "Stand-up at 9-10 today",
                ParsedEntry {
                    kind: EntryKind::Event,
                    time: event((9, 0), Some((10, 0))),
                    ..expected("Stand-up")
                },
            ),
            (
                "Stand-up from 9 to 10",
                ParsedEntry {
                    kind: EntryKind::Event,
                    time: event((9, 0), Some((10, 0))),
                    ..expected("Stand-up")
                },
            ),
            (
                "Demo 9-10:30",
                ParsedEntry {
                    kind: EntryKind::Event,
                    time: event((9, 0), Some((10, 30))),
                    ..expected("Demo")
                },
            ),
            (
                "Planning from 14:00 to 15:00",
                ParsedEntry {
                    kind: EntryKind::Event,
                    time: event((14, 0), Some((15, 0))),
                    ..expected("Planning")
                },
            ),
            (
                "Retro 13:00 – 13:45 WEB-157",
                ParsedEntry {
                    kind: EntryKind::Event,
                    story: ticket("WEB-157", true),
                    workspace: acme(),
                    time: event((13, 0), Some((13, 45))),
                    ..expected("Retro")
                },
            ),
            (
                "Waiting on Dana for the field mapping",
                ParsedEntry {
                    section: Section::PendingInput,
                    waiting_on: Some("Dana".to_owned()),
                    ..expected("Waiting on Dana for the field mapping")
                },
            ),
            (
                "Lunch with Lee 12pm",
                ParsedEntry {
                    kind: EntryKind::Event,
                    time: event((12, 0), None),
                    ..expected("Lunch with Lee")
                },
            ),
            (
                "Gym 6:30pm 1h",
                ParsedEntry {
                    kind: EntryKind::Event,
                    time: event((18, 30), Some((19, 30))),
                    estimate: Some(Estimate::from_minutes(60)),
                    ..expected("Gym")
                },
            ),
            (
                "Read the brief next week",
                ParsedEntry {
                    day: date(2026, 10, 26),
                    ..expected("Read the brief")
                },
            ),
            (
                "Onboarding emails: draft the welcome mail 1.5h thursday",
                ParsedEntry {
                    story: by_name("ACME-9"),
                    workspace: acme(),
                    day: date(2026, 10, 29),
                    estimate: Some(Estimate::from_minutes(90)),
                    ..expected("Onboarding emails: draft the welcome mail")
                },
            ),
            (
                "Ship it next friday /acme",
                ParsedEntry {
                    workspace: acme(),
                    day: date(2026, 10, 30),
                    ..expected("Ship it")
                },
            ),
        ];
        for (line, want) in cases {
            let got = parse_line(line).unwrap_or_else(|error| panic!("{line:?}: {error}"));
            assert_eq!(got, want, "line {line:?}");
        }
    }

    #[test]
    fn edge_cases() {
        let cases: Vec<(&str, ParsedEntry)> = vec![
            (
                "WEB-999 Chase the missing invoice",
                ParsedEntry {
                    story: ticket("WEB-999", false),
                    warnings: vec![ParseWarning::UnknownTicket {
                        number: StoryNumber::new("WEB-999"),
                    }],
                    ..expected("Chase the missing invoice")
                },
            ),
            (
                "Write tests 1h 2h",
                ParsedEntry {
                    estimate: Some(Estimate::from_minutes(60)),
                    warnings: vec![ParseWarning::ExtraEstimateDropped {
                        token: "2h".to_owned(),
                    }],
                    ..expected("Write tests")
                },
            ),
            (
                "Zażółć gęślą jaźń 30m",
                ParsedEntry {
                    estimate: Some(Estimate::from_minutes(30)),
                    ..expected("Zażółć gęślą jaźń")
                },
            ),
            (
                "Café ☕ with Ağa at 15:00",
                ParsedEntry {
                    kind: EntryKind::Event,
                    time: event((15, 0), None),
                    ..expected("Café ☕ with Ağa")
                },
            ),
            (
                "Fix the /etc config /Nowhere",
                ParsedEntry {
                    warnings: vec![
                        ParseWarning::UnknownWorkspace {
                            name: "etc".to_owned(),
                        },
                        ParseWarning::UnknownWorkspace {
                            name: "Nowhere".to_owned(),
                        },
                    ],
                    ..expected("Fix the /etc config /Nowhere")
                },
            ),
            (
                "Call /acme /personal",
                ParsedEntry {
                    workspace: acme(),
                    warnings: vec![ParseWarning::ExtraWorkspaceDropped {
                        name: "personal".to_owned(),
                    }],
                    ..expected("Call")
                },
            ),
            (
                "fix web-142 bug",
                ParsedEntry {
                    story: ticket("WEB-142", true),
                    workspace: acme(),
                    ..expected("fix bug")
                },
            ),
            (
                "Ask waiting on Bob @carol",
                ParsedEntry {
                    section: Section::PendingInput,
                    waiting_on: Some("carol".to_owned()),
                    ..expected("Ask")
                },
            ),
            ("Deploy 25:00", expected("Deploy 25:00")),
            ("Pay invoice 99", expected("Pay invoice 99")),
            ("Order 2 hats", expected("Order 2 hats")),
            (
                "Read pages 9-10 of the spec",
                expected("Read pages 9-10 of the spec"),
            ),
            ("Ship release 10-22", expected("Ship release 10-22")),
            (
                "Standup 9:00 then 9:30",
                ParsedEntry {
                    kind: EntryKind::Event,
                    time: event((9, 0), None),
                    warnings: vec![ParseWarning::ExtraTimeDropped {
                        token: "9:30".to_owned(),
                    }],
                    ..expected("Standup then")
                },
            ),
            (
                "today tomorrow Plan",
                ParsedEntry {
                    warnings: vec![ParseWarning::ExtraDayDropped {
                        token: "tomorrow".to_owned(),
                    }],
                    ..expected("Plan")
                },
            ),
            (
                "ACME-9 then WEB-142 both",
                ParsedEntry {
                    story: ticket("ACME-9", true),
                    workspace: acme(),
                    ..expected("then WEB-142 both")
                },
            ),
            (
                "Checkout redesign: fix the Checkout v2 banner",
                ParsedEntry {
                    story: by_name("WEB-142"),
                    workspace: acme(),
                    ..expected("Checkout redesign: fix the Checkout v2 banner")
                },
            ),
            ("Mycheckout redesignx", expected("Mycheckout redesignx")),
            (
                "Chase Lee, waiting on Lee.",
                ParsedEntry {
                    section: Section::PendingInput,
                    waiting_on: Some("Lee".to_owned()),
                    ..expected("Chase Lee,")
                },
            ),
            ("waiting on", expected("waiting on")),
            // A reversed range is not a time, so it stays in the text.
            (
                "Ping ＠Dana about the mapping 10:00-9:00",
                ParsedEntry {
                    section: Section::PendingInput,
                    waiting_on: Some("Dana".to_owned()),
                    ..expected("Ping about the mapping 10:00-9:00")
                },
            ),
            ("Sleep 1h75", expected("Sleep 1h75")),
            ("Swim 13pm", expected("Swim 13pm")),
            (
                "Nap 0h",
                ParsedEntry {
                    estimate: Some(Estimate::ZERO),
                    ..expected("Nap")
                },
            ),
        ];
        for (line, want) in cases {
            let got = parse_line(line).unwrap_or_else(|error| panic!("{line:?}: {error}"));
            assert_eq!(got, want, "line {line:?}");
        }
    }

    #[test]
    fn a_line_without_text_is_refused() {
        for line in [
            "",
            "   ",
            "15:00",
            "at 15:00 30m",
            "WEB-142",
            "/acme tomorrow",
            "@Dana",
        ] {
            assert_eq!(
                parse_line(line),
                Err(DomainError::EmptyEntry),
                "line {line:?}"
            );
        }
    }

    #[test]
    fn day_words_count_from_today() {
        let day = |line: &str| parse_line(line).unwrap().day;
        assert_eq!(day("x today"), TODAY);
        assert_eq!(day("x tomorrow"), date(2026, 10, 23));
        assert_eq!(day("x friday"), date(2026, 10, 23));
        assert_eq!(day("x Thursday"), date(2026, 10, 29));
        assert_eq!(day("x wednesday"), date(2026, 10, 28));
        assert_eq!(day("x next week"), date(2026, 10, 26));
        assert_eq!(day("x next monday"), date(2026, 10, 26));
        assert_eq!(day("x next sunday"), date(2026, 11, 1));
        assert_eq!(day("x on 2026-12-01"), date(2026, 12, 1));
        assert_eq!(parse_line("x 2026-13-01").unwrap().text, "x 2026-13-01");
        assert_eq!(parse_line("x next nothing").unwrap().text, "x next nothing");
    }

    #[test]
    fn estimates_cover_the_written_forms() {
        let minutes = |text: &str| parse_estimate(text).map(|estimate| estimate.as_secs() / 60);
        assert_eq!(minutes("30m"), Some(30));
        assert_eq!(minutes("45min"), Some(45));
        assert_eq!(minutes("2h"), Some(120));
        assert_eq!(minutes("1h30"), Some(90));
        assert_eq!(minutes("1h30m"), Some(90));
        assert_eq!(minutes("1.5h"), Some(90));
        assert_eq!(minutes("0.25h"), Some(15));
        assert_eq!(minutes("1.5h30"), None);
        assert_eq!(minutes("1h60"), None);
        assert_eq!(minutes("h"), None);
        assert_eq!(minutes("30"), None);
        assert_eq!(minutes("30 m"), None);
    }

    #[test]
    fn times_cover_the_written_forms() {
        assert_eq!(parse_time("15:00", false), Some(time(15, 0, 0, 0)));
        assert_eq!(parse_time("9:05", false), Some(time(9, 5, 0, 0)));
        assert_eq!(parse_time("9", false), None);
        assert_eq!(parse_time("9", true), Some(time(9, 0, 0, 0)));
        assert_eq!(parse_time("12am", false), Some(time(0, 0, 0, 0)));
        assert_eq!(parse_time("12pm", false), Some(time(12, 0, 0, 0)));
        assert_eq!(parse_time("12:30AM", false), Some(time(0, 30, 0, 0)));
        assert_eq!(parse_time("0pm", false), None);
        assert_eq!(parse_time("24:00", false), None);
        assert_eq!(parse_time("10:60", false), None);
        assert!(parse_time_range_token("10:00-9:00", true).is_none());
        assert!(parse_time_range_token("9am-11am", false).is_some());
        assert!(parse_time_range_token("9-10", false).is_none());
        assert!(parse_time_range_token("9-10", true).is_some());
    }

    #[test]
    fn a_bare_start_takes_the_end_s_am_or_pm_when_that_keeps_it_first() {
        let range = |line: &str| parse_line(line).unwrap().time;
        assert_eq!(range("Review 2-4pm"), event((14, 0), Some((16, 0))));
        assert_eq!(range("Review from 1 to 3pm"), event((13, 0), Some((15, 0))));
        assert_eq!(range("Review 9-11am"), event((9, 0), Some((11, 0))));
        assert_eq!(range("Review 12-1pm"), event((12, 0), Some((13, 0))));
        assert_eq!(range("Review 11-1pm"), event((11, 0), Some((13, 0))));
        assert_eq!(range("Review 10:30-2pm"), event((10, 30), Some((14, 0))));
        assert_eq!(range("Review 1:30-2:30pm"), event((13, 30), Some((14, 30))));
        assert_eq!(range("Review 15:00-4pm"), event((15, 0), Some((16, 0))));
    }

    #[test]
    fn a_time_wins_over_a_numeric_ticket_pattern() {
        let patterns = vec![TicketPattern::new(r"\d+").unwrap()];
        let parse_numeric = |line: &str| {
            parse(
                line,
                &QuickAddContext {
                    today: TODAY,
                    ticket_patterns: &patterns,
                    stories: &[],
                    workspaces: &[],
                    next_id: TaskId::new("task-15"),
                },
            )
            .unwrap()
        };
        let entry = parse_numeric("Stand-up at 9 for 4521");
        assert_eq!(entry.time, event((9, 0), None));
        assert_eq!(entry.story, ticket("4521", false));
        assert_eq!(entry.text, "Stand-up for");
        let entry = parse_numeric("Plan 4521 from 9 to 10");
        assert_eq!(entry.time, event((9, 0), Some((10, 0))));
        assert_eq!(entry.story, ticket("4521", false));
    }

    #[test]
    fn day_and_time_read_the_same_in_either_order() {
        let tomorrow_at_nine = parse_line("Call Dana tomorrow at 9").unwrap();
        assert_eq!(tomorrow_at_nine.day, date(2026, 10, 23));
        assert_eq!(tomorrow_at_nine.time, event((9, 0), None));
        assert_eq!(tomorrow_at_nine.text, "Call Dana");
        let at_nine_tomorrow = parse_line("Call Dana at 9 tomorrow").unwrap();
        assert_eq!(at_nine_tomorrow, tomorrow_at_nine);
        // A bare hour without `at` is not a time: it stays in the text of a task.
        let bare = parse_line("Call Dana 9 tomorrow").unwrap();
        assert_eq!(bare.kind, EntryKind::Task);
        assert_eq!(bare.text, "Call Dana 9");
        assert_eq!(bare.day, date(2026, 10, 23));
    }

    #[test]
    fn the_longest_story_name_wins_over_its_prefix() {
        let stories = vec![
            StoryRef {
                number: StoryNumber::new("WEB-1"),
                name: "Checkout".to_owned(),
                aliases: Vec::new(),
                workspace: None,
            },
            StoryRef {
                number: StoryNumber::new("WEB-2"),
                name: "Checkout redesign".to_owned(),
                aliases: Vec::new(),
                workspace: None,
            },
        ];
        let route = |line: &str| {
            parse(
                line,
                &QuickAddContext {
                    today: TODAY,
                    ticket_patterns: &[],
                    stories: &stories,
                    workspaces: &[],
                    next_id: TaskId::new("task-15"),
                },
            )
            .unwrap()
            .story
        };
        assert_eq!(route("Fix the checkout redesign banner"), by_name("WEB-2"));
        assert_eq!(route("Fix the checkout banner"), by_name("WEB-1"));
        assert_eq!(route("Fix the checkouts banner"), None);
    }

    #[test]
    fn a_workspace_token_takes_trailing_punctuation_but_not_glued_text() {
        let entry = parse_line("Ship it /acme, then rest").unwrap();
        assert_eq!(entry.workspace, acme());
        assert_eq!(entry.text, "Ship it then rest");
        let entry = parse_line("Ship it /acmecorp").unwrap();
        assert_eq!(entry.workspace, None);
        assert_eq!(entry.text, "Ship it /acmecorp");
        assert_eq!(
            entry.warnings,
            vec![ParseWarning::UnknownWorkspace {
                name: "acmecorp".to_owned(),
            }]
        );
    }

    #[test]
    fn a_parsed_task_becomes_a_vault_task_and_an_event_does_not() {
        let entry =
            parse_line("Wire the payment form WEB-142 2h tomorrow waiting on Dana").unwrap();
        let task = entry.vault_task(TODAY).unwrap();
        assert_eq!(task.id, TaskId::new("task-15"));
        assert_eq!(task.text, "Wire the payment form");
        assert_eq!(task.status, TaskStatus::Open);
        assert_eq!(task.section, Section::PendingInput);
        assert_eq!(task.story, Some(StoryNumber::new("WEB-142")));
        assert_eq!(task.created, Some(TODAY));
        assert_eq!(task.scheduled, Some(date(2026, 10, 23)));
        assert_eq!(task.estimate, Some(Estimate::from_minutes(120)));
        assert_eq!(task.assigned_to, Some("Dana".to_owned()));
        assert_eq!(task.due, None);
        assert!(
            parse_line("Sprint review 15:00-16:00")
                .unwrap()
                .vault_task(TODAY)
                .is_none()
        );
    }
}
