//! A loaded vault and the queries the CLI and the app ask of it: stories with
//! their task sections, a task by id, a day's log lines and waiting-on items.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use thiserror::Error;

use crate::date::Date;
use crate::file_access::{FileAccess, FileAccessError};
use crate::index::{FileIndex, IndexError, IndexScope};
use crate::note::{DayLog, LogLine, NoteType, ProjectHub, Story};
use crate::scan::{Document, Task};
use crate::workspace::Workspace;

/// The assignee token that means the vault's owner.
pub const ME: &str = "me";

/// One parsed note with its typed views.
#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    /// Path relative to the root.
    pub relative_path: PathBuf,
    /// The parsed note.
    pub document: Document,
    /// What the note is.
    pub note_type: NoteType,
    /// The story view when the note is a story.
    pub story: Option<Story>,
    /// The project hub view when the note is one.
    pub project: Option<ProjectHub>,
    /// The day log view when the note is one.
    pub day_log: Option<DayLog>,
}

/// Every note under a root, parsed once. Paths in the queries are relative to
/// that root: the workspace root when opened through [`Vault::open`].
#[derive(Debug, Clone, PartialEq)]
pub struct Vault {
    root: PathBuf,
    log_folder: String,
    notes: BTreeMap<PathBuf, Note>,
    unreadable: Vec<UnreadableNote>,
}

/// A file the index listed that could not be read as a note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnreadableNote {
    /// Path relative to the root.
    pub relative_path: PathBuf,
    /// Why it was skipped.
    pub reason: String,
}

/// A story with the note it lives in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoryEntry<'a> {
    /// Path relative to the root.
    pub path: &'a Path,
    /// The story view, next steps and pending input included.
    pub story: &'a Story,
}

/// A task with the note it lives in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskRef<'a> {
    /// Path relative to the root.
    pub path: &'a Path,
    /// The task line.
    pub task: &'a Task,
}

/// A log line with the note it lives in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry<'a> {
    /// Path relative to the root.
    pub path: &'a Path,
    /// The day of the log.
    pub date: Date,
    /// The line.
    pub line: &'a LogLine,
}

/// An open task assigned to someone else, with how long it has waited.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaitingOn<'a> {
    /// Path relative to the root.
    pub path: &'a Path,
    /// The task line.
    pub task: &'a Task,
    /// Who it waits on.
    pub assigned_to: &'a str,
    /// Days since `[created:: …]`, when the field is present and parses.
    pub age_days: Option<i64>,
    /// The story number when the note is a story.
    pub story_number: Option<&'a str>,
}

/// Why a vault could not be loaded.
#[derive(Debug, Error)]
pub enum VaultError {
    /// The index scope does not compile.
    #[error(transparent)]
    Index(#[from] IndexError),
    /// The file system failed.
    #[error(transparent)]
    FileAccess(#[from] FileAccessError),
}

impl Vault {
    /// Loads every file of a refreshed index from the file system.
    ///
    /// # Errors
    /// I/O errors while reading a note. A note that vanished between the
    /// refresh and the read, or that is not UTF-8, is skipped and listed in
    /// [`Vault::unreadable`] so one bad file never takes down every query.
    pub fn load(
        index: &FileIndex,
        log_folder: &str,
        file_access: &dyn FileAccess,
    ) -> Result<Vault, VaultError> {
        let mut notes = BTreeMap::new();
        let mut unreadable = Vec::new();
        for (relative_path, _) in index.files() {
            let text = match file_access.read_to_string(&index.root().join(relative_path)) {
                Ok(text) => text,
                Err(FileAccessError::NotFound(_)) => continue,
                Err(error @ FileAccessError::InvalidUtf8(_)) => {
                    unreadable.push(UnreadableNote {
                        relative_path: relative_path.to_path_buf(),
                        reason: error.to_string(),
                    });
                    continue;
                }
                Err(error) => return Err(error.into()),
            };
            let note = Note::parse(relative_path, &text, log_folder);
            notes.insert(relative_path.to_path_buf(), note);
        }
        Ok(Vault {
            root: index.root().to_path_buf(),
            log_folder: log_folder.to_owned(),
            notes,
            unreadable,
        })
    }

    /// Builds an index over the workspace root with `scope`, refreshes it at
    /// `now` and loads the notes. Folders that are workspaces of their own are
    /// never descended into, so a nested client workspace stays out of the
    /// enclosing one.
    ///
    /// # Errors
    /// Index or file errors.
    pub fn open(
        workspace: &Workspace,
        scope: IndexScope,
        file_access: &dyn FileAccess,
        now: SystemTime,
    ) -> Result<Vault, VaultError> {
        let mut index = FileIndex::new(&workspace.root, scope)?;
        index.refresh(file_access, now)?;
        Vault::load(&index, &workspace.settings.log_folder, file_access)
    }

    /// The root the relative paths hang off.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The configured log folder name.
    #[must_use]
    pub fn log_folder(&self) -> &str {
        &self.log_folder
    }

    /// Every note in path order.
    pub fn notes(&self) -> impl Iterator<Item = &Note> {
        self.notes.values()
    }

    /// One note by its root-relative path.
    #[must_use]
    pub fn note(&self, relative_path: &Path) -> Option<&Note> {
        self.notes.get(relative_path)
    }

    /// Number of notes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.notes.len()
    }

    /// Files the index listed that could not be read as notes.
    #[must_use]
    pub fn unreadable(&self) -> &[UnreadableNote] {
        &self.unreadable
    }

    /// True without notes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.notes.is_empty()
    }

    /// Every story with its Next Steps and Pending Input tasks, ordered by
    /// `sort_index` (missing last), then number (missing last), then path.
    #[must_use]
    pub fn stories(&self) -> Vec<StoryEntry<'_>> {
        let mut stories: Vec<StoryEntry<'_>> = self
            .notes
            .values()
            .filter_map(|note| {
                note.story.as_ref().map(|story| StoryEntry {
                    path: note.relative_path.as_path(),
                    story,
                })
            })
            .collect();
        stories.sort_by(|a, b| {
            sort_index_key(a.story.sort_index)
                .cmp(&sort_index_key(b.story.sort_index))
                .then_with(|| {
                    number_key(a.story.number.as_deref())
                        .cmp(&number_key(b.story.number.as_deref()))
                })
                .then_with(|| a.path.cmp(b.path))
        });
        stories
    }

    /// The story whose number equals `number`.
    #[must_use]
    pub fn story_by_number(&self, number: &str) -> Option<StoryEntry<'_>> {
        self.stories()
            .into_iter()
            .find(|entry| entry.story.number.as_deref() == Some(number))
    }

    /// Every project hub with its path.
    #[must_use]
    pub fn projects(&self) -> Vec<(&Path, &ProjectHub)> {
        self.notes
            .values()
            .filter_map(|note| {
                note.project
                    .as_ref()
                    .map(|hub| (note.relative_path.as_path(), hub))
            })
            .collect()
    }

    /// Every task line in every note, in path then line order.
    pub fn tasks(&self) -> impl Iterator<Item = TaskRef<'_>> {
        self.notes.values().flat_map(|note| {
            note.document.tasks().iter().map(move |task| TaskRef {
                path: note.relative_path.as_path(),
                task,
            })
        })
    }

    /// The task carrying `[id:: id]`. Ids are meant to be unique across the
    /// vault; when two lines carry the same id the first in path order, then
    /// line order, wins and the other is still listed by [`Vault::tasks`].
    #[must_use]
    pub fn task_by_id(&self, id: &str) -> Option<TaskRef<'_>> {
        self.tasks().find(|task_ref| task_ref.task.id() == Some(id))
    }

    /// The log lines of `date`, in time order; empty without a log for that day.
    #[must_use]
    pub fn log_lines_on(&self, date: Date) -> Vec<LogEntry<'_>> {
        let mut entries: Vec<LogEntry<'_>> = self
            .notes
            .values()
            .filter_map(|note| note.day_log.as_ref().map(|log| (note, log)))
            .filter(|(_, log)| log.date == date)
            .flat_map(|(note, log)| {
                log.lines.iter().map(move |line| LogEntry {
                    path: note.relative_path.as_path(),
                    date: log.date,
                    line,
                })
            })
            .collect();
        entries.sort_by(|a, b| {
            a.line
                .time
                .cmp(&b.line.time)
                .then_with(|| a.line.line.cmp(&b.line.line))
        });
        entries
    }

    /// Open tasks whose `[assigned_to:: …]` is set and is not `me`, with their
    /// age in days since `[created:: …]` as of `today`. Oldest first. The
    /// comparison ignores case and a leading `@`, so `Me` and `@me` are me.
    #[must_use]
    pub fn waiting_on(&self, me: &str, today: Date) -> Vec<WaitingOn<'_>> {
        let mut items: Vec<WaitingOn<'_>> = self
            .notes
            .values()
            .flat_map(|note| {
                let story_number = note
                    .story
                    .as_ref()
                    .and_then(|story| story.number.as_deref());
                note.document.tasks().iter().filter_map(move |task| {
                    let assigned_to = task
                        .field("assigned_to")
                        .filter(|who| !who.is_empty() && !is_same_person(who, me))?;
                    task.is_open().then(|| WaitingOn {
                        path: note.relative_path.as_path(),
                        task,
                        assigned_to,
                        age_days: task
                            .field("created")
                            .and_then(Date::parse)
                            .map(|created| created.days_until(today)),
                        story_number,
                    })
                })
            })
            .collect();
        items.sort_by(|a, b| {
            b.age_days
                .cmp(&a.age_days)
                .then_with(|| a.path.cmp(b.path))
                .then_with(|| a.task.line.cmp(&b.task.line))
        });
        items
    }
}

impl Note {
    /// Parses a note's text and computes its typed views.
    #[must_use]
    pub fn parse(relative_path: &Path, text: &str, log_folder: &str) -> Note {
        let document = Document::parse(text);
        let note_type = NoteType::of(&document, relative_path, log_folder);
        let story = Story::from_document(&document, relative_path, log_folder);
        let project = ProjectHub::from_document(&document, relative_path, log_folder);
        let day_log = DayLog::from_document(&document, relative_path, log_folder);
        Note {
            relative_path: relative_path.to_path_buf(),
            document,
            note_type,
            story,
            project,
            day_log,
        }
    }
}

/// Assignee tokens match without case and without a leading `@`.
fn is_same_person(assigned_to: &str, me: &str) -> bool {
    let normalise = |who: &str| who.trim().trim_start_matches('@').to_lowercase();
    normalise(assigned_to) == normalise(me)
}

/// Sort key that places stories without a number after those with one.
fn number_key(number: Option<&str>) -> (bool, &str) {
    match number {
        Some(value) => (false, value),
        None => (true, ""),
    }
}

/// Sort key that places stories without a `sort_index` after those with one.
fn sort_index_key(sort_index: Option<i64>) -> (bool, i64) {
    match sort_index {
        Some(value) => (false, value),
        None => (true, 0),
    }
}
