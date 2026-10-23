//! Read-only previews: what the app shows beside a task or a story without
//! opening the note. The story note is cut into plain blocks (headings,
//! paragraphs, bullets, code) with the frontmatter, the title, the provenance
//! markers, the two task sections and the Activity query left out, because
//! the preview shows the tasks, the waiting items and the activity in their
//! own places.

use crate::date::{Date, Time};
use crate::note::{ACTIVITY_HEADING, LogKind, NEXT_STEPS_HEADING, PENDING_INPUT_HEADING};
use crate::scan::{Document, LineKind, Task, TaskStatus};
use crate::vault::{ME, Vault};

/// One block of a rendered note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoteBlock {
    /// A heading below the title.
    Heading {
        /// 1 to 6.
        level: u8,
        /// The heading text.
        text: String,
    },
    /// Consecutive text lines joined with single spaces. Inline markdown stays as written.
    Paragraph(String),
    /// A list item; `state` is set when it is a task line.
    Bullet {
        /// 0 for a top-level item.
        depth: usize,
        /// The text without inline fields.
        text: String,
        /// The checkbox state of a task line.
        state: Option<TaskStatus>,
    },
    /// The lines of a fenced code block, without the fences.
    Code(String),
}

/// One open task that waits on someone else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaitingEntry {
    /// The `[id:: …]` field.
    pub task_id: Option<String>,
    /// The description without its fields.
    pub text: String,
    /// Who it waits on.
    pub assigned_to: String,
    /// Days since `[created:: …]`.
    pub age_days: Option<i64>,
}

/// One day-log line about a story or one of its tasks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityEntry {
    /// The day of the log.
    pub date: Date,
    /// The time prefix.
    pub time: Time,
    /// The `[kind:: …]` field as written.
    pub kind: Option<String>,
    /// The text without inline fields.
    pub text: String,
    /// The `[task:: …]` field.
    pub task: Option<String>,
    /// The `[agent:: …]` field: the model that wrote the line.
    pub agent: Option<String>,
}

/// A direct subtask.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubtaskEntry {
    /// The `[id:: …]` field.
    pub id: Option<String>,
    /// The description without its fields.
    pub text: String,
    /// The checkbox state.
    pub status: TaskStatus,
    /// The `[estimate:: …]` field as written.
    pub estimate: Option<String>,
}

/// A story as the preview shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoryPreview {
    /// The note, relative to the vault's root.
    pub relative_path: String,
    /// The ticket number.
    pub number: Option<String>,
    /// The note's first heading.
    pub title: Option<String>,
    /// The status as written.
    pub status: Option<String>,
    /// The priority as written.
    pub priority: Option<String>,
    /// The due date as written, when it parses.
    pub due: Option<Date>,
    /// The estimate as written.
    pub estimate: Option<String>,
    /// The note's body, without the sections the preview shows elsewhere.
    pub blocks: Vec<NoteBlock>,
    /// Open tasks of the story waiting on someone else, oldest first.
    pub waiting: Vec<WaitingEntry>,
    /// The latest log lines about the story and its tasks, newest first.
    pub activity: Vec<ActivityEntry>,
}

/// A task's story and subtasks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskPreview {
    /// The story whose note holds the task; `None` for a task elsewhere.
    pub story: Option<StoryPreview>,
    /// Direct subtasks in note order.
    pub subtasks: Vec<SubtaskEntry>,
    /// The latest log lines about this task, newest first.
    pub task_activity: Vec<ActivityEntry>,
}

/// Cuts a note into blocks. Sections whose heading is in `hidden_sections`
/// (compared without case) are left out whole, subsections included, and so
/// are the frontmatter, the first level-1 heading and the provenance markers.
#[must_use]
pub fn note_blocks(document: &Document, hidden_sections: &[&str]) -> Vec<NoteBlock> {
    let hidden_ranges: Vec<(usize, usize)> = document
        .sections()
        .into_iter()
        .filter_map(|section| {
            let heading_line = section.heading_line?;
            hidden_sections
                .iter()
                .any(|hidden| hidden.eq_ignore_ascii_case(&section.title))
                .then_some((heading_line, section.end))
        })
        .collect();
    let mut blocks = Vec::new();
    let mut paragraph: Vec<&str> = Vec::new();
    let mut code: Option<Vec<&str>> = None;
    let mut has_skipped_title = false;
    for (index, line) in document
        .lines()
        .iter()
        .enumerate()
        .skip(document.body_start())
    {
        if hidden_ranges
            .iter()
            .any(|(start, end)| (*start..*end).contains(&index))
        {
            continue;
        }
        match &line.kind {
            LineKind::Fence => {
                flush_paragraph(&mut paragraph, &mut blocks);
                match code.take() {
                    Some(lines) => blocks.push(NoteBlock::Code(lines.join("\n"))),
                    None => code = Some(Vec::new()),
                }
            }
            LineKind::Fenced => {
                if let Some(lines) = code.as_mut() {
                    lines.push(&line.text);
                }
            }
            LineKind::FrontmatterDelimiter | LineKind::Frontmatter | LineKind::Marker(_) => {}
            LineKind::Heading(heading) => {
                if heading.level == 1 && !has_skipped_title {
                    has_skipped_title = true;
                    continue;
                }
                flush_paragraph(&mut paragraph, &mut blocks);
                blocks.push(NoteBlock::Heading {
                    level: heading.level,
                    text: heading.title.clone(),
                });
            }
            LineKind::Task(task_index) => {
                flush_paragraph(&mut paragraph, &mut blocks);
                if let Some(task) = document.tasks().get(*task_index) {
                    blocks.push(NoteBlock::Bullet {
                        depth: task.depth,
                        text: task.text.clone(),
                        state: Some(task.status),
                    });
                }
            }
            LineKind::ListItem(item) => {
                flush_paragraph(&mut paragraph, &mut blocks);
                blocks.push(NoteBlock::Bullet {
                    depth: indent_depth(&item.indent),
                    text: item.text.clone(),
                    state: None,
                });
            }
            LineKind::Blank => flush_paragraph(&mut paragraph, &mut blocks),
            LineKind::Text => paragraph.push(line.text.trim()),
        }
    }
    flush_paragraph(&mut paragraph, &mut blocks);
    if let Some(lines) = code {
        blocks.push(NoteBlock::Code(lines.join("\n")));
    }
    blocks
}

impl Vault {
    /// The preview of the story numbered `number`: its note as blocks, the
    /// tasks waiting on others and the latest `activity_limit` log lines
    /// about it or its tasks. `today` ages the waiting items.
    #[must_use]
    pub fn story_preview(
        &self,
        number: &str,
        today: Date,
        activity_limit: usize,
    ) -> Option<StoryPreview> {
        let entry = self.story_by_number(number)?;
        let note = self.note(entry.path)?;
        let story = entry.story;
        let task_ids: Vec<&str> = story
            .next_steps
            .iter()
            .chain(&story.pending_input)
            .filter_map(Task::id)
            .collect();
        let waiting = self
            .waiting_on(ME, today)
            .into_iter()
            .filter(|item| item.story_number == Some(number))
            .map(|item| WaitingEntry {
                task_id: item.task.id().map(str::to_owned),
                text: item.task.text.clone(),
                assigned_to: item.assigned_to.to_owned(),
                age_days: item.age_days,
            })
            .collect();
        let mut activity = self.activity_where(|line| {
            line.story() == Some(number) || line.task().is_some_and(|id| task_ids.contains(&id))
        });
        activity.truncate(activity_limit);
        Some(StoryPreview {
            relative_path: entry.path.to_string_lossy().into_owned(),
            number: story.number.clone(),
            title: story.title.clone(),
            status: story
                .status
                .map(|status| status.as_str().to_owned())
                .or_else(|| story.raw_status.clone()),
            priority: story.priority.clone(),
            due: story.due,
            estimate: story.estimate.clone(),
            blocks: note_blocks(
                &note.document,
                &[NEXT_STEPS_HEADING, PENDING_INPUT_HEADING, ACTIVITY_HEADING],
            ),
            waiting,
            activity,
        })
    }

    /// The preview of the task carrying `[id:: id]`: its story, its direct
    /// subtasks and its own latest `activity_limit` log lines.
    #[must_use]
    pub fn task_preview(
        &self,
        id: &str,
        today: Date,
        activity_limit: usize,
    ) -> Option<TaskPreview> {
        let found = self.task_by_id(id)?;
        let note = self.note(found.path)?;
        let story = note
            .story
            .as_ref()
            .and_then(|story| story.number.as_deref())
            .and_then(|number| self.story_preview(number, today, activity_limit));
        let position = note
            .document
            .tasks()
            .iter()
            .position(|candidate| candidate.line == found.task.line);
        let subtasks = position
            .map(|position| note.document.subtasks(position))
            .unwrap_or_default()
            .into_iter()
            .map(|subtask| SubtaskEntry {
                id: subtask.id().map(str::to_owned),
                text: subtask.text.clone(),
                status: subtask.status,
                estimate: subtask.field("estimate").map(str::to_owned),
            })
            .collect();
        let mut task_activity = self.activity_where(|line| line.task() == Some(id));
        task_activity.truncate(activity_limit);
        Some(TaskPreview {
            story,
            subtasks,
            task_activity,
        })
    }

    /// Every day-log line `keep` accepts, newest first.
    fn activity_where(&self, keep: impl Fn(&crate::note::LogLine) -> bool) -> Vec<ActivityEntry> {
        let mut entries: Vec<ActivityEntry> = self
            .notes()
            .filter_map(|note| note.day_log.as_ref())
            .flat_map(|log| {
                log.lines
                    .iter()
                    .filter(|line| keep(line))
                    .map(move |line| (log.date, line))
            })
            .map(|(date, line)| ActivityEntry {
                date,
                time: line.time,
                kind: line.kind.as_ref().map(LogKind::as_str).map(str::to_owned),
                text: line.text.clone(),
                task: line.task().map(str::to_owned),
                agent: line.field("agent").map(str::to_owned),
            })
            .collect();
        entries.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| b.time.cmp(&a.time)));
        entries
    }
}

fn flush_paragraph(paragraph: &mut Vec<&str>, blocks: &mut Vec<NoteBlock>) {
    if !paragraph.is_empty() {
        blocks.push(NoteBlock::Paragraph(paragraph.join(" ")));
        paragraph.clear();
    }
}

/// Nesting of a list item from its indent: a tab or two spaces per level; a
/// blockquote prefix is not nesting.
fn indent_depth(indent: &str) -> usize {
    let whitespace: String = indent
        .chars()
        .filter(|character| matches!(character, ' ' | '\t'))
        .collect();
    let tabs = whitespace.matches('\t').count();
    let spaces = whitespace.len() - tabs;
    tabs + spaces / 2
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOTE: &str = "---\ntype: story\nnumber: WEB-142\n---\n# Checkout redesign\n\nReplace the three-step checkout.\nTicket: [WEB-142](https://x.test).\n\n## Background\n```markdown\n- [ ] a task inside a fence\n%% ai:begin fable · 2026-10-01 · 000000000000 %%\n```\n\n## Implementation\n%% ai:begin fable · 2026-10-20 · 3f9a1c2b7e4d %%\nRender the forms side by side.\n- [x] Extract the address form  [id:: task-1]\n%% ai:end %%\n\n## Next Steps\n- [ ] Wire the payment form  [id:: task-2]\n\n### Later\nstill hidden\n\n## Pending Input\n- [ ] Final copy  [id:: task-6]\n\n## Notes\n- 2026-10-21 Decided to keep the coupon field.\n\t- nested point\n";

    fn blocks() -> Vec<NoteBlock> {
        note_blocks(
            &Document::parse(NOTE),
            &[NEXT_STEPS_HEADING, PENDING_INPUT_HEADING],
        )
    }

    #[test]
    fn the_preview_drops_frontmatter_title_markers_and_the_task_sections() {
        let blocks = blocks();
        let headings: Vec<&str> = blocks
            .iter()
            .filter_map(|block| match block {
                NoteBlock::Heading { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(headings, ["Background", "Implementation", "Notes"]);
        // A marker-looking line inside a fence is code, not a marker.
        let outside_code = format!(
            "{:?}",
            blocks
                .iter()
                .filter(|block| !matches!(block, NoteBlock::Code(_)))
                .collect::<Vec<_>>()
        );
        assert!(!outside_code.contains("ai:"), "{outside_code}");
        let text = format!("{blocks:?}");
        assert!(!text.contains("Wire the payment form"), "{text}");
        assert!(!text.contains("still hidden"), "{text}");
        assert!(!text.contains("Final copy"), "{text}");
        assert!(!text.contains("type: story"), "{text}");
    }

    #[test]
    fn lines_group_into_paragraphs_code_and_bullets() {
        let blocks = blocks();
        assert_eq!(
            blocks[0],
            NoteBlock::Paragraph(
                "Replace the three-step checkout. Ticket: [WEB-142](https://x.test).".to_owned()
            )
        );
        assert_eq!(
            blocks[2],
            NoteBlock::Code(
                "- [ ] a task inside a fence\n%% ai:begin fable · 2026-10-01 · 000000000000 %%"
                    .to_owned()
            )
        );
        assert_eq!(
            blocks[4],
            NoteBlock::Paragraph("Render the forms side by side.".to_owned())
        );
        assert_eq!(
            blocks[5],
            NoteBlock::Bullet {
                depth: 0,
                text: "Extract the address form".to_owned(),
                state: Some(TaskStatus::Done),
            }
        );
        assert_eq!(
            blocks[8],
            NoteBlock::Bullet {
                depth: 1,
                text: "nested point".to_owned(),
                state: None,
            }
        );
    }

    #[test]
    fn hidden_section_names_ignore_case() {
        let blocks = note_blocks(&Document::parse(NOTE), &["background", "NOTES"]);
        let text = format!("{blocks:?}");
        assert!(!text.contains("Background"), "{text}");
        assert!(!text.contains("coupon"), "{text}");
        assert!(text.contains("Implementation"), "{text}");
    }

    #[test]
    fn an_unclosed_fence_still_yields_its_code() {
        let blocks = note_blocks(&Document::parse("# T\n```\nlet x = 1;\n"), &[]);
        assert_eq!(blocks, [NoteBlock::Code("let x = 1;".to_owned())]);
    }
}
