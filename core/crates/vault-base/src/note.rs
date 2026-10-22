//! Typed views over a parsed note: stories, project hubs and day logs. The
//! views read the frontmatter and the task lines; they never own the text.

use std::path::Path;

use crate::date::{Date, Time};
use crate::scan::{Document, InlineField, Task};

/// Heading of the section that holds a story's own next actions.
pub const NEXT_STEPS_HEADING: &str = "Next Steps";
/// Heading of the section that holds what a story waits on from others.
pub const PENDING_INPUT_HEADING: &str = "Pending Input";
/// Heading of the list in a day log.
pub const LOG_HEADING: &str = "Log";

/// What a note is, read from frontmatter `type` (trimmed, case-insensitive)
/// with two fallbacks: a `number` key marks a story and a dated file in the
/// log folder marks a day log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoteType {
    /// A story note.
    Story,
    /// A project hub note.
    Project,
    /// A day log.
    Log,
    /// Any other declared type.
    Other(String),
    /// No `type` and no recognisable fallback.
    Untyped,
}

/// Story status vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StoryStatus {
    /// Not yet ready to work on.
    Draft,
    /// Ready to start.
    Ready,
    /// Being worked on.
    InProgress,
    /// Paused.
    OnHold,
    /// Finished.
    Done,
    /// Dropped.
    Cancelled,
}

/// A story note's frontmatter and its two task sections.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Story {
    /// The ticket number, such as `WEB-142`.
    pub number: Option<String>,
    /// Host of the tracker the story came from.
    pub source_host: Option<String>,
    /// Record kind on that host.
    pub source_kind: Option<String>,
    /// Record id on that host.
    pub source_id: Option<String>,
    /// The status when it is in the vocabulary.
    pub status: Option<StoryStatus>,
    /// The status as written, for notes outside the vocabulary.
    pub raw_status: Option<String>,
    /// Priority as written.
    pub priority: Option<String>,
    /// Due date when it parses.
    pub due: Option<Date>,
    /// Estimate as written, such as `3h` or `2d`.
    pub estimate: Option<String>,
    /// Manual sort key.
    pub sort_index: Option<i64>,
    /// Alternative names.
    pub aliases: Vec<String>,
    /// The first level-1 heading.
    pub title: Option<String>,
    /// Tasks under `## Next Steps`, nested ones included.
    pub next_steps: Vec<Task>,
    /// Tasks under `## Pending Input`, nested ones included.
    pub pending_input: Vec<Task>,
}

/// A project hub note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectHub {
    /// The ticket key prefix, such as `WEB`.
    pub key: Option<String>,
    /// Status as written.
    pub status: Option<String>,
    /// Manual sort key.
    pub sort_index: Option<i64>,
    /// The first level-1 heading.
    pub title: Option<String>,
    /// Every task in the hub.
    pub tasks: Vec<Task>,
}

/// The verb of a log line, carried in `[kind:: …]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogKind {
    /// A timer session.
    Session,
    /// Something finished.
    Done,
    /// Something in the way.
    Blocker,
    /// A decision taken.
    Decision,
    /// A status update.
    Status,
    /// A free note.
    Note,
    /// Any other kind, kept as written.
    Other(String),
}

/// One `- HH:MM text [kind:: …]` line of a day log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLine {
    /// Line index in the document.
    pub line: usize,
    /// The time prefix.
    pub time: Time,
    /// The verb from `[kind:: …]`, when present.
    pub kind: Option<LogKind>,
    /// The text after the time with the fields removed.
    pub text: String,
    /// Inline fields in source order.
    pub fields: Vec<InlineField>,
}

/// A `Log/YYYY-MM-DD.md` note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DayLog {
    /// The day, from frontmatter `date` or the file name.
    pub date: Date,
    /// Well-formed log lines in order.
    pub lines: Vec<LogLine>,
    /// Line indices of `## Log` items that lack a valid `HH:MM` prefix.
    pub malformed_lines: Vec<usize>,
}

impl NoteType {
    /// Reads the note type. `relative_path` is the note's path inside the
    /// vault and `log_folder` the configured log folder name.
    #[must_use]
    pub fn of(document: &Document, relative_path: &Path, log_folder: &str) -> NoteType {
        let declared = document
            .frontmatter()
            .and_then(|fm| fm.scalar("type"))
            .map(|value| value.trim().to_lowercase())
            .filter(|value| !value.is_empty());
        if let Some(declared) = declared {
            return match declared.as_str() {
                "story" => NoteType::Story,
                "project" => NoteType::Project,
                "log" => NoteType::Log,
                other => NoteType::Other(other.to_owned()),
            };
        }
        if document.frontmatter().is_some_and(|fm| fm.has("number")) {
            return NoteType::Story;
        }
        if log_date_from_path(relative_path, log_folder).is_some() {
            return NoteType::Log;
        }
        NoteType::Untyped
    }
}

impl StoryStatus {
    /// Parses a status token leniently: case-insensitive, spaces or
    /// underscores count as hyphens (`In progress` is `in-progress`).
    #[must_use]
    pub fn parse(text: &str) -> Option<StoryStatus> {
        let normalised = text.trim().to_lowercase().replace([' ', '_'], "-");
        match normalised.as_str() {
            "draft" => Some(StoryStatus::Draft),
            "ready" => Some(StoryStatus::Ready),
            "in-progress" => Some(StoryStatus::InProgress),
            "on-hold" => Some(StoryStatus::OnHold),
            "done" => Some(StoryStatus::Done),
            "cancelled" | "canceled" => Some(StoryStatus::Cancelled),
            _ => None,
        }
    }

    /// The canonical token.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            StoryStatus::Draft => "draft",
            StoryStatus::Ready => "ready",
            StoryStatus::InProgress => "in-progress",
            StoryStatus::OnHold => "on-hold",
            StoryStatus::Done => "done",
            StoryStatus::Cancelled => "cancelled",
        }
    }

    /// True for `done` and `cancelled`.
    #[must_use]
    pub fn is_closed(self) -> bool {
        matches!(self, StoryStatus::Done | StoryStatus::Cancelled)
    }
}

impl Story {
    /// Reads a story from a note whose type resolves to story; `None` otherwise.
    #[must_use]
    pub fn from_document(
        document: &Document,
        relative_path: &Path,
        log_folder: &str,
    ) -> Option<Story> {
        if NoteType::of(document, relative_path, log_folder) != NoteType::Story {
            return None;
        }
        let frontmatter = document.frontmatter();
        let scalar = |key: &str| {
            frontmatter
                .and_then(|fm| fm.scalar(key))
                .filter(|v| !v.is_empty())
        };
        let raw_status = scalar("status");
        Some(Story {
            number: scalar("number"),
            source_host: scalar("source_host"),
            source_kind: scalar("source_kind"),
            source_id: scalar("source_id"),
            status: raw_status.as_deref().and_then(StoryStatus::parse),
            raw_status,
            priority: scalar("priority"),
            due: scalar("due").as_deref().and_then(Date::parse),
            estimate: scalar("estimate"),
            sort_index: frontmatter.and_then(|fm| fm.integer("sort_index")),
            aliases: frontmatter.map(|fm| fm.list("aliases")).unwrap_or_default(),
            title: document.title().map(str::to_owned),
            next_steps: document
                .tasks_under(NEXT_STEPS_HEADING)
                .into_iter()
                .cloned()
                .collect(),
            pending_input: document
                .tasks_under(PENDING_INPUT_HEADING)
                .into_iter()
                .cloned()
                .collect(),
        })
    }
}

impl ProjectHub {
    /// Reads a project hub from a note whose type is project; `None` otherwise.
    #[must_use]
    pub fn from_document(
        document: &Document,
        relative_path: &Path,
        log_folder: &str,
    ) -> Option<ProjectHub> {
        if NoteType::of(document, relative_path, log_folder) != NoteType::Project {
            return None;
        }
        let frontmatter = document.frontmatter();
        let scalar = |key: &str| {
            frontmatter
                .and_then(|fm| fm.scalar(key))
                .filter(|v| !v.is_empty())
        };
        Some(ProjectHub {
            key: scalar("key"),
            status: scalar("status"),
            sort_index: frontmatter.and_then(|fm| fm.integer("sort_index")),
            title: document.title().map(str::to_owned),
            tasks: document.tasks().to_vec(),
        })
    }
}

impl LogKind {
    /// Parses the `[kind:: …]` token.
    #[must_use]
    pub fn parse(text: &str) -> LogKind {
        match text.trim() {
            "session" => LogKind::Session,
            "done" => LogKind::Done,
            "blocker" => LogKind::Blocker,
            "decision" => LogKind::Decision,
            "status" => LogKind::Status,
            "note" => LogKind::Note,
            other => LogKind::Other(other.to_owned()),
        }
    }

    /// The token as written in the vault.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            LogKind::Session => "session",
            LogKind::Done => "done",
            LogKind::Blocker => "blocker",
            LogKind::Decision => "decision",
            LogKind::Status => "status",
            LogKind::Note => "note",
            LogKind::Other(other) => other,
        }
    }
}

impl LogLine {
    /// The value of the first field with `key`.
    #[must_use]
    pub fn field(&self, key: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|field| field.key == key)
            .map(|field| field.value.as_str())
    }

    /// The `[story:: …]` reference.
    #[must_use]
    pub fn story(&self) -> Option<&str> {
        self.field("story")
    }

    /// The `[project:: …]` reference.
    #[must_use]
    pub fn project(&self) -> Option<&str> {
        self.field("project")
    }

    /// The `[task:: …]` reference.
    #[must_use]
    pub fn task(&self) -> Option<&str> {
        self.field("task")
    }
}

impl DayLog {
    /// Reads a day log. The date comes from frontmatter `date`, else from a
    /// `YYYY-MM-DD.md` file name inside `log_folder`; without either, `None`.
    #[must_use]
    pub fn from_document(
        document: &Document,
        relative_path: &Path,
        log_folder: &str,
    ) -> Option<DayLog> {
        let note_type = NoteType::of(document, relative_path, log_folder);
        if !matches!(note_type, NoteType::Log) {
            return None;
        }
        let date = document
            .frontmatter()
            .and_then(|fm| fm.scalar("date"))
            .as_deref()
            .and_then(Date::parse)
            .or_else(|| log_date_from_path(relative_path, log_folder))?;
        let Some(section) = document.section(LOG_HEADING) else {
            return Some(DayLog {
                date,
                lines: Vec::new(),
                malformed_lines: Vec::new(),
            });
        };
        let mut lines = Vec::new();
        let mut malformed_lines = Vec::new();
        for (index, item) in document.list_items() {
            if !section.contains(index) || !item.indent.is_empty() {
                continue;
            }
            match parse_log_item(index, item) {
                Some(line) => lines.push(line),
                None => malformed_lines.push(index),
            }
        }
        Some(DayLog {
            date,
            lines,
            malformed_lines,
        })
    }
}

/// The date encoded in a `<log_folder>/YYYY-MM-DD.md` path.
#[must_use]
pub fn log_date_from_path(relative_path: &Path, log_folder: &str) -> Option<Date> {
    let parent = relative_path.parent()?;
    if parent.as_os_str() != log_folder {
        return None;
    }
    let stem = relative_path.file_stem()?.to_str()?;
    (relative_path.extension().is_some_and(|ext| ext == "md"))
        .then(|| Date::parse(stem))
        .flatten()
}

fn parse_log_item(index: usize, item: &crate::scan::ListItem) -> Option<LogLine> {
    if item.bullet != "-" {
        return None;
    }
    let (time_text, _) = item
        .text
        .split_once(' ')
        .unwrap_or((item.text.as_str(), ""));
    let time = Time::parse(time_text)?;
    let text = item.text.get(time_text.len()..)?.trim().to_owned();
    Some(LogLine {
        line: index,
        time,
        kind: item.field("kind").map(LogKind::parse),
        text,
        fields: item.fields.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::{DayLog, LogKind, NoteType, ProjectHub, Story, StoryStatus, log_date_from_path};
    use crate::scan::Document;
    use std::path::Path;

    #[test]
    fn reads_a_story_with_both_sections() {
        let document = Document::parse(
            "---\ntype: story\nnumber: WEB-142\nstatus: In progress\npriority: high\ndue: 2026-10-30\nestimate: 3h\nsort_index: \"20\"\naliases: [Checkout]\n---\n# Checkout redesign\n## Next Steps\n- [ ] Ship it [id:: task-1]\n\t- [ ] Sub [id:: task-2]\n## Pending Input\n- [ ] Copy from marketing [id:: task-3] [assigned_to:: Dana]\n",
        );
        let story = Story::from_document(&document, Path::new("Projects/W/WEB-142 X.md"), "Log")
            .expect("story");
        assert_eq!(story.number.as_deref(), Some("WEB-142"));
        assert_eq!(story.status, Some(StoryStatus::InProgress));
        assert_eq!(story.raw_status.as_deref(), Some("In progress"));
        assert_eq!(
            story.due.map(|d| d.to_string()).as_deref(),
            Some("2026-10-30")
        );
        assert_eq!(story.sort_index, Some(20));
        assert_eq!(story.aliases, ["Checkout"]);
        assert_eq!(story.title.as_deref(), Some("Checkout redesign"));
        assert_eq!(story.next_steps.len(), 2);
        assert_eq!(story.pending_input.len(), 1);
        assert_eq!(story.pending_input[0].field("assigned_to"), Some("Dana"));
    }

    #[test]
    fn missing_fields_are_none_and_legacy_number_marks_a_story() {
        let document = Document::parse("---\nnumber: ACME-1\n---\n# Bare\n");
        let path = Path::new("x.md");
        assert_eq!(NoteType::of(&document, path, "Log"), NoteType::Story);
        let story = Story::from_document(&document, path, "Log").expect("story");
        assert_eq!(story.status, None);
        assert!(
            story.next_steps.is_empty()
                && story.pending_input.is_empty()
                && story.aliases.is_empty()
        );
        assert!(
            Story::from_document(&Document::parse("# No frontmatter\n"), path, "Log").is_none()
        );
        assert_eq!(
            NoteType::of(&Document::parse("---\ntype: decision\n---\n"), path, "Log"),
            NoteType::Other("decision".to_owned())
        );
        assert_eq!(
            NoteType::of(
                &Document::parse("plain\n"),
                Path::new("Log/2026-10-22.md"),
                "Log"
            ),
            NoteType::Log
        );
        assert_eq!(
            NoteType::of(
                &Document::parse("plain\n"),
                Path::new("Log/notes.md"),
                "Log"
            ),
            NoteType::Untyped
        );
    }

    #[test]
    fn status_vocabulary_is_lenient_on_spelling() {
        assert_eq!(StoryStatus::parse("On Hold"), Some(StoryStatus::OnHold));
        assert_eq!(
            StoryStatus::parse("in_progress"),
            Some(StoryStatus::InProgress)
        );
        assert_eq!(StoryStatus::parse("Canceled"), Some(StoryStatus::Cancelled));
        assert_eq!(StoryStatus::parse("blocked"), None);
        assert!(StoryStatus::Done.is_closed());
    }

    #[test]
    fn reads_a_project_hub() {
        let document = Document::parse(
            "---\ntype: project\nkey: WEB\nstatus: active\n---\n# Website\n- [ ] Hub task [id:: task-5]\n",
        );
        let hub =
            ProjectHub::from_document(&document, Path::new("Projects/Website/Website.md"), "Log")
                .expect("hub");
        assert_eq!(hub.key.as_deref(), Some("WEB"));
        assert_eq!(hub.tasks.len(), 1);
    }

    #[test]
    fn reads_log_lines_inside_the_log_section_only() {
        let document = Document::parse(
            "---\ntype: log\ndate: 2026-10-22\n---\n# 2026-10-22\n\n## Log\n- 09:05 Routing decided [kind:: decision] [story:: WEB-142]\n- 9:40 bad time [kind:: done]\n- 11:30 Drafted steps [kind:: done] [project:: Website] [task:: task-3]\n\t- nested, ignored\n- 16:45 Session closed [kind:: session]\n\n## Journal\n- 17:00 not a log line\n",
        );
        let log =
            DayLog::from_document(&document, Path::new("Log/2026-10-22.md"), "Log").expect("log");
        assert_eq!(log.date.to_string(), "2026-10-22");
        assert_eq!(log.lines.len(), 3);
        assert_eq!(log.malformed_lines, [8]);
        assert_eq!(log.lines[0].time.to_string(), "09:05");
        assert_eq!(log.lines[0].kind, Some(LogKind::Decision));
        assert_eq!(log.lines[0].text, "Routing decided");
        assert_eq!(log.lines[0].story(), Some("WEB-142"));
        assert_eq!(log.lines[1].project(), Some("Website"));
        assert_eq!(log.lines[1].task(), Some("task-3"));
        assert_eq!(log.lines[2].kind, Some(LogKind::Session));
    }

    #[test]
    fn a_log_without_a_section_or_date_is_handled() {
        let dated_by_name = Document::parse("# nothing\n");
        let log = DayLog::from_document(&dated_by_name, Path::new("Log/2026-10-23.md"), "Log")
            .expect("log");
        assert!(log.lines.is_empty());
        assert!(
            DayLog::from_document(
                &Document::parse("---\ntype: log\n---\n"),
                Path::new("Log/notes.md"),
                "Log"
            )
            .is_none()
        );
        assert_eq!(
            log_date_from_path(Path::new("Journal/2026-10-23.md"), "Journal")
                .map(|d| d.to_string())
                .as_deref(),
            Some("2026-10-23")
        );
        assert!(log_date_from_path(Path::new("Log/2026-10-23.txt"), "Log").is_none());
    }
}
