//! The write discipline. The vault is the source of truth, so every write is
//! line-scoped: the file is read again, the line is found by its `[id:: ]`
//! field, only that line is replaced, and the whole file is written back
//! atomically (temporary file and rename). Files are never held open, a write
//! that races another writer is detected and retried, and bursts of edits to
//! one file are coalesced by [`WriteQueue`].
//!
//! [`VaultWriter`] offers the operations the app and the CLI need: tick,
//! untick, set `scheduled` / `due` / `estimate`, change text, add a task or a
//! subtask, create a story from the workspace template, and append a day-log
//! line. Lines inside `%% ai:… %%` provenance blocks are only ever replaced
//! one line at a time; nothing is inserted into a model-text block.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use thiserror::Error;

use crate::buffer::{LineBuffer, end_of_section_content, outside_model_text, status_char_range};
use crate::date::Date;
use crate::file_access::{FileAccess, FileAccessError};
use crate::log::{LogAppend, LogFormatError, day_log_path, format_log_line, new_day_log_header};
use crate::note::{LOG_HEADING, NEXT_STEPS_HEADING, PENDING_INPUT_HEADING};
use crate::scan::{Document, InlineField, Task, TaskStatus, inline_fields};
use crate::workspace::Workspace;

/// How long a burst of edits to one file is held back by default.
pub const DEFAULT_DEBOUNCE: Duration = Duration::from_millis(400);

/// The story template used when the workspace names none.
pub const DEFAULT_STORY_TEMPLATE: &str = "---\ntype: story\nnumber:\nstatus: draft\npriority:\ndue:\nestimate:\nsort_index:\nnote_system: \"1.0\"\n---\n# Description\n\n## Next Steps\n- [ ] \n\n## Pending Input\n- [ ] \n";

const MAX_WRITE_ATTEMPTS: usize = 3;
const COMPLETION_KEY: &str = "completion";
/// The line that closes a model-text block.
const MODEL_BLOCK_END: &str = "%% ai:end %%";

/// What one write changed: the file, the 1-based line number, and the line
/// before and after. `old_line` is `None` for a line that was inserted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteReceipt {
    /// The note, relative to the workspace root.
    pub path: PathBuf,
    /// 1-based line number of the changed line in the new file.
    pub line_number: usize,
    /// The line as it was; `None` when the line is new.
    pub old_line: Option<String>,
    /// The line as it is now.
    pub new_line: String,
}

/// A task named by the note that holds it and its id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskTarget {
    /// The note, relative to the workspace root (or absolute inside it).
    pub path: PathBuf,
    /// The `[id:: …]` value.
    pub id: String,
    /// The line as the caller last saw it. When set and the line has changed
    /// since, the write is refused with [`WriteError::TaskLineChanged`].
    pub expected_line: Option<String>,
}

impl TaskTarget {
    /// A target without a staleness check.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>, id: impl Into<String>) -> TaskTarget {
        TaskTarget {
            path: path.into(),
            id: id.into(),
            expected_line: None,
        }
    }

    /// Refuses the write when the line no longer equals `line`.
    #[must_use]
    pub fn expecting_line(mut self, line: impl Into<String>) -> TaskTarget {
        self.expected_line = Some(line.into());
        self
    }
}

/// A change to one task line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskEdit {
    /// `[x]` plus `[completion:: date]`. A task that is already done with a
    /// completion date is left as it is.
    Done(Date),
    /// `[ ]` and no `[completion:: …]`.
    Reopen,
    /// Sets or, with `None`, removes `[scheduled:: …]`.
    Schedule(Option<Date>),
    /// Sets or, with `None`, removes `[due:: …]`.
    Due(Option<Date>),
    /// Sets or, with `None`, removes `[estimate:: …]`.
    Estimate(Option<String>),
    /// Replaces the description, up to the first inline field.
    Text(String),
}

/// The two task sections of a story.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskSection {
    /// `## Next Steps`.
    NextSteps,
    /// `## Pending Input`.
    PendingInput,
}

impl TaskSection {
    /// The heading title.
    #[must_use]
    pub fn heading(self) -> &'static str {
        match self {
            TaskSection::NextSteps => NEXT_STEPS_HEADING,
            TaskSection::PendingInput => PENDING_INPUT_HEADING,
        }
    }
}

/// A task line to add: `- [ ] text [id:: …] [created:: …] [assigned_to:: …]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTask {
    /// The description, one line without inline fields.
    pub text: String,
    /// The new task's id; the caller allocates it.
    pub id: String,
    /// The creation date.
    pub created: Date,
    /// Who the task waits on, for Pending Input.
    pub assigned_to: Option<String>,
}

/// A story note to create from the workspace template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewStory {
    /// The story number, such as `WEB-142`.
    pub number: String,
    /// The title, which becomes the note's `# ` heading.
    pub title: String,
    /// The folder, relative to the workspace root, that gets the story's own
    /// folder `<number> <title>/`.
    pub project_folder: PathBuf,
}

/// Why a write was refused or failed.
#[derive(Debug, Error)]
pub enum WriteError {
    /// The note does not exist.
    #[error("{0} does not exist")]
    NoteNotFound(PathBuf),
    /// No task line in the note carries the id.
    #[error("no task with [id:: {id}] in {path}")]
    TaskNotFound {
        /// The id looked for.
        id: String,
        /// The note searched.
        path: PathBuf,
    },
    /// Several task lines in the note carry the id, so the line is ambiguous.
    #[error("[id:: {id}] appears on several task lines of {path} (lines {lines:?})")]
    DuplicateTaskId {
        /// The id.
        id: String,
        /// The note.
        path: PathBuf,
        /// 1-based line numbers.
        lines: Vec<usize>,
    },
    /// A new task's id is already used in the note.
    #[error("[id:: {id}] is already used in {path}")]
    IdAlreadyUsed {
        /// The id.
        id: String,
        /// The note.
        path: PathBuf,
    },
    /// The line differs from the one the caller based its edit on.
    #[error("the line of [id:: {id}] in {path} changed since it was read: found {found:?}")]
    TaskLineChanged {
        /// The id.
        id: String,
        /// The note.
        path: PathBuf,
        /// The line as it is now.
        found: String,
    },
    /// The file kept changing while it was being written.
    #[error("{0} kept changing while it was being written; nothing was written")]
    FileChangedWhileWriting(PathBuf),
    /// The file to create exists already.
    #[error("{0} already exists")]
    AlreadyExists(PathBuf),
    /// The story template named by the workspace is missing.
    #[error("story template {0} does not exist")]
    TemplateNotFound(PathBuf),
    /// The path leaves the workspace.
    #[error("{0} is not inside the workspace")]
    OutsideWorkspace(PathBuf),
    /// The only place for the line is inside a provenance block.
    #[error("{path} has no place for the line outside a model-text block")]
    InsideModelText {
        /// The note.
        path: PathBuf,
    },
    /// An argument is not acceptable.
    #[error("{0}")]
    InvalidInput(String),
    /// A log line broke the day-log grammar.
    #[error(transparent)]
    Log(#[from] LogFormatError),
    /// The edit was not applied because another edit of the same write failed.
    #[error("{0}: another edit of the same write failed")]
    BatchFailed(PathBuf),
    /// A file operation failed.
    #[error(transparent)]
    File(#[from] FileAccessError),
}

impl WriteError {
    /// True when the error says something was not found.
    #[must_use]
    pub fn is_not_found(&self) -> bool {
        matches!(
            self,
            WriteError::NoteNotFound(_)
                | WriteError::TaskNotFound { .. }
                | WriteError::TemplateNotFound(_)
        )
    }

    /// True when the error says the vault changed under the caller or the
    /// request clashes with what is there.
    #[must_use]
    pub fn is_conflict(&self) -> bool {
        matches!(
            self,
            WriteError::DuplicateTaskId { .. }
                | WriteError::IdAlreadyUsed { .. }
                | WriteError::TaskLineChanged { .. }
                | WriteError::FileChangedWhileWriting(_)
                | WriteError::AlreadyExists(_)
                | WriteError::InsideModelText { .. }
        )
    }
}

/// Writes to one workspace through a [`FileAccess`].
pub struct VaultWriter<'a> {
    pub(crate) file_access: &'a dyn FileAccess,
    pub(crate) root: PathBuf,
    log_folder: String,
    story_template: Option<String>,
}

impl std::fmt::Debug for VaultWriter<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("VaultWriter")
            .field("root", &self.root)
            .field("log_folder", &self.log_folder)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingChange {
    id: String,
    expected_line: Option<String>,
    edit: TaskEdit,
}

impl<'a> VaultWriter<'a> {
    /// A writer for `workspace`: its root, `log_folder` and `story_template`.
    #[must_use]
    pub fn new(file_access: &'a dyn FileAccess, workspace: &Workspace) -> VaultWriter<'a> {
        VaultWriter {
            file_access,
            root: workspace.root.clone(),
            log_folder: workspace.settings.log_folder.clone(),
            story_template: workspace.settings.story_template.clone(),
        }
    }

    /// The workspace root every path is relative to.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Applies one edit to the task line named by `target`.
    ///
    /// # Errors
    /// [`WriteError`] when the note or the task is missing, the id is
    /// ambiguous, the line changed since the caller read it, a value is
    /// invalid, or the file cannot be written.
    pub fn edit_task(
        &self,
        target: &TaskTarget,
        edit: &TaskEdit,
    ) -> Result<WriteReceipt, WriteError> {
        let change = PendingChange {
            id: target.id.clone(),
            expected_line: target.expected_line.clone(),
            edit: edit.clone(),
        };
        let mut results = self.apply_changes(&target.path, &[change])?;
        results
            .pop()
            .unwrap_or_else(|| Err(WriteError::BatchFailed(target.path.clone())))
    }

    /// Applies several edits to tasks of one note in a single read and write.
    /// The outer error is a failure of the whole write; each inner result is
    /// that edit's own outcome, so one missing id does not block the others.
    ///
    /// # Errors
    /// [`WriteError`] when the note is missing or cannot be written.
    pub fn edit_tasks(
        &self,
        path: &Path,
        edits: &[(String, TaskEdit)],
    ) -> Result<Vec<Result<WriteReceipt, WriteError>>, WriteError> {
        let changes: Vec<PendingChange> = edits
            .iter()
            .map(|(id, edit)| PendingChange {
                id: id.clone(),
                expected_line: None,
                edit: edit.clone(),
            })
            .collect();
        self.apply_changes(path, &changes)
    }

    /// Ticks a task: `[x]` and `[completion:: completion]`.
    ///
    /// # Errors
    /// See [`VaultWriter::edit_task`].
    pub fn tick_done(
        &self,
        target: &TaskTarget,
        completion: Date,
    ) -> Result<WriteReceipt, WriteError> {
        self.edit_task(target, &TaskEdit::Done(completion))
    }

    /// Unticks a task: `[ ]` without a completion date.
    ///
    /// # Errors
    /// See [`VaultWriter::edit_task`].
    pub fn untick(&self, target: &TaskTarget) -> Result<WriteReceipt, WriteError> {
        self.edit_task(target, &TaskEdit::Reopen)
    }

    /// Sets or clears `[scheduled:: …]`.
    ///
    /// # Errors
    /// See [`VaultWriter::edit_task`].
    pub fn set_scheduled(
        &self,
        target: &TaskTarget,
        scheduled: Option<Date>,
    ) -> Result<WriteReceipt, WriteError> {
        self.edit_task(target, &TaskEdit::Schedule(scheduled))
    }

    /// Sets or clears `[due:: …]`.
    ///
    /// # Errors
    /// See [`VaultWriter::edit_task`].
    pub fn set_due(
        &self,
        target: &TaskTarget,
        due: Option<Date>,
    ) -> Result<WriteReceipt, WriteError> {
        self.edit_task(target, &TaskEdit::Due(due))
    }

    /// Sets or clears `[estimate:: …]` (`2h`, `45m`, `3d`).
    ///
    /// # Errors
    /// See [`VaultWriter::edit_task`]; an estimate with brackets or line
    /// breaks is [`WriteError::InvalidInput`].
    pub fn set_estimate(
        &self,
        target: &TaskTarget,
        estimate: Option<&str>,
    ) -> Result<WriteReceipt, WriteError> {
        self.edit_task(target, &TaskEdit::Estimate(estimate.map(str::to_owned)))
    }

    /// Changes a task's description (`cc`), keeping its fields.
    ///
    /// # Errors
    /// See [`VaultWriter::edit_task`]; empty text, line breaks and inline
    /// fields in the text are [`WriteError::InvalidInput`].
    pub fn set_text(&self, target: &TaskTarget, text: &str) -> Result<WriteReceipt, WriteError> {
        self.edit_task(target, &TaskEdit::Text(text.to_owned()))
    }

    /// Adds a task line at the end of a story's `## Next Steps` or
    /// `## Pending Input`. A missing section is created: Next Steps before
    /// Pending Input, Pending Input after Next Steps, otherwise at the end of
    /// the note. An empty `- [ ] ` placeholder from the template is filled
    /// instead of leaving it behind.
    ///
    /// # Errors
    /// [`WriteError`] when the note is missing, the id is already used there,
    /// a value is invalid, or the only insertion point lies in a model-text
    /// block.
    pub fn add_task(
        &self,
        path: &Path,
        section: TaskSection,
        task: &NewTask,
    ) -> Result<WriteReceipt, WriteError> {
        let (relative, absolute) = self.resolve(path)?;
        let line = new_task_line("", task)?;
        let mut receipts = Vec::new();
        self.rewrite(&relative, &absolute, |buffer| {
            let document = buffer.document();
            ensure_id_unused(&document, &task.id, &relative)?;
            match empty_placeholder_line(&document, section) {
                Some(line_index) => buffer.replace(line_index, line.clone()),
                None => insert_into_section(buffer, section.heading(), line.clone(), &relative)?,
            }
            receipts = receipts_from(buffer, &relative);
            Ok(())
        })?;
        single_receipt(receipts, &relative)
    }

    /// Adds a task line like [`VaultWriter::add_task`], wrapped in a model-text
    /// block: `begin_marker`, the line, `%% ai:end %%`. The caller computes the
    /// marker's fingerprint over the line from [`task_line`], so the provenance
    /// tooling reads the block as intact. An empty template placeholder is
    /// replaced by the block.
    ///
    /// # Errors
    /// As [`VaultWriter::add_task`]; [`WriteError::InvalidInput`] when
    /// `begin_marker` is not a single `%% ai:begin … %%` line.
    pub fn add_model_task(
        &self,
        path: &Path,
        section: TaskSection,
        task: &NewTask,
        begin_marker: &str,
    ) -> Result<WriteReceipt, WriteError> {
        if !is_begin_marker(begin_marker) {
            return Err(WriteError::InvalidInput(
                "the model-text marker must be one `%% ai:begin … %%` line".to_owned(),
            ));
        }
        let (relative, absolute) = self.resolve(path)?;
        let line = task_line(task)?;
        let mut receipts = Vec::new();
        self.rewrite(&relative, &absolute, |buffer| {
            let document = buffer.document();
            ensure_id_unused(&document, &task.id, &relative)?;
            let placeholder = empty_placeholder_line(&document, section);
            let start = match placeholder {
                Some(line_index) => {
                    buffer.replace(line_index, begin_marker.to_owned());
                    line_index + 1
                }
                None => {
                    insert_into_section(
                        buffer,
                        section.heading(),
                        begin_marker.to_owned(),
                        &relative,
                    )?;
                    buffer.changes().last().map_or(0, |change| change.index + 1)
                }
            };
            buffer.insert(start, line.clone());
            buffer.insert(start + 1, MODEL_BLOCK_END.to_owned());
            receipts = receipts_from(buffer, &relative);
            Ok(())
        })?;
        receipts
            .into_iter()
            .find(|receipt| receipt.new_line == line)
            .ok_or(WriteError::BatchFailed(relative))
    }

    /// Adds a subtask under the task named by `parent`, after the parent's
    /// last existing child, indented like its siblings (one tab otherwise).
    ///
    /// # Errors
    /// As [`VaultWriter::add_task`]; a parent that is itself model text is
    /// [`WriteError::InsideModelText`].
    pub fn add_subtask(
        &self,
        parent: &TaskTarget,
        task: &NewTask,
    ) -> Result<WriteReceipt, WriteError> {
        let (relative, absolute) = self.resolve(&parent.path)?;
        let mut receipts = Vec::new();
        self.rewrite(&relative, &absolute, |buffer| {
            let document = buffer.document();
            ensure_id_unused(&document, &task.id, &relative)?;
            let parent_task = find_task(&document, &parent.id, &relative)?;
            check_expected_line(&document, parent_task, parent, &relative)?;
            if document.is_model_text(parent_task.line) {
                return Err(WriteError::InsideModelText {
                    path: relative.clone(),
                });
            }
            let indent = child_indent(&document, parent_task);
            let line = new_task_line(&indent, task)?;
            let at = subtree_end(buffer, parent_task);
            buffer.insert(at, line);
            receipts = receipts_from(buffer, &relative);
            Ok(())
        })?;
        single_receipt(receipts, &relative)
    }

    /// Creates a story note from the workspace's story template (or
    /// [`DEFAULT_STORY_TEMPLATE`]) at
    /// `<project_folder>/<number> <title>/<number> <title>.md`. The template's
    /// `number:` frontmatter line is set and its first `# ` heading becomes
    /// the title; `{{number}}` and `{{title}}` placeholders are honoured when
    /// the template has them.
    ///
    /// # Errors
    /// [`WriteError::AlreadyExists`] when the note exists,
    /// [`WriteError::TemplateNotFound`] when the named template is missing,
    /// [`WriteError::InvalidInput`] for an unusable number or title.
    pub fn new_story(&self, story: &NewStory) -> Result<WriteReceipt, WriteError> {
        validate_story(story)?;
        let template = self.read_story_template()?;
        let (contents, title_line) = fill_story_template(&template, story);
        let folder_name = format!("{} {}", story.number, file_name_safe(&story.title)?);
        let requested = story
            .project_folder
            .join(&folder_name)
            .join(format!("{folder_name}.md"));
        let (relative, absolute) = self.resolve(&requested)?;
        if self.file_access.stat(&absolute)?.is_some() {
            return Err(WriteError::AlreadyExists(relative));
        }
        if let Some(parent) = absolute.parent() {
            self.file_access.create_dir_all(parent)?;
        }
        self.file_access.write_atomic(&absolute, &contents)?;
        Ok(WriteReceipt {
            path: relative,
            line_number: title_line + 1,
            old_line: None,
            new_line: format!("# {}", story.title.trim()),
        })
    }

    /// Appends one line to `<log_folder>/<date>.md`, creating the file with
    /// the convention's header when the day has no log yet and the `## Log`
    /// section when the note lacks one. See [`crate::log`] for the format.
    ///
    /// # Errors
    /// [`WriteError::Log`] when the line breaks the grammar, otherwise file
    /// errors; [`WriteError::InsideModelText`] when the log's only place for
    /// the line is a model-text block.
    pub fn append_log(&self, date: Date, entry: &LogAppend) -> Result<WriteReceipt, WriteError> {
        let line = format_log_line(entry)?;
        let (relative, absolute) = self.resolve(&day_log_path(&self.log_folder, date))?;
        if self.file_access.stat(&absolute)?.is_none() {
            let header = new_day_log_header(date);
            let line_number = header.lines().count() + 1;
            if let Some(parent) = absolute.parent() {
                self.file_access.create_dir_all(parent)?;
            }
            self.file_access
                .write_atomic(&absolute, &format!("{header}{line}\n"))?;
            return Ok(WriteReceipt {
                path: relative,
                line_number,
                old_line: None,
                new_line: line,
            });
        }
        let mut receipts = Vec::new();
        self.rewrite(&relative, &absolute, |buffer| {
            insert_into_section(buffer, LOG_HEADING, line.clone(), &relative)?;
            receipts = receipts_from(buffer, &relative);
            Ok(())
        })?;
        single_receipt(receipts, &relative)
    }

    fn apply_changes(
        &self,
        path: &Path,
        changes: &[PendingChange],
    ) -> Result<Vec<Result<WriteReceipt, WriteError>>, WriteError> {
        let (relative, absolute) = self.resolve(path)?;
        let mut outcomes = Vec::new();
        self.rewrite(&relative, &absolute, |buffer| {
            outcomes.clear();
            for change in changes {
                outcomes.push(apply_change(buffer, change, &relative));
            }
            Ok(())
        })?;
        Ok(outcomes)
    }

    fn read_story_template(&self) -> Result<String, WriteError> {
        let Some(template_path) = &self.story_template else {
            return Ok(DEFAULT_STORY_TEMPLATE.to_owned());
        };
        let (relative, absolute) = self.resolve(Path::new(template_path))?;
        match self.file_access.read_to_string(&absolute) {
            Ok(text) => Ok(text),
            Err(FileAccessError::NotFound(_)) => Err(WriteError::TemplateNotFound(relative)),
            Err(error) => Err(error.into()),
        }
    }

    /// Splits `path` into the root-relative path and the absolute path,
    /// refusing anything that leaves the root.
    pub(crate) fn resolve(&self, path: &Path) -> Result<(PathBuf, PathBuf), WriteError> {
        let relative = if path.is_absolute() {
            path.strip_prefix(&self.root)
                .map_err(|_| WriteError::OutsideWorkspace(path.to_path_buf()))?
        } else {
            path
        };
        let mut clean = PathBuf::new();
        for component in relative.components() {
            match component {
                Component::Normal(part) => clean.push(part),
                Component::CurDir => {}
                _ => return Err(WriteError::OutsideWorkspace(path.to_path_buf())),
            }
        }
        if clean.as_os_str().is_empty() {
            return Err(WriteError::OutsideWorkspace(path.to_path_buf()));
        }
        let absolute = self.root.join(&clean);
        Ok((clean, absolute))
    }

    /// Re-reads the note, lets `edit` change a line buffer, and writes the
    /// result atomically unless the note changed on disk in between, in which
    /// case it starts over from a fresh read.
    pub(crate) fn rewrite(
        &self,
        relative: &Path,
        absolute: &Path,
        mut edit: impl FnMut(&mut LineBuffer) -> Result<(), WriteError>,
    ) -> Result<(), WriteError> {
        for _ in 0..MAX_WRITE_ATTEMPTS {
            let Some(before) = self.stamp(absolute)? else {
                return Err(WriteError::NoteNotFound(relative.to_path_buf()));
            };
            let text = match self.file_access.read_to_string(absolute) {
                Ok(text) => text,
                Err(FileAccessError::NotFound(_)) => {
                    return Err(WriteError::NoteNotFound(relative.to_path_buf()));
                }
                Err(error) => return Err(error.into()),
            };
            let mut buffer = LineBuffer::parse(&text);
            edit(&mut buffer)?;
            let rendered = buffer.render();
            if rendered == text {
                return Ok(());
            }
            if self.stamp(absolute)? != Some(before) {
                continue;
            }
            self.file_access.write_atomic(absolute, &rendered)?;
            return Ok(());
        }
        Err(WriteError::FileChangedWhileWriting(relative.to_path_buf()))
    }

    fn stamp(&self, absolute: &Path) -> Result<Option<(Option<SystemTime>, u64)>, WriteError> {
        Ok(self
            .file_access
            .stat(absolute)?
            .map(|entry| (entry.modified, entry.len)))
    }
}

/// Coalesces bursts of task edits. Edits are pushed with the time they
/// arrived; a file's edits are written together once no new edit for that
/// file has arrived for the debounce window. A later edit of the same kind to
/// the same task replaces the earlier one (typing `cc` text, dragging a
/// date), so only the last value is written. Time is supplied by the caller,
/// so the queue does no sleeping of its own.
#[derive(Debug)]
pub struct WriteQueue {
    window: Duration,
    files: BTreeMap<PathBuf, QueuedFile>,
}

#[derive(Debug)]
struct QueuedFile {
    last_pushed: Instant,
    changes: Vec<(String, TaskEdit)>,
}

/// What happened to one queued edit.
#[derive(Debug)]
pub struct QueueOutcome {
    /// The note the edit was for, as it was pushed.
    pub path: PathBuf,
    /// The task id.
    pub id: String,
    /// The receipt, or why the edit was not written.
    pub result: Result<WriteReceipt, WriteError>,
}

impl WriteQueue {
    /// A queue that holds a file's edits back for `window` after the last one.
    #[must_use]
    pub fn new(window: Duration) -> WriteQueue {
        WriteQueue {
            window,
            files: BTreeMap::new(),
        }
    }

    /// Queues an edit that arrived at `now`.
    pub fn push(
        &mut self,
        path: impl Into<PathBuf>,
        id: impl Into<String>,
        edit: TaskEdit,
        now: Instant,
    ) {
        let id = id.into();
        let file = self.files.entry(path.into()).or_insert_with(|| QueuedFile {
            last_pushed: now,
            changes: Vec::new(),
        });
        file.last_pushed = now;
        file.changes
            .retain(|(queued_id, queued)| !(*queued_id == id && same_slot(queued, &edit)));
        file.changes.push((id, edit));
    }

    /// Number of edits waiting.
    #[must_use]
    pub fn pending(&self) -> usize {
        self.files.values().map(|file| file.changes.len()).sum()
    }

    /// When the earliest file becomes due, `None` for an empty queue.
    #[must_use]
    pub fn next_due(&self) -> Option<Instant> {
        self.files
            .values()
            .map(|file| file.last_pushed + self.window)
            .min()
    }

    /// Writes every file whose window has passed at `now`.
    pub fn flush_due(&mut self, writer: &VaultWriter<'_>, now: Instant) -> Vec<QueueOutcome> {
        let due: Vec<PathBuf> = self
            .files
            .iter()
            .filter(|(_, file)| file.last_pushed + self.window <= now)
            .map(|(path, _)| path.clone())
            .collect();
        self.flush_paths(writer, due)
    }

    /// Writes everything waiting, due or not (at exit, or before a read).
    pub fn flush_all(&mut self, writer: &VaultWriter<'_>) -> Vec<QueueOutcome> {
        let all: Vec<PathBuf> = self.files.keys().cloned().collect();
        self.flush_paths(writer, all)
    }

    fn flush_paths(&mut self, writer: &VaultWriter<'_>, paths: Vec<PathBuf>) -> Vec<QueueOutcome> {
        let mut outcomes = Vec::new();
        for path in paths {
            let Some(file) = self.files.remove(&path) else {
                continue;
            };
            match writer.edit_tasks(&path, &file.changes) {
                Ok(results) => {
                    for ((id, _), result) in file.changes.into_iter().zip(results) {
                        outcomes.push(QueueOutcome {
                            path: path.clone(),
                            id,
                            result,
                        });
                    }
                }
                Err(error) => {
                    let mut failure = Some(error);
                    for (id, _) in file.changes {
                        let result = Err(failure
                            .take()
                            .unwrap_or_else(|| WriteError::BatchFailed(path.clone())));
                        outcomes.push(QueueOutcome {
                            path: path.clone(),
                            id,
                            result,
                        });
                    }
                }
            }
        }
        outcomes
    }
}

impl Default for WriteQueue {
    fn default() -> Self {
        WriteQueue::new(DEFAULT_DEBOUNCE)
    }
}

/// Inserts `line` at the end of the section titled `title`, creating the
/// section when the note lacks it. Next Steps goes before Pending Input and
/// Pending Input after Next Steps; any other section goes at the end.
pub(crate) fn insert_into_section(
    buffer: &mut LineBuffer,
    title: &str,
    line: String,
    relative: &Path,
) -> Result<(), WriteError> {
    let document = buffer.document();
    let inside_model_text = || WriteError::InsideModelText {
        path: relative.to_path_buf(),
    };
    if let Some(section) = document.section(title) {
        let at = outside_model_text(&document, end_of_section_content(buffer, &section))
            .ok_or_else(inside_model_text)?;
        buffer.insert(at, line);
        return Ok(());
    }
    let wanted_at = if title == NEXT_STEPS_HEADING {
        document
            .section(PENDING_INPUT_HEADING)
            .and_then(|pending| pending.heading_line)
    } else if title == PENDING_INPUT_HEADING {
        document
            .section(NEXT_STEPS_HEADING)
            .map(|next_steps| next_steps.end)
    } else {
        None
    };
    let at = outside_model_text(&document, wanted_at.unwrap_or(buffer.len()))
        .ok_or_else(inside_model_text)?;
    let mut at = at;
    if at > 0 && !buffer.is_blank(at - 1) {
        buffer.insert_spacer(at);
        at += 1;
    }
    buffer.insert(at, format!("## {title}"));
    buffer.insert(at + 1, line);
    if at + 2 < buffer.len() && !buffer.is_blank(at + 2) {
        buffer.insert_spacer(at + 2);
    }
    Ok(())
}

/// The receipts of everything a buffer changed, in the order it was changed.
pub(crate) fn receipts_from(buffer: &LineBuffer, relative: &Path) -> Vec<WriteReceipt> {
    buffer
        .changes()
        .iter()
        .filter_map(|change| {
            Some(WriteReceipt {
                path: relative.to_path_buf(),
                line_number: change.index + 1,
                old_line: change.old_text.clone(),
                new_line: buffer.text(change.index)?.to_owned(),
            })
        })
        .collect()
}

/// The single line a write was meant to change.
fn single_receipt(
    mut receipts: Vec<WriteReceipt>,
    relative: &Path,
) -> Result<WriteReceipt, WriteError> {
    receipts
        .pop()
        .ok_or_else(|| WriteError::BatchFailed(relative.to_path_buf()))
}

fn apply_change(
    buffer: &mut LineBuffer,
    change: &PendingChange,
    relative: &Path,
) -> Result<WriteReceipt, WriteError> {
    let document = buffer.document();
    let task = find_task(&document, &change.id, relative)?;
    let old_line = document
        .lines()
        .get(task.line)
        .map(|line| line.text.clone())
        .ok_or_else(|| WriteError::TaskNotFound {
            id: change.id.clone(),
            path: relative.to_path_buf(),
        })?;
    if let Some(expected) = &change.expected_line
        && *expected != old_line
    {
        return Err(WriteError::TaskLineChanged {
            id: change.id.clone(),
            path: relative.to_path_buf(),
            found: old_line,
        });
    }
    let new_line = edited_line(&old_line, task, &change.edit)?;
    buffer.replace(task.line, new_line.clone());
    Ok(WriteReceipt {
        path: relative.to_path_buf(),
        line_number: task.line + 1,
        old_line: Some(old_line),
        new_line,
    })
}

fn check_expected_line(
    document: &Document,
    task: &Task,
    target: &TaskTarget,
    relative: &Path,
) -> Result<(), WriteError> {
    let found = document
        .lines()
        .get(task.line)
        .map(|line| line.text.as_str())
        .unwrap_or_default();
    match &target.expected_line {
        Some(expected) if expected != found => Err(WriteError::TaskLineChanged {
            id: target.id.clone(),
            path: relative.to_path_buf(),
            found: found.to_owned(),
        }),
        _ => Ok(()),
    }
}

pub(crate) fn find_task<'d>(
    document: &'d Document,
    id: &str,
    relative: &Path,
) -> Result<&'d Task, WriteError> {
    let mut matches = document.tasks().iter().filter(|task| task.id() == Some(id));
    let Some(first) = matches.next() else {
        return Err(WriteError::TaskNotFound {
            id: id.to_owned(),
            path: relative.to_path_buf(),
        });
    };
    let others: Vec<usize> = matches.map(|task| task.line + 1).collect();
    if others.is_empty() {
        return Ok(first);
    }
    let mut lines = vec![first.line + 1];
    lines.extend(others);
    Err(WriteError::DuplicateTaskId {
        id: id.to_owned(),
        path: relative.to_path_buf(),
        lines,
    })
}

/// The first task line carrying `[id:: id]`, if any.
pub(crate) fn find_task_by_id<'d>(document: &'d Document, id: &str) -> Option<&'d Task> {
    document.tasks().iter().find(|task| task.id() == Some(id))
}

fn ensure_id_unused(document: &Document, id: &str, relative: &Path) -> Result<(), WriteError> {
    if document.tasks().iter().any(|task| task.id() == Some(id)) {
        return Err(WriteError::IdAlreadyUsed {
            id: id.to_owned(),
            path: relative.to_path_buf(),
        });
    }
    Ok(())
}

/// Applies `edit` to one task line and returns the new line.
fn edited_line(line: &str, task: &Task, edit: &TaskEdit) -> Result<String, WriteError> {
    match edit {
        TaskEdit::Done(completion) => {
            let already_done =
                task.status == TaskStatus::Done && task.field(COMPLETION_KEY).is_some();
            if already_done {
                return Ok(line.to_owned());
            }
            let ticked = with_status(line, task, 'x')?;
            let without_completion = remove_fields(&ticked, COMPLETION_KEY);
            Ok(append_field(
                &without_completion,
                COMPLETION_KEY,
                &completion.to_string(),
            ))
        }
        TaskEdit::Reopen => {
            let reopened = with_status(line, task, ' ')?;
            Ok(remove_fields(&reopened, COMPLETION_KEY))
        }
        TaskEdit::Schedule(date) => Ok(with_optional_field(
            line,
            "scheduled",
            date.map(|date| date.to_string()),
        )),
        TaskEdit::Due(date) => Ok(with_optional_field(
            line,
            "due",
            date.map(|date| date.to_string()),
        )),
        TaskEdit::Estimate(estimate) => {
            let value = estimate
                .as_deref()
                .map(|estimate| validated_value("estimate", estimate))
                .transpose()?;
            Ok(with_optional_field(line, "estimate", value))
        }
        TaskEdit::Text(text) => with_text(line, task, text),
    }
}

fn with_status(line: &str, task: &Task, status: char) -> Result<String, WriteError> {
    let (start, end) = status_char_range(line, task.indent.len(), task.bullet.len())
        .ok_or_else(|| WriteError::InvalidInput(format!("not a task line: {line:?}")))?;
    Ok(format!("{}{status}{}", &line[..start], &line[end..]))
}

fn with_text(line: &str, task: &Task, text: &str) -> Result<String, WriteError> {
    let text = text.trim();
    if text.is_empty() || text.contains(['\n', '\r']) {
        return Err(WriteError::InvalidInput(
            "task text must be one non-empty line".to_owned(),
        ));
    }
    if !inline_fields(text).is_empty() {
        return Err(WriteError::InvalidInput(
            "task text must not contain inline fields such as [key:: value]".to_owned(),
        ));
    }
    let (_, status_end) = status_char_range(line, task.indent.len(), task.bullet.len())
        .ok_or_else(|| WriteError::InvalidInput(format!("not a task line: {line:?}")))?;
    let after_checkbox = &line[status_end + 1..];
    let body_start = status_end + 1 + (after_checkbox.len() - after_checkbox.trim_start().len());
    let region_end = inline_fields(line)
        .first()
        .map_or(line.len(), |field| field.start)
        .max(body_start);
    let old_text_len = line[body_start..region_end].trim_end().len();
    let tail = &line[body_start + old_text_len..];
    let separator = if old_text_len == 0 && !tail.is_empty() {
        " "
    } else {
        ""
    };
    Ok(format!("{}{text}{separator}{tail}", &line[..body_start]))
}

fn validated_value(key: &str, value: &str) -> Result<String, WriteError> {
    let value = value.trim();
    if value.is_empty() || value.contains(['\n', '\r', '[', ']']) {
        return Err(WriteError::InvalidInput(format!(
            "{key} must be a non-empty single-line value without brackets"
        )));
    }
    Ok(value.to_owned())
}

/// Sets `key` to `value`, replacing the first existing field in place or
/// appending one; `None` removes the field.
fn with_optional_field(line: &str, key: &str, value: Option<String>) -> String {
    match value {
        Some(value) => set_field(line, key, &value),
        None => remove_fields(line, key),
    }
}

fn set_field(line: &str, key: &str, value: &str) -> String {
    let existing = inline_fields(line)
        .into_iter()
        .find(|field| field.key == key);
    match existing {
        Some(field) => format!(
            "{}[{key}:: {value}]{}",
            &line[..field.start],
            &line[field.end..]
        ),
        None => append_field(line, key, value),
    }
}

fn append_field(line: &str, key: &str, value: &str) -> String {
    format!("{} [{key}:: {value}]", line.trim_end())
}

fn remove_fields(line: &str, key: &str) -> String {
    let mut result = line.to_owned();
    let fields: Vec<InlineField> = inline_fields(line)
        .into_iter()
        .filter(|field| field.key == key)
        .collect();
    for field in fields.iter().rev() {
        result = remove_span(&result, field.start, field.end);
    }
    result
}

/// Removes `start..end` with the whitespace that separated it from its
/// neighbours: the whitespace after it when another token follows, otherwise
/// the whitespace before it.
fn remove_span(line: &str, start: usize, end: usize) -> String {
    let after = &line[end..];
    let after_trimmed = after.trim_start();
    if !after_trimmed.is_empty() {
        let whitespace = after.len() - after_trimmed.len();
        return format!("{}{}", &line[..start], &line[end + whitespace..]);
    }
    format!("{}{}", line[..start].trim_end(), after)
}

/// The empty `- [ ] ` line the story template leaves under a heading.
fn empty_placeholder_line(document: &Document, section: TaskSection) -> Option<usize> {
    let found = document.section(section.heading())?;
    document
        .tasks_in(&found)
        .into_iter()
        .find(|candidate| {
            candidate.depth == 0 && candidate.text.is_empty() && candidate.fields.is_empty()
        })
        .map(|candidate| candidate.line)
}

/// The line [`VaultWriter::add_task`] writes for `task`, without indentation.
/// Callers fingerprint it for [`VaultWriter::add_model_task`].
///
/// # Errors
/// [`WriteError::InvalidInput`] for an unusable text, id or assignee.
pub fn task_line(task: &NewTask) -> Result<String, WriteError> {
    new_task_line("", task)
}

fn is_begin_marker(marker: &str) -> bool {
    marker.starts_with("%% ai:begin ") && marker.ends_with(" %%") && !marker.contains(['\n', '\r'])
}

fn new_task_line(indent: &str, task: &NewTask) -> Result<String, WriteError> {
    let text = task.text.trim();
    if text.is_empty() || text.contains(['\n', '\r']) {
        return Err(WriteError::InvalidInput(
            "task text must be one non-empty line".to_owned(),
        ));
    }
    if !inline_fields(text).is_empty() {
        return Err(WriteError::InvalidInput(
            "task text must not contain inline fields such as [key:: value]".to_owned(),
        ));
    }
    let id = task.id.trim();
    if id.is_empty() || id.contains(char::is_whitespace) || id.contains(['[', ']']) {
        return Err(WriteError::InvalidInput(
            "task id must be a non-empty token without spaces or brackets".to_owned(),
        ));
    }
    let mut line = format!(
        "{indent}- [ ] {text} [id:: {id}] [created:: {}]",
        task.created
    );
    if let Some(assigned_to) = &task.assigned_to {
        let assigned_to = validated_value("assigned_to", assigned_to)?;
        line.push_str(&format!(" [assigned_to:: {assigned_to}]"));
    }
    Ok(line)
}

/// The indent for a new child: the parent's first child's indent, else the
/// parent's indent plus the step the note already nests with, else one tab.
fn child_indent(document: &Document, parent: &Task) -> String {
    let parent_position = document
        .tasks()
        .iter()
        .position(|task| task.line == parent.line);
    parent_position
        .and_then(|position| {
            document
                .subtasks(position)
                .first()
                .map(|child| child.indent.clone())
        })
        .unwrap_or_else(|| format!("{}{}", parent.indent, nesting_step(document)))
}

/// The indent a child adds to its parent in this note (the first nested task
/// decides), or a tab when nothing is nested yet.
fn nesting_step(document: &Document) -> String {
    let tasks = document.tasks();
    tasks
        .iter()
        .find_map(|child| {
            let parent = tasks.get(child.parent?)?;
            let step = child.indent.strip_prefix(parent.indent.as_str())?;
            (!step.is_empty()
                && step
                    .chars()
                    .all(|character| matches!(character, ' ' | '\t')))
            .then(|| step.to_owned())
        })
        .unwrap_or_else(|| "\t".to_owned())
}

/// The line just past the parent and everything indented deeper below it.
pub(crate) fn subtree_end(buffer: &LineBuffer, parent: &Task) -> usize {
    let parent_columns = columns(&parent.indent);
    let mut end = parent.line + 1;
    while let Some(text) = buffer.text(end) {
        if text.trim().is_empty() || columns(leading_run(text)) <= parent_columns {
            break;
        }
        end += 1;
    }
    end
}

fn leading_run(text: &str) -> &str {
    let rest = text.trim_start_matches([' ', '\t', '>']);
    &text[..text.len() - rest.len()]
}

fn columns(indent: &str) -> usize {
    indent
        .chars()
        .map(|character| if character == '\t' { 4 } else { 1 })
        .sum()
}

fn same_slot(left: &TaskEdit, right: &TaskEdit) -> bool {
    matches!(
        (left, right),
        (
            TaskEdit::Done(_) | TaskEdit::Reopen,
            TaskEdit::Done(_) | TaskEdit::Reopen
        ) | (TaskEdit::Schedule(_), TaskEdit::Schedule(_))
            | (TaskEdit::Due(_), TaskEdit::Due(_))
            | (TaskEdit::Estimate(_), TaskEdit::Estimate(_))
            | (TaskEdit::Text(_), TaskEdit::Text(_))
    )
}

fn validate_story(story: &NewStory) -> Result<(), WriteError> {
    let number = story.number.trim();
    let is_token = !number.is_empty()
        && number
            .chars()
            .all(|character| character.is_alphanumeric() || matches!(character, '-' | '_'));
    if !is_token {
        return Err(WriteError::InvalidInput(
            "story number must be letters, digits, `-` or `_`".to_owned(),
        ));
    }
    let title = story.title.trim();
    if title.is_empty() || title.contains(['\n', '\r']) {
        return Err(WriteError::InvalidInput(
            "story title must be one non-empty line".to_owned(),
        ));
    }
    Ok(())
}

/// The title as a folder and file name: characters file systems or sync
/// clients reject become `-`, and leading dots are dropped.
fn file_name_safe(title: &str) -> Result<String, WriteError> {
    let replaced: String = title
        .trim()
        .chars()
        .map(|character| {
            if character.is_control() || "/\\:*?\"<>|".contains(character) {
                '-'
            } else {
                character
            }
        })
        .collect();
    let safe = replaced.trim_start_matches('.').trim().to_owned();
    if safe.is_empty() {
        return Err(WriteError::InvalidInput(
            "story title has no usable file name".to_owned(),
        ));
    }
    Ok(safe)
}

/// Fills the template and returns it with the 0-based line of the title.
fn fill_story_template(template: &str, story: &NewStory) -> (String, usize) {
    let number = story.number.trim();
    let title = story.title.trim();
    let has_title_placeholder = template.contains("{{title}}");
    let mut buffer = LineBuffer::parse(
        &template
            .replace("{{number}}", number)
            .replace("{{title}}", title),
    );
    let document = buffer.document();
    if document.body_start() > 0 {
        let frontmatter_end = document.body_start() - 1;
        let number_line = (1..frontmatter_end).find(|index| {
            buffer
                .text(*index)
                .is_some_and(|text| text.starts_with("number:"))
        });
        match number_line {
            Some(index) => buffer.replace(index, format!("number: {number}")),
            None => buffer.insert(frontmatter_end, format!("number: {number}")),
        }
    }
    let document = buffer.document();
    let first_heading = document
        .headings()
        .find(|(_, heading)| heading.level == 1)
        .map(|(index, _)| index);
    let title_line = match (first_heading, has_title_placeholder) {
        (Some(index), true) => index,
        (Some(index), false) => {
            buffer.replace(index, format!("# {title}"));
            index
        }
        (None, _) => {
            let at = document.body_start();
            buffer.insert(at, format!("# {title}"));
            if !buffer.is_blank(at + 1) && at + 1 < buffer.len() {
                buffer.insert_spacer(at + 1);
            }
            at
        }
    };
    (buffer.render(), title_line)
}

#[cfg(test)]
mod tests {
    use super::{TaskEdit, edited_line, file_name_safe, same_slot};
    use crate::date::Date;
    use crate::scan::Document;

    fn edit(line: &str, edit: &TaskEdit) -> String {
        let document = Document::parse(line);
        let task = document.tasks().first().unwrap().clone();
        edited_line(line, &task, edit).unwrap()
    }

    fn date(text: &str) -> Date {
        Date::parse(text).unwrap()
    }

    #[test]
    fn tick_and_untick_mirror_each_other() {
        let open = "- [ ] Ship it  [id:: task-1] [created:: 2026-10-01]";
        let done = edit(open, &TaskEdit::Done(date("2026-10-23")));
        assert_eq!(
            done,
            "- [x] Ship it  [id:: task-1] [created:: 2026-10-01] [completion:: 2026-10-23]"
        );
        assert_eq!(edit(&done, &TaskEdit::Reopen), open);
        assert_eq!(edit(&done, &TaskEdit::Done(date("2026-11-01"))), done);
    }

    #[test]
    fn tick_replaces_a_stale_completion_on_an_open_task() {
        let line = "- [ ] Odd [completion:: 2026-01-01] [id:: t]";
        assert_eq!(
            edit(line, &TaskEdit::Done(date("2026-10-23"))),
            "- [x] Odd [id:: t] [completion:: 2026-10-23]"
        );
    }

    #[test]
    fn fields_are_replaced_in_place_appended_or_removed() {
        let line = "\t- [/] Work  [id:: task-4] [created:: 2026-10-21] [estimate:: 45m]";
        assert_eq!(
            edit(line, &TaskEdit::Estimate(Some("2h".to_owned()))),
            "\t- [/] Work  [id:: task-4] [created:: 2026-10-21] [estimate:: 2h]"
        );
        assert_eq!(
            edit(line, &TaskEdit::Due(Some(date("2026-10-30")))),
            "\t- [/] Work  [id:: task-4] [created:: 2026-10-21] [estimate:: 45m] [due:: 2026-10-30]"
        );
        assert_eq!(
            edit(line, &TaskEdit::Estimate(None)),
            "\t- [/] Work  [id:: task-4] [created:: 2026-10-21]"
        );
        assert_eq!(
            edit("- [ ] A [due:: 2026-10-01] [id:: t]", &TaskEdit::Due(None)),
            "- [ ] A [id:: t]"
        );
        assert_eq!(edit(line, &TaskEdit::Schedule(None)), line);
    }

    #[test]
    fn text_changes_keep_the_separator_and_fields() {
        assert_eq!(
            edit(
                "- [ ] Old words  [id:: t] [due:: 2026-10-01]",
                &TaskEdit::Text("New words".to_owned())
            ),
            "- [ ] New words  [id:: t] [due:: 2026-10-01]"
        );
        assert_eq!(
            edit("- [ ] ", &TaskEdit::Text("Filled".to_owned())),
            "- [ ] Filled"
        );
        assert_eq!(
            edit("- [ ]  [id:: t]", &TaskEdit::Text("Filled".to_owned())),
            "- [ ]  Filled [id:: t]"
        );
        assert_eq!(
            edit("- [ ] Plain", &TaskEdit::Text("Other".to_owned())),
            "- [ ] Other"
        );
    }

    #[test]
    fn invalid_values_are_refused() {
        let line = "- [ ] A [id:: t]";
        let document = Document::parse(line);
        let task = document.tasks().first().unwrap();
        for bad in [
            TaskEdit::Text(String::new()),
            TaskEdit::Text("a\nb".to_owned()),
            TaskEdit::Text("see [due:: 2026-01-01]".to_owned()),
            TaskEdit::Estimate(Some("2]h".to_owned())),
            TaskEdit::Estimate(Some("  ".to_owned())),
        ] {
            assert!(edited_line(line, task, &bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn edits_of_one_kind_share_a_slot() {
        assert!(same_slot(
            &TaskEdit::Done(date("2026-10-23")),
            &TaskEdit::Reopen
        ));
        assert!(!same_slot(&TaskEdit::Due(None), &TaskEdit::Schedule(None)));
    }

    #[test]
    fn file_names_drop_unsafe_characters() {
        assert_eq!(
            file_name_safe("Pay: now/later?").unwrap(),
            "Pay- now-later-"
        );
        assert_eq!(file_name_safe("..hidden").unwrap(), "hidden");
        assert!(file_name_safe("...").is_err());
    }
}
