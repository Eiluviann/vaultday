//! Browse: the typed query line over every task of the workspace's stories.
//!
//! A query is whitespace-separated terms. `key:value` terms are typed filters,
//! any other term is free text that must appear in the task's text or id.
//! Double quotes keep spaces in a value (`story:"Checkout redesign"`,
//! `"payment form"`). Terms combine with AND; a comma inside a value is OR.
//!
//! | filter | values |
//! | --- | --- |
//! | `status` | `open` (default: open or in progress), `in-progress`, `done`, `all` |
//! | `story` | part of a number or of the title, such as `WEB-142`, `142`, `web`, `checkout` |
//! | `section` | `next` or `next-steps`, `pending` or `pending-input` |
//! | `due`, `scheduled` | `none`, `any`, `overdue`, a day (`today`, `tomorrow`, `yesterday`, `week`, `2026-10-22`); `<`, `<=`, `>`, `>=` in front compare with the day's range |
//! | `estimate` | `none`, `any`, a length such as `30m`, `1h30`; `<`, `<=`, `>`, `>=` in front |
//! | `waiting` (or `assigned`) | `any` (assigned to someone but me), `none`, or a name |
//! | `text` | text that must appear in the task's text or id |
//! | `created` | as `scheduled`, against the task's `[created:: ]` day |
//! | `source` | the task's `[source:: ]` field, whole and ignoring case: `braindump`, a signal id `sig-…` |
//! | `story-status` | the story note's status: `draft`, `ready`, `in-progress`, `on-hold`, `done`, `cancelled` or the word as written |
//! | `group` | `story,section` (default), `story`, `section` or `none` |
//! | `sort` | `manual` (default, the note's order), `scheduled`, `due`, `estimate`, `text`, `id` |
//!
//! `week` is Monday to Sunday around today. `<week` is before Monday,
//! `<=week` up to and including Sunday (overdue and this week), `week` the
//! seven days themselves.
//!
//! The rows are the top-level tasks under a story's Next Steps and Pending
//! Input, grouped by story, then by section, unless `group` says otherwise.
//! Without story grouping every row sits in one group; without section
//! grouping each group has one section holding both, in the note's order. Filtering and grouping are pure:
//! the bindings build the rows from the vault.

use std::cmp::Ordering;

use jiff::ToSpan;
use jiff::civil::{Date, Weekday};
use thiserror::Error;

use crate::estimate::Estimate;
use crate::quick_add::parse_estimate;
use crate::task::{Section, TaskStatus, VaultTask};

/// The saved queries every workspace starts with: a name and its query line.
/// Braindump is what was captured as a brain dump: the open tasks carrying
/// `[source:: braindump]`, however long ago.
pub const PRESETS: [(&str, &str); 6] = [
    ("All open", "status:open"),
    ("Backlog", "status:open section:next scheduled:none"),
    ("Chasers", "status:open waiting:any"),
    ("Active stories", "status:open story-status:in-progress"),
    ("Due this week", "status:open due:<=week"),
    ("Braindump", "status:open source:braindump"),
];

/// One task of a story with the story's title, as Browse lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowseTask {
    /// The task line's fields.
    pub task: VaultTask,
    /// The title of the story note that holds the line.
    pub story_title: Option<String>,
    /// The story note's status token, lower case.
    pub story_status: Option<String>,
    /// The line's `[source:: ]` field: where the task came from.
    pub source: Option<String>,
}

/// Why a query line was refused. The message is meant to be shown as typed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum QueryError {
    /// The key before the colon is not a filter.
    #[error(
        "{0} is not a filter; use status, story, story-status, section, due, scheduled, created, source, estimate, waiting, text, group or sort"
    )]
    UnknownFilter(String),
    /// The value after the colon does not suit the filter.
    #[error("{filter}:{value} is not understood; {expected}")]
    BadValue {
        /// The filter key.
        filter: String,
        /// The value as typed.
        value: String,
        /// What the filter accepts.
        expected: &'static str,
    },
    /// A double quote is never closed.
    #[error("a quote is never closed")]
    UnclosedQuote,
}

/// A parsed query line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowseQuery {
    status: StatusFilter,
    stories: Vec<String>,
    story_statuses: Vec<String>,
    sections: Vec<Section>,
    due: Option<DateFilter>,
    scheduled: Option<DateFilter>,
    created: Option<DateFilter>,
    sources: Vec<String>,
    estimate: Option<EstimateFilter>,
    waiting: Option<WaitingFilter>,
    text: Vec<String>,
    group: Grouping,
    sort: SortKey,
}

/// How [`BrowseQuery::run`] groups the rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Grouping {
    /// One group per story; otherwise one group holds every story.
    pub by_story: bool,
    /// One section per Next Steps and Pending Input; otherwise one section holds both.
    pub by_section: bool,
}

/// The tasks of one story that match, by section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowseGroup {
    /// `true` for the one group of a query that does not group by story;
    /// the story fields are then `None`.
    pub spans_stories: bool,
    /// The story number; `None` for tasks of a story without one.
    pub story_number: Option<String>,
    /// The story note's title.
    pub story_title: Option<String>,
    /// Next Steps, then Pending Input; a section without a match is left out.
    /// One section without a name when the query does not group by section.
    pub sections: Vec<BrowseSection>,
}

/// The matching tasks of one section of one story.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowseSection {
    /// Which section; `None` when the query does not group by section.
    pub section: Option<Section>,
    /// The tasks in query order.
    pub tasks: Vec<BrowseTask>,
}

impl BrowseGroup {
    /// How many tasks the group holds.
    pub fn task_count(&self) -> usize {
        self.sections
            .iter()
            .map(|section| section.tasks.len())
            .sum()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StatusFilter {
    Open,
    InProgress,
    Done,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Comparison {
    Equal,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Day {
    Today,
    Tomorrow,
    Yesterday,
    Week,
    On(Date),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DateFilter {
    Unset,
    Set,
    Compare(Comparison, Day),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EstimateFilter {
    Unset,
    Set,
    Compare(Comparison, u64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum WaitingFilter {
    Anyone,
    Nobody,
    Named(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SortKey {
    Manual,
    Scheduled,
    Due,
    Estimate,
    Text,
    Id,
}

impl BrowseQuery {
    /// Reads a query line. An empty line is the default: every open task.
    ///
    /// # Errors
    /// [`QueryError`] for an unknown filter, a value the filter does not
    /// accept or an unclosed quote.
    pub fn parse(line: &str) -> Result<BrowseQuery, QueryError> {
        let mut query = BrowseQuery {
            status: StatusFilter::Open,
            stories: Vec::new(),
            story_statuses: Vec::new(),
            sections: Vec::new(),
            due: None,
            scheduled: None,
            created: None,
            sources: Vec::new(),
            estimate: None,
            waiting: None,
            text: Vec::new(),
            group: Grouping {
                by_story: true,
                by_section: true,
            },
            sort: SortKey::Manual,
        };
        for term in split_terms(line)? {
            match term.split_once(':') {
                Some((key, value)) if is_filter_key(key) => {
                    query.apply(&key.to_ascii_lowercase(), unquote(value))?;
                }
                Some((key, _)) if looks_like_key(key) => {
                    return Err(QueryError::UnknownFilter(key.to_owned()));
                }
                _ => query.text.push(unquote(&term).to_lowercase()),
            }
        }
        Ok(query)
    }

    /// How the query groups its rows.
    pub fn grouping(&self) -> Grouping {
        self.group
    }

    /// Whether `row` passes every filter as of `today`.
    pub fn matches(&self, row: &BrowseTask, today: Date) -> bool {
        let task = &row.task;
        self.status.accepts(task.status)
            && self.matches_story(row)
            && self.matches_story_status(row)
            && (self.sections.is_empty() || self.sections.contains(&task.section))
            && self
                .due
                .is_none_or(|filter| filter.accepts(task.due, today))
            && self
                .scheduled
                .is_none_or(|filter| filter.accepts(task.scheduled, today))
            && self
                .created
                .is_none_or(|filter| filter.accepts(task.created, today))
            && self.matches_source(row)
            && self
                .estimate
                .is_none_or(|filter| filter.accepts(task.estimate))
            && self
                .waiting
                .as_ref()
                .is_none_or(|filter| filter.accepts(task))
            && self.text.iter().all(|term| {
                task.text.to_lowercase().contains(term)
                    || task.id.as_str().to_lowercase().contains(term)
            })
    }

    /// The matching rows grouped as the query's `group` says (by story,
    /// then by section, by default). Stories keep the order of `rows`; the
    /// rows of a section keep it too unless the query names a `sort`.
    pub fn run(&self, rows: &[BrowseTask], today: Date) -> Vec<BrowseGroup> {
        let mut groups: Vec<BrowseGroup> = Vec::new();
        for row in rows.iter().filter(|row| self.matches(row, today)) {
            let (number, title) = if self.group.by_story {
                (
                    row.task
                        .story
                        .as_ref()
                        .map(|story| story.as_str().to_owned()),
                    row.story_title.clone(),
                )
            } else {
                (None, None)
            };
            let section = self.group.by_section.then_some(row.task.section);
            let index = match groups
                .iter()
                .position(|group| group.story_number == number && group.story_title == title)
            {
                Some(index) => index,
                None => {
                    groups.push(BrowseGroup {
                        spans_stories: !self.group.by_story,
                        story_number: number,
                        story_title: title,
                        sections: Vec::new(),
                    });
                    groups.len() - 1
                }
            };
            let Some(group) = groups.get_mut(index) else {
                continue;
            };
            match group
                .sections
                .iter_mut()
                .find(|candidate| candidate.section == section)
            {
                Some(found) => found.tasks.push(row.clone()),
                None => group.sections.push(BrowseSection {
                    section,
                    tasks: vec![row.clone()],
                }),
            }
        }
        for group in &mut groups {
            group
                .sections
                .sort_by_key(|section| section.section.map_or(0, section_rank));
            for section in &mut group.sections {
                self.sort_tasks(&mut section.tasks);
            }
        }
        groups
    }

    fn apply(&mut self, key: &str, value: &str) -> Result<(), QueryError> {
        let bad = |expected: &'static str| QueryError::BadValue {
            filter: key.to_owned(),
            value: value.to_owned(),
            expected,
        };
        match key {
            "status" => {
                self.status = match value.to_ascii_lowercase().as_str() {
                    "open" => StatusFilter::Open,
                    "in-progress" | "doing" => StatusFilter::InProgress,
                    "done" => StatusFilter::Done,
                    "all" => StatusFilter::All,
                    _ => return Err(bad("use open, in-progress, done or all")),
                };
            }
            "story" => {
                self.stories = value
                    .split(',')
                    .map(|part| part.trim().to_lowercase())
                    .filter(|part| !part.is_empty())
                    .collect();
                if self.stories.is_empty() {
                    return Err(bad("name a story number or a word of its title"));
                }
            }
            "story-status" => {
                self.story_statuses = value
                    .split(',')
                    .map(|part| part.trim().to_lowercase().replace(' ', "-"))
                    .filter(|part| !part.is_empty())
                    .collect();
                if self.story_statuses.is_empty() {
                    return Err(bad("name a story status such as in-progress or ready"));
                }
            }
            "created" => {
                self.created = Some(parse_date_filter(value, false).ok_or_else(|| {
                    bad("use none, any, today, yesterday, week, a day or <, <=, >, >= with one")
                })?);
            }
            "source" => {
                self.sources = value
                    .split(',')
                    .map(|part| part.trim().to_lowercase())
                    .filter(|part| !part.is_empty())
                    .collect();
                if self.sources.is_empty() {
                    return Err(bad("name a source such as braindump"));
                }
            }
            "group" => {
                self.group = parse_grouping(value)
                    .ok_or_else(|| bad("use story,section, story, section or none"))?;
            }
            "section" => {
                let sections: Option<Vec<Section>> = value
                    .split(',')
                    .map(|part| parse_section(part.trim()))
                    .collect();
                self.sections = sections.ok_or_else(|| bad("use next or pending"))?;
            }
            "due" => {
                self.due = Some(parse_date_filter(value, true).ok_or_else(|| {
                    bad("use none, any, overdue, today, week, a day or <, <=, >, >= with one")
                })?);
            }
            "scheduled" => {
                self.scheduled = Some(parse_date_filter(value, false).ok_or_else(|| {
                    bad("use none, any, today, week, a day or <, <=, >, >= with one")
                })?);
            }
            "estimate" => {
                self.estimate = Some(parse_estimate_filter(value).ok_or_else(|| {
                    bad("use none, any, a length such as 30m or 1h30, or <, <=, >, >= with one")
                })?);
            }
            "waiting" | "assigned" => {
                self.waiting = Some(match value.to_ascii_lowercase().as_str() {
                    "" => return Err(bad("use any, none or a name")),
                    "any" => WaitingFilter::Anyone,
                    "none" => WaitingFilter::Nobody,
                    _ => WaitingFilter::Named(value.trim_start_matches('@').to_lowercase()),
                });
            }
            "text" => {
                if value.is_empty() {
                    return Err(bad("type the text to look for"));
                }
                self.text.push(value.to_lowercase());
            }
            "sort" => {
                self.sort = match value.to_ascii_lowercase().as_str() {
                    "manual" => SortKey::Manual,
                    "scheduled" => SortKey::Scheduled,
                    "due" => SortKey::Due,
                    "estimate" => SortKey::Estimate,
                    "text" => SortKey::Text,
                    "id" => SortKey::Id,
                    _ => return Err(bad("use manual, scheduled, due, estimate, text or id")),
                };
            }
            _ => return Err(QueryError::UnknownFilter(key.to_owned())),
        }
        Ok(())
    }

    fn matches_story(&self, row: &BrowseTask) -> bool {
        if self.stories.is_empty() {
            return true;
        }
        let number = row
            .task
            .story
            .as_ref()
            .map(|story| story.as_str().to_lowercase())
            .unwrap_or_default();
        let title = row
            .story_title
            .as_deref()
            .unwrap_or_default()
            .to_lowercase();
        self.stories.iter().any(|wanted| {
            (!number.is_empty() && number.contains(wanted.as_str()))
                || title.contains(wanted.as_str())
        })
    }

    fn matches_source(&self, row: &BrowseTask) -> bool {
        self.sources.is_empty()
            || row.source.as_deref().is_some_and(|source| {
                let source = source.trim().to_lowercase();
                self.sources.contains(&source)
            })
    }

    fn matches_story_status(&self, row: &BrowseTask) -> bool {
        self.story_statuses.is_empty()
            || row
                .story_status
                .as_deref()
                .is_some_and(|status| self.story_statuses.iter().any(|wanted| wanted == status))
    }

    fn sort_tasks(&self, tasks: &mut [BrowseTask]) {
        // Rows without a value sort last.
        fn by_option<T: Ord>(left: Option<T>, right: Option<T>) -> Ordering {
            match (left, right) {
                (Some(left), Some(right)) => left.cmp(&right),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => Ordering::Equal,
            }
        }
        match self.sort {
            SortKey::Manual => {}
            SortKey::Scheduled => {
                tasks.sort_by(|a, b| by_option(a.task.scheduled, b.task.scheduled))
            }
            SortKey::Due => tasks.sort_by(|a, b| by_option(a.task.due, b.task.due)),
            SortKey::Estimate => tasks.sort_by(|a, b| by_option(a.task.estimate, b.task.estimate)),
            SortKey::Text => tasks.sort_by_key(|row| row.task.text.to_lowercase()),
            SortKey::Id => tasks.sort_by(|a, b| a.task.id.cmp(&b.task.id)),
        }
    }
}

impl StatusFilter {
    fn accepts(self, status: TaskStatus) -> bool {
        match self {
            StatusFilter::Open => status.is_unfinished(),
            StatusFilter::InProgress => status == TaskStatus::InProgress,
            StatusFilter::Done => status == TaskStatus::Done,
            StatusFilter::All => true,
        }
    }
}

impl Comparison {
    /// Splits a leading `<`, `<=`, `>`, `>=` off `value`.
    fn split(value: &str) -> (Comparison, &str) {
        if let Some(rest) = value.strip_prefix("<=") {
            (Comparison::LessOrEqual, rest)
        } else if let Some(rest) = value.strip_prefix(">=") {
            (Comparison::GreaterOrEqual, rest)
        } else if let Some(rest) = value.strip_prefix('<') {
            (Comparison::Less, rest)
        } else if let Some(rest) = value.strip_prefix('>') {
            (Comparison::Greater, rest)
        } else {
            (Comparison::Equal, value)
        }
    }

    /// Whether `value` stands in this relation to the range `low..=high`.
    fn holds<T: Ord>(self, value: T, low: T, high: T) -> bool {
        match self {
            Comparison::Equal => value >= low && value <= high,
            Comparison::Less => value < low,
            Comparison::LessOrEqual => value <= high,
            Comparison::Greater => value > high,
            Comparison::GreaterOrEqual => value >= low,
        }
    }
}

impl Day {
    /// The first and last day the word covers as of `today`.
    fn range(self, today: Date) -> (Date, Date) {
        match self {
            Day::Today => (today, today),
            Day::Tomorrow => {
                let day = shifted(today, 1);
                (day, day)
            }
            Day::Yesterday => {
                let day = shifted(today, -1);
                (day, day)
            }
            Day::Week => {
                let since_monday = i64::from(today.weekday().since(Weekday::Monday));
                let monday = shifted(today, -since_monday);
                (monday, shifted(monday, 6))
            }
            Day::On(day) => (day, day),
        }
    }
}

impl DateFilter {
    fn accepts(self, value: Option<Date>, today: Date) -> bool {
        match self {
            DateFilter::Unset => value.is_none(),
            DateFilter::Set => value.is_some(),
            DateFilter::Compare(comparison, day) => {
                let (low, high) = day.range(today);
                value.is_some_and(|value| comparison.holds(value, low, high))
            }
        }
    }
}

impl EstimateFilter {
    fn accepts(self, value: Option<Estimate>) -> bool {
        match self {
            EstimateFilter::Unset => value.is_none(),
            EstimateFilter::Set => value.is_some(),
            EstimateFilter::Compare(comparison, minutes) => value.is_some_and(|estimate| {
                comparison.holds(estimate.as_secs() / 60, minutes, minutes)
            }),
        }
    }
}

impl WaitingFilter {
    fn accepts(&self, task: &VaultTask) -> bool {
        let assignee = task
            .assigned_to
            .as_deref()
            .map(|who| who.trim().trim_start_matches('@').to_lowercase())
            .filter(|who| !who.is_empty());
        let waits_on_someone = assignee.as_deref().filter(|who| *who != "me");
        match self {
            WaitingFilter::Anyone => waits_on_someone.is_some(),
            WaitingFilter::Nobody => waits_on_someone.is_none(),
            WaitingFilter::Named(name) => {
                waits_on_someone.is_some_and(|who| who.contains(name.as_str()))
            }
        }
    }
}

const FILTER_KEYS: [&str; 13] = [
    "status",
    "story",
    "story-status",
    "section",
    "due",
    "scheduled",
    "created",
    "source",
    "estimate",
    "waiting",
    "assigned",
    "text",
    "group",
];

fn is_filter_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key == "sort" || FILTER_KEYS.contains(&key.as_str())
}

/// A word that is typed like a filter key (`stroy:x`), so a typo is reported
/// instead of searched as text. A URL's scheme is not one.
fn looks_like_key(key: &str) -> bool {
    !key.is_empty()
        && key.chars().all(|c| c.is_ascii_alphabetic() || c == '-')
        && !matches!(
            key.to_ascii_lowercase().as_str(),
            "http" | "https" | "mailto"
        )
}

fn parse_section(text: &str) -> Option<Section> {
    match text.to_ascii_lowercase().replace(' ', "-").as_str() {
        "next" | "next-steps" => Some(Section::NextSteps),
        "pending" | "pending-input" => Some(Section::PendingInput),
        _ => None,
    }
}

/// `story,section` in either order, `story`, `section` or `none`.
fn parse_grouping(value: &str) -> Option<Grouping> {
    let mut grouping = Grouping {
        by_story: false,
        by_section: false,
    };
    let value = value.to_ascii_lowercase();
    if value == "none" {
        return Some(grouping);
    }
    for part in value.split(',').map(str::trim) {
        match part {
            "story" if !grouping.by_story => grouping.by_story = true,
            "section" if !grouping.by_section => grouping.by_section = true,
            _ => return None,
        }
    }
    Some(grouping)
}

fn section_rank(section: Section) -> u8 {
    match section {
        Section::NextSteps => 0,
        Section::PendingInput => 1,
    }
}

fn parse_date_filter(value: &str, allows_overdue: bool) -> Option<DateFilter> {
    match value.to_ascii_lowercase().as_str() {
        "none" => return Some(DateFilter::Unset),
        "any" => return Some(DateFilter::Set),
        "overdue" if allows_overdue => {
            return Some(DateFilter::Compare(Comparison::Less, Day::Today));
        }
        _ => {}
    }
    let (comparison, word) = Comparison::split(value);
    let day = match word.to_ascii_lowercase().as_str() {
        "today" => Day::Today,
        "tomorrow" => Day::Tomorrow,
        "yesterday" => Day::Yesterday,
        "week" => Day::Week,
        other => Day::On(other.parse().ok()?),
    };
    Some(DateFilter::Compare(comparison, day))
}

fn parse_estimate_filter(value: &str) -> Option<EstimateFilter> {
    match value.to_ascii_lowercase().as_str() {
        "none" => return Some(EstimateFilter::Unset),
        "any" => return Some(EstimateFilter::Set),
        _ => {}
    }
    let (comparison, length) = Comparison::split(value);
    let minutes = parse_estimate(length)?.as_secs() / 60;
    Some(EstimateFilter::Compare(comparison, minutes))
}

fn shifted(day: Date, days: i64) -> Date {
    day.checked_add(days.days()).unwrap_or(day)
}

/// Splits on whitespace outside double quotes; the quotes stay on the term.
fn split_terms(line: &str) -> Result<Vec<String>, QueryError> {
    let mut terms = Vec::new();
    let mut current = String::new();
    let mut is_quoted = false;
    for character in line.chars() {
        match character {
            '"' => {
                is_quoted = !is_quoted;
                current.push(character);
            }
            c if c.is_whitespace() && !is_quoted => {
                if !current.is_empty() {
                    terms.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if is_quoted {
        return Err(QueryError::UnclosedQuote);
    }
    if !current.is_empty() {
        terms.push(current);
    }
    Ok(terms)
}

fn unquote(text: &str) -> &str {
    text.strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::{StoryNumber, TaskId};

    /// Story number and the sections' sizes, as a group reads.
    type CountShape = Vec<(String, Vec<(Option<Section>, usize)>)>;
    /// Whether a group spans stories, its number and its sections' ids.
    type GroupShape = Vec<(bool, Option<String>, Vec<(Option<Section>, Vec<String>)>)>;

    fn day(text: &str) -> Date {
        text.parse().unwrap()
    }

    // Thursday 22 October 2026.
    fn today() -> Date {
        day("2026-10-22")
    }

    fn row(id: &str, story: &str, section: Section, text: &str) -> BrowseTask {
        let mut task = VaultTask::open(TaskId::new(id), text);
        task.section = section;
        task.story = Some(StoryNumber::new(story));
        let story_status = if story == "WEB-142" {
            "in-progress"
        } else {
            "ready"
        };
        BrowseTask {
            task,
            story_title: Some(format!("{story} title")),
            story_status: Some(story_status.to_owned()),
            source: None,
        }
    }

    fn rows() -> Vec<BrowseTask> {
        let mut gateway = row(
            "task-2",
            "WEB-142",
            Section::NextSteps,
            "Wire the payment form",
        );
        gateway.task.scheduled = Some(today());
        gateway.task.estimate = Some(Estimate::from_minutes(120));
        gateway.task.created = Some(day("2026-10-21"));
        let mut declined = row(
            "task-4",
            "WEB-142",
            Section::NextSteps,
            "Handle the declined card",
        );
        declined.task.status = TaskStatus::InProgress;
        declined.task.estimate = Some(Estimate::from_minutes(45));
        let mut test = row(
            "task-5",
            "WEB-142",
            Section::NextSteps,
            "Write the checkout test",
        );
        test.task.due = Some(day("2026-10-28"));
        let mut copy = row("task-6", "WEB-142", Section::PendingInput, "Final copy");
        copy.task.created = Some(day("2026-10-10"));
        copy.task.assigned_to = Some("Dana".to_owned());
        let mut mail = row(
            "task-8",
            "WEB-157",
            Section::NextSteps,
            "Draft the confirmation mail",
        );
        mail.task.due = Some(day("2026-10-20"));
        mail.task.created = Some(today());
        mail.source = Some("braindump".to_owned());
        copy.source = Some("Braindump".to_owned());
        let mut provider = row(
            "task-9",
            "WEB-157",
            Section::PendingInput,
            "Provider decision",
        );
        provider.task.assigned_to = Some("Lee".to_owned());
        let mut mine = row("task-10", "WEB-157", Section::NextSteps, "Mine already");
        mine.task.assigned_to = Some("me".to_owned());
        let mut done = row(
            "task-1",
            "WEB-142",
            Section::NextSteps,
            "Extract the address form",
        );
        done.task.status = TaskStatus::Done;
        vec![done, gateway, declined, test, copy, mail, provider, mine]
    }

    fn ids(query: &str) -> Vec<String> {
        let query = BrowseQuery::parse(query).unwrap();
        query
            .run(&rows(), today())
            .into_iter()
            .flat_map(|group| group.sections)
            .flat_map(|section| section.tasks)
            .map(|row| row.task.id.as_str().to_owned())
            .collect()
    }

    #[test]
    fn the_empty_query_lists_every_open_task_grouped_by_story_then_section() {
        let query = BrowseQuery::parse("").unwrap();
        let groups = query.run(&rows(), today());
        let shape: CountShape = groups
            .iter()
            .map(|group| {
                (
                    group.story_number.clone().unwrap(),
                    group
                        .sections
                        .iter()
                        .map(|section| (section.section, section.tasks.len()))
                        .collect(),
                )
            })
            .collect();
        assert_eq!(
            shape,
            vec![
                (
                    "WEB-142".to_owned(),
                    vec![
                        (Some(Section::NextSteps), 3),
                        (Some(Section::PendingInput), 1)
                    ]
                ),
                (
                    "WEB-157".to_owned(),
                    vec![
                        (Some(Section::NextSteps), 2),
                        (Some(Section::PendingInput), 1)
                    ]
                ),
            ]
        );
        assert_eq!(groups[0].task_count(), 4);
        assert!(groups.iter().all(|group| !group.spans_stories));
    }

    fn shape(query: &str) -> GroupShape {
        BrowseQuery::parse(query)
            .unwrap()
            .run(&rows(), today())
            .into_iter()
            .map(|group| {
                (
                    group.spans_stories,
                    group.story_number,
                    group
                        .sections
                        .into_iter()
                        .map(|section| {
                            (
                                section.section,
                                section
                                    .tasks
                                    .into_iter()
                                    .map(|row| row.task.id.as_str().to_owned())
                                    .collect(),
                            )
                        })
                        .collect(),
                )
            })
            .collect()
    }

    fn owned(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|id| (*id).to_owned()).collect()
    }

    #[test]
    fn group_by_story_alone_keeps_both_sections_in_one_in_note_order() {
        assert_eq!(
            shape("group:story story:142"),
            vec![(
                false,
                Some("WEB-142".to_owned()),
                vec![(None, owned(&["task-2", "task-4", "task-5", "task-6"]))]
            )]
        );
    }

    #[test]
    fn group_by_section_alone_spans_every_story() {
        assert_eq!(
            shape("group:section"),
            vec![(
                true,
                None,
                vec![
                    (
                        Some(Section::NextSteps),
                        owned(&["task-2", "task-4", "task-5", "task-8", "task-10"])
                    ),
                    (Some(Section::PendingInput), owned(&["task-6", "task-9"])),
                ]
            )]
        );
    }

    #[test]
    fn group_none_is_one_flat_list_and_the_default_matches_story_section() {
        assert_eq!(
            shape("group:none"),
            vec![(
                true,
                None,
                vec![(
                    None,
                    owned(&[
                        "task-2", "task-4", "task-5", "task-6", "task-8", "task-9", "task-10"
                    ])
                )]
            )]
        );
        assert_eq!(shape("group:story,section"), shape(""));
        assert_eq!(shape("group:SECTION,story"), shape(""));
        assert_eq!(
            shape("group:none sort:id")[0].2[0].1,
            owned(&[
                "task-10", "task-2", "task-4", "task-5", "task-6", "task-8", "task-9"
            ])
        );
    }

    #[test]
    fn story_status_and_created_filter_the_rows() {
        assert_eq!(
            ids("story-status:in-progress"),
            vec!["task-2", "task-4", "task-5", "task-6"]
        );
        assert_eq!(
            ids("story-status:ready,on-hold"),
            vec!["task-8", "task-10", "task-9"]
        );
        assert_eq!(ids("story-status:done"), Vec::<String>::new());
        assert_eq!(ids("created:today"), vec!["task-8"]);
        assert_eq!(ids("created:>=yesterday"), vec!["task-2", "task-8"]);
        assert_eq!(ids("created:<week"), vec!["task-6"]);
        assert_eq!(ids("created:none").len(), 4);
    }

    #[test]
    fn a_bad_group_story_status_or_created_is_refused() {
        for line in [
            "group:",
            "group:task",
            "group:story,story",
            "group:none,story",
            "story-status:",
            "story-status:,",
            "created:overdue",
            "created:someday",
        ] {
            assert!(
                matches!(BrowseQuery::parse(line), Err(QueryError::BadValue { .. })),
                "{line}"
            );
        }
    }

    #[test]
    fn status_filters_pick_done_in_progress_or_everything() {
        assert_eq!(ids("status:done"), vec!["task-1"]);
        assert_eq!(ids("status:in-progress"), vec!["task-4"]);
        assert_eq!(ids("status:all").len(), 8);
    }

    #[test]
    fn story_matches_part_of_a_number_or_of_the_title() {
        assert_eq!(ids("story:WEB-157"), vec!["task-8", "task-10", "task-9"]);
        assert_eq!(ids("story:web-1"), ids("story:web"));
        assert_eq!(
            ids("story:\"157 title\""),
            vec!["task-8", "task-10", "task-9"]
        );
        assert_eq!(ids("story:web-142,web-157").len(), 7);
    }

    #[test]
    fn section_picks_next_steps_or_pending_input() {
        assert_eq!(ids("section:pending"), vec!["task-6", "task-9"]);
        assert_eq!(
            ids("section:next-steps story:157"),
            vec!["task-8", "task-10"]
        );
    }

    #[test]
    fn due_compares_against_a_day_or_the_week() {
        assert_eq!(ids("due:overdue"), vec!["task-8"]);
        assert_eq!(ids("due:<today"), vec!["task-8"]);
        assert_eq!(ids("due:2026-10-28"), vec!["task-5"]);
        // The week of Thursday 22 October is Monday 19 to Sunday 25.
        assert_eq!(ids("due:week"), vec!["task-8"]);
        assert_eq!(ids("due:<=week"), vec!["task-8"]);
        assert_eq!(ids("due:>week"), vec!["task-5"]);
        assert_eq!(ids("due:none").len(), 5);
        assert_eq!(ids("due:any"), vec!["task-5", "task-8"]);
    }

    #[test]
    fn scheduled_understands_today_and_none() {
        assert_eq!(ids("scheduled:today"), vec!["task-2"]);
        assert_eq!(ids("scheduled:>=tomorrow"), Vec::<String>::new());
        assert_eq!(ids("scheduled:none").len(), 6);
    }

    #[test]
    fn estimate_compares_in_minutes() {
        assert_eq!(ids("estimate:45m"), vec!["task-4"]);
        assert_eq!(ids("estimate:<=1h"), vec!["task-4"]);
        assert_eq!(ids("estimate:>1h"), vec!["task-2"]);
        assert_eq!(ids("estimate:>=0h45"), vec!["task-2", "task-4"]);
        assert_eq!(ids("estimate:any"), vec!["task-2", "task-4"]);
        assert_eq!(ids("estimate:none").len(), 5);
    }

    #[test]
    fn waiting_means_assigned_to_someone_but_me() {
        assert_eq!(ids("waiting:any"), vec!["task-6", "task-9"]);
        assert_eq!(ids("assigned:dana"), vec!["task-6"]);
        assert_eq!(ids("waiting:@lee"), vec!["task-9"]);
        assert_eq!(ids("waiting:none").len(), 5);
    }

    #[test]
    fn free_text_and_the_text_filter_search_text_and_id_together() {
        assert_eq!(ids("payment"), vec!["task-2"]);
        assert_eq!(ids("\"declined card\""), vec!["task-4"]);
        assert_eq!(ids("text:task-9"), vec!["task-9"]);
        assert_eq!(ids("form story:142 text:wire"), vec!["task-2"]);
    }

    #[test]
    fn filters_combine_with_and_and_sort_reorders_within_a_section() {
        assert_eq!(
            ids("story:142 section:next sort:estimate"),
            vec!["task-4", "task-2", "task-5"]
        );
        assert_eq!(
            ids("story:142 section:next sort:text"),
            vec!["task-4", "task-2", "task-5"]
        );
        assert_eq!(
            ids("story:142 section:next sort:id"),
            vec!["task-2", "task-4", "task-5"]
        );
        assert_eq!(
            ids("story:157 section:next sort:due"),
            vec!["task-8", "task-10"]
        );
    }

    #[test]
    fn the_presets_all_parse_and_backlog_means_unscheduled_next_steps() {
        for (name, line) in PRESETS {
            assert!(BrowseQuery::parse(line).is_ok(), "{name}: {line}");
        }
        assert_eq!(
            ids("status:open section:next scheduled:none"),
            vec!["task-4", "task-5", "task-8", "task-10"]
        );
        let preset = |name: &str| {
            PRESETS
                .iter()
                .find(|(preset, _)| *preset == name)
                .map(|(_, line)| ids(line))
                .unwrap()
        };
        assert_eq!(
            PRESETS.map(|(name, _)| name),
            [
                "All open",
                "Backlog",
                "Chasers",
                "Active stories",
                "Due this week",
                "Braindump"
            ]
        );
        assert_eq!(preset("Due this week"), vec!["task-8"]);
        assert_eq!(preset("Chasers"), vec!["task-6", "task-9"]);
        assert_eq!(
            preset("Active stories"),
            vec!["task-2", "task-4", "task-5", "task-6"]
        );
        assert_eq!(preset("Braindump"), vec!["task-6", "task-8"]);
        assert_eq!(ids("source:sig-1"), Vec::<String>::new());
        assert_eq!(ids("source:braindump,sig-1 story:157"), vec!["task-8"]);
        assert!(matches!(
            BrowseQuery::parse("source:"),
            Err(QueryError::BadValue { .. })
        ));
    }

    #[test]
    fn a_bad_query_is_refused_with_a_message_that_names_the_term() {
        assert_eq!(
            BrowseQuery::parse("stroy:x").unwrap_err(),
            QueryError::UnknownFilter("stroy".to_owned())
        );
        let error = BrowseQuery::parse("due:soonish").unwrap_err();
        assert!(
            error
                .to_string()
                .starts_with("due:soonish is not understood"),
            "{error}"
        );
        assert!(BrowseQuery::parse("estimate:lots").is_err());
        assert!(BrowseQuery::parse("section:today").is_err());
        assert!(BrowseQuery::parse("status:maybe").is_err());
        assert!(BrowseQuery::parse("sort:random").is_err());
        assert_eq!(
            BrowseQuery::parse("text:\"open").unwrap_err(),
            QueryError::UnclosedQuote
        );
    }

    #[test]
    fn a_link_in_the_text_is_searched_not_read_as_a_filter() {
        assert!(BrowseQuery::parse("https://example.test").is_ok());
    }

    #[test]
    fn the_week_starts_on_monday_even_when_today_is_sunday() {
        let (low, high) = Day::Week.range(day("2026-10-25"));
        assert_eq!((low, high), (day("2026-10-19"), day("2026-10-25")));
        let (low, high) = Day::Week.range(day("2026-10-19"));
        assert_eq!((low, high), (day("2026-10-19"), day("2026-10-25")));
    }
}
