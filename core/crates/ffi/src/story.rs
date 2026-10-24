//! The story screen: the note rendered the way Obsidian's Reading view shows
//! it, next to its steps, waiting items and activity.
//!
//! The render model is [`vault_base::render`]'s block tree, flattened into
//! nodes that name their parent, because UniFFI records cannot nest their own
//! type. The Swift package builds the tree back. Nothing here writes: the
//! checkboxes carry task ids, and ticking one goes through [`crate::edit_task`].

use std::path::Path;

use vault_base::ops;
use vault_base::{
    AppView, AuthorshipKind, CalloutFold, ColumnAlignment, EmbedKind, FileAccess, LinkTarget,
    ListEntry, PlainFileAccess, RenderBlock, Span, SpanLink, Task, authorship, render_note,
};

use crate::browse::{
    ActivityEntryView, BrowseRowState, WaitingEntryView, activity_view, row_state, waiting_view,
};
use crate::home::count;
use crate::{CoreError, WorkspaceHandle, day_or_today};

/// How many log lines the story screen lists.
const STORY_ACTIVITY_LIMIT: usize = 50;

/// Where a span links to.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum SpanLinkView {
    /// A markdown link.
    Url {
        /// The address as written.
        url: String,
    },
    /// A wikilink to a note.
    Note {
        /// The note as written; empty for a heading of the same note.
        target: String,
        /// The heading after `#`.
        section: Option<String>,
        /// The note it resolves to, relative to the workspace root.
        resolved_path: Option<String>,
        /// The `obsidian://` link to it, at the section when one is named.
        obsidian_link: Option<String>,
    },
}

/// A run of text with one style.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SpanView {
    /// The text as shown.
    pub text: String,
    /// Bold.
    pub strong: bool,
    /// Italic.
    pub emphasis: bool,
    /// Struck through.
    pub strikethrough: bool,
    /// Highlighted.
    pub highlight: bool,
    /// Inline code.
    pub code: bool,
    /// The link it carries.
    pub link: Option<SpanLinkView>,
}

/// One table cell.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TableCellView {
    /// The cell's text.
    pub spans: Vec<SpanView>,
}

/// One table row.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TableRowView {
    /// The row's cells.
    pub cells: Vec<TableCellView>,
}

/// What an embed shows.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum EmbedKindView {
    /// An image file of the vault.
    Image,
    /// Another note.
    Note,
    /// A file a plugin draws.
    Plugin {
        /// `Excalidraw`, `Canvas`.
        name: String,
    },
    /// Any other file.
    File,
}

/// A view the app draws in place of a part of the note.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum AppViewKind {
    /// The Next Steps accordion.
    NextSteps,
    /// The Pending Input list.
    PendingInput,
    /// The activity list.
    Activity,
}

/// How a callout opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum CalloutFoldView {
    /// Not foldable.
    Fixed,
    /// Foldable, open at first.
    Expanded,
    /// Foldable, folded at first.
    Collapsed,
}

/// A table column's alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ColumnAlignmentView {
    /// No colon.
    Natural,
    /// `:---`.
    Leading,
    /// `:---:`.
    Center,
    /// `---:`.
    Trailing,
}

/// The checkbox of a task line in the body.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct CheckboxView {
    /// The state between the brackets.
    pub state: BrowseRowState,
    /// The id the writer changes the line by; `None` leaves it read-only.
    pub task_id: Option<String>,
}

/// What one node of the rendered note is.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum RenderNodeKind {
    /// A heading below the title.
    Heading {
        /// 1 to 6.
        level: u32,
        /// The heading as written.
        title: String,
        /// Its text as drawn.
        spans: Vec<SpanView>,
        /// Opens the note at this heading in Obsidian.
        obsidian_link: String,
    },
    /// A paragraph; `\n` marks a line break.
    Paragraph {
        /// The text.
        spans: Vec<SpanView>,
    },
    /// A list; its items are its children.
    List {
        /// Numbered.
        is_ordered: bool,
    },
    /// A list item; the items nested under it are its children.
    ListItem {
        /// The bullet as written.
        marker: String,
        /// The text without inline fields.
        spans: Vec<SpanView>,
        /// Set on a task line.
        checkbox: Option<CheckboxView>,
    },
    /// A blockquote; its blocks are its children.
    Quote,
    /// A callout; its blocks are its children.
    Callout {
        /// The kind, lower case.
        kind: String,
        /// The title.
        title: Vec<SpanView>,
        /// How it opens.
        fold: CalloutFoldView,
    },
    /// A fenced code block.
    Code {
        /// The info string's first word.
        language: Option<String>,
        /// The code.
        text: String,
    },
    /// A pipe table.
    Table {
        /// One per column.
        alignments: Vec<ColumnAlignmentView>,
        /// The header row.
        header: Vec<TableCellView>,
        /// The body rows.
        rows: Vec<TableRowView>,
    },
    /// An embed on a line of its own.
    Embed {
        /// What it shows.
        kind: EmbedKindView,
        /// The target as written.
        target: String,
        /// The heading after `#`.
        section: Option<String>,
        /// The vault file, relative to the workspace root, when it resolves.
        resolved_path: Option<String>,
        /// The same file as an absolute path, for an image to load.
        file_path: Option<String>,
        /// Opens the target in Obsidian, when it resolves.
        obsidian_link: Option<String>,
    },
    /// A query Obsidian runs and the app does not.
    Query {
        /// `dataview`, `dataviewjs`, `tasks`, `query`.
        language: String,
        /// The query as written.
        source: String,
        /// The app's own view that stands for it.
        view: Option<AppViewKind>,
    },
    /// A part of the note the app draws with its own view.
    AppView {
        /// The view.
        view: AppViewKind,
    },
    /// A thematic break.
    Rule,
}

/// One node of the rendered note, in document order; a child follows its parent.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct RenderNodeView {
    /// The index of the parent node in the list; `None` at the top level.
    pub parent: Option<u32>,
    /// The node's first line in the note, from 0.
    pub line: u32,
    /// What the node is.
    pub kind: RenderNodeKind,
}

/// One task line of Next Steps or Pending Input, in note order.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct StoryStepView {
    /// The `[id:: …]` field.
    pub task_id: Option<String>,
    /// The description without its fields.
    pub text: String,
    /// The checkbox state.
    pub state: BrowseRowState,
    /// 0 for a top-level step.
    pub depth: u32,
    /// `[scheduled:: …]` as written.
    pub scheduled: Option<String>,
    /// `[due:: …]` as written.
    pub due: Option<String>,
    /// `[estimate:: …]` in whole minutes.
    pub estimate_minutes: Option<u32>,
    /// `[completion:: …]` as written.
    pub completion: Option<String>,
    /// `[assigned_to:: …]` as written.
    pub assigned_to: Option<String>,
}

/// Whose words the note is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum AuthorshipKindView {
    /// No provenance unit.
    Human,
    /// Only model text.
    Model,
    /// Both.
    Mixed,
}

/// The provenance summary.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AuthorshipView {
    /// Human, model or mixed.
    pub kind: AuthorshipKindView,
    /// How many provenance units the note holds.
    pub model_units: u32,
    /// The latest date a unit's marker names, `YYYY-MM-DD`.
    pub latest_model_date: Option<String>,
}

/// Everything the story screen shows.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct StoryScreenView {
    /// The note, relative to the workspace root.
    pub relative_path: String,
    /// The ticket number.
    pub number: String,
    /// The note's first heading.
    pub title: Option<String>,
    /// `source_kind` from the frontmatter.
    pub source_kind: Option<String>,
    /// `source_host` from the frontmatter.
    pub source_host: Option<String>,
    /// The status as written.
    pub status: Option<String>,
    /// The priority as written.
    pub priority: Option<String>,
    /// The due date, `YYYY-MM-DD`.
    pub due: Option<String>,
    /// The estimate as written.
    pub estimate: Option<String>,
    /// The project hub above the note, by title.
    pub project: Option<String>,
    /// The provenance summary.
    pub authorship: AuthorshipView,
    /// The rendered body.
    pub nodes: Vec<RenderNodeView>,
    /// Next Steps in note order, done steps in place.
    pub next_steps: Vec<StoryStepView>,
    /// Pending Input in note order.
    pub pending_input: Vec<StoryStepView>,
    /// Open items that wait on someone, oldest first.
    pub waiting: Vec<WaitingEntryView>,
    /// Log lines about the story and its tasks, newest first.
    pub activity: Vec<ActivityEntryView>,
    /// The `obsidian://` link to the note.
    pub obsidian_link: String,
}

/// The story screen of the story numbered `number`. `today` (default: the
/// host's day) ages the waiting items.
///
/// # Errors
/// [`CoreError::NotFound`] without such a story, [`CoreError::InvalidDate`]
/// for a malformed day, [`CoreError::FileAccess`] for an unreadable vault.
#[uniffi::export(default(today = None))]
pub fn story_screen(
    workspace: &WorkspaceHandle,
    number: String,
    today: Option<String>,
) -> Result<StoryScreenView, CoreError> {
    let today = day_or_today(today.as_deref())?;
    let vault = ops::open_vault(&workspace.workspace, &PlainFileAccess)?;
    let not_found = || CoreError::NotFound(format!("no story numbered {number} in the workspace"));
    let entry = vault.story_by_number(&number).ok_or_else(not_found)?;
    let note = vault.note(entry.path).ok_or_else(not_found)?;
    let preview = vault
        .story_preview(&number, today, STORY_ACTIVITY_LIMIT)
        .ok_or_else(not_found)?;
    let story = entry.story;
    let mut blocks = render_note(&note.document);
    let root = workspace.workspace.root.clone();
    vault.resolve_links(&mut blocks, entry.path, &|path| {
        PlainFileAccess.stat(path).ok().flatten().is_some()
    });
    let context = NodeContext {
        workspace,
        note_path: entry.path,
        root: &root,
    };
    let mut nodes = Vec::new();
    for block in &blocks {
        push_block(&mut nodes, None, block, &context);
    }
    let summary = authorship(&note.document);
    Ok(StoryScreenView {
        relative_path: entry.path.to_string_lossy().into_owned(),
        number: number.clone(),
        title: story.title.clone(),
        source_kind: story.source_kind.clone(),
        source_host: story.source_host.clone(),
        status: preview.status,
        priority: story.priority.clone(),
        due: story.due.map(|date| date.to_string()),
        estimate: story.estimate.clone(),
        project: vault.project_of(entry.path),
        authorship: AuthorshipView {
            kind: match summary.kind() {
                AuthorshipKind::Human => AuthorshipKindView::Human,
                AuthorshipKind::Model => AuthorshipKindView::Model,
                AuthorshipKind::Mixed => AuthorshipKindView::Mixed,
            },
            model_units: count(summary.model_units),
            latest_model_date: summary.latest_model_date.map(|date| date.to_string()),
        },
        nodes,
        next_steps: story.next_steps.iter().map(step_view).collect(),
        pending_input: story.pending_input.iter().map(step_view).collect(),
        waiting: preview.waiting.into_iter().map(waiting_view).collect(),
        activity: preview.activity.into_iter().map(activity_view).collect(),
        obsidian_link: workspace.workspace.obsidian_link(entry.path),
    })
}

/// What a node needs besides its block: the links are built per workspace.
struct NodeContext<'a> {
    workspace: &'a WorkspaceHandle,
    note_path: &'a Path,
    root: &'a Path,
}

fn push_node(
    nodes: &mut Vec<RenderNodeView>,
    parent: Option<u32>,
    line: usize,
    kind: RenderNodeKind,
) -> u32 {
    nodes.push(RenderNodeView {
        parent,
        line: count(line),
        kind,
    });
    count(nodes.len() - 1)
}

fn push_block(
    nodes: &mut Vec<RenderNodeView>,
    parent: Option<u32>,
    block: &RenderBlock,
    context: &NodeContext<'_>,
) {
    let spans = |spans: &[Span]| spans_view(spans, context);
    match block {
        RenderBlock::Heading {
            line,
            level,
            title,
            spans: own,
        } => {
            let kind = RenderNodeKind::Heading {
                level: u32::from(*level),
                title: title.clone(),
                spans: spans(own),
                obsidian_link: context
                    .workspace
                    .workspace
                    .obsidian_link_at(context.note_path, title),
            };
            push_node(nodes, parent, *line, kind);
        }
        RenderBlock::Paragraph { line, spans: own } => {
            push_node(
                nodes,
                parent,
                *line,
                RenderNodeKind::Paragraph { spans: spans(own) },
            );
        }
        RenderBlock::List {
            line,
            is_ordered,
            items,
        } => {
            let list = push_node(
                nodes,
                parent,
                *line,
                RenderNodeKind::List {
                    is_ordered: *is_ordered,
                },
            );
            push_entries(nodes, list, items, context);
        }
        RenderBlock::Quote { line, children } => {
            let quote = push_node(nodes, parent, *line, RenderNodeKind::Quote);
            for child in children {
                push_block(nodes, Some(quote), child, context);
            }
        }
        RenderBlock::Callout {
            line,
            kind,
            title,
            fold,
            children,
        } => {
            let callout = push_node(
                nodes,
                parent,
                *line,
                RenderNodeKind::Callout {
                    kind: kind.clone(),
                    title: spans(title),
                    fold: match fold {
                        CalloutFold::Static => CalloutFoldView::Fixed,
                        CalloutFold::Open => CalloutFoldView::Expanded,
                        CalloutFold::Folded => CalloutFoldView::Collapsed,
                    },
                },
            );
            for child in children {
                push_block(nodes, Some(callout), child, context);
            }
        }
        RenderBlock::Code {
            line,
            language,
            text,
        } => {
            let kind = RenderNodeKind::Code {
                language: language.clone(),
                text: text.clone(),
            };
            push_node(nodes, parent, *line, kind);
        }
        RenderBlock::Table {
            line,
            alignments,
            header,
            rows,
        } => {
            let cells = |row: &[Vec<Span>]| -> Vec<TableCellView> {
                row.iter()
                    .map(|cell| TableCellView { spans: spans(cell) })
                    .collect()
            };
            let kind = RenderNodeKind::Table {
                alignments: alignments
                    .iter()
                    .map(|alignment| alignment_view(*alignment))
                    .collect(),
                header: cells(header),
                rows: rows
                    .iter()
                    .map(|row| TableRowView { cells: cells(row) })
                    .collect(),
            };
            push_node(nodes, parent, *line, kind);
        }
        RenderBlock::Embed { line, kind, target } => {
            let kind = RenderNodeKind::Embed {
                kind: match kind {
                    EmbedKind::Image => EmbedKindView::Image,
                    EmbedKind::Note => EmbedKindView::Note,
                    EmbedKind::Plugin(name) => EmbedKindView::Plugin { name: name.clone() },
                    EmbedKind::File => EmbedKindView::File,
                },
                target: target.target.clone(),
                section: target.section.clone(),
                resolved_path: resolved_text(target),
                file_path: target
                    .resolved
                    .as_ref()
                    .map(|path| context.root.join(path).to_string_lossy().into_owned()),
                obsidian_link: obsidian_link(target, context),
            };
            push_node(nodes, parent, *line, kind);
        }
        RenderBlock::Query {
            line,
            language,
            source,
            view,
        } => {
            let kind = RenderNodeKind::Query {
                language: language.clone(),
                source: source.clone(),
                view: view.map(app_view_kind),
            };
            push_node(nodes, parent, *line, kind);
        }
        RenderBlock::AppView { line, view } => {
            push_node(
                nodes,
                parent,
                *line,
                RenderNodeKind::AppView {
                    view: app_view_kind(*view),
                },
            );
        }
        RenderBlock::Rule { line } => {
            push_node(nodes, parent, *line, RenderNodeKind::Rule);
        }
    }
}

fn push_entries(
    nodes: &mut Vec<RenderNodeView>,
    parent: u32,
    entries: &[ListEntry],
    context: &NodeContext<'_>,
) {
    for entry in entries {
        let item = push_node(
            nodes,
            Some(parent),
            entry.line,
            RenderNodeKind::ListItem {
                marker: entry.marker.clone(),
                spans: spans_view(&entry.spans, context),
                checkbox: entry.checkbox.as_ref().map(|checkbox| CheckboxView {
                    state: row_state(checkbox.status),
                    task_id: checkbox.task_id.clone(),
                }),
            },
        );
        push_entries(nodes, item, &entry.children, context);
    }
}

fn spans_view(spans: &[Span], context: &NodeContext<'_>) -> Vec<SpanView> {
    spans
        .iter()
        .map(|span| SpanView {
            text: span.text.clone(),
            strong: span.style.strong,
            emphasis: span.style.emphasis,
            strikethrough: span.style.strikethrough,
            highlight: span.style.highlight,
            code: span.style.code,
            link: span.link.as_ref().map(|link| match link {
                SpanLink::Url(url) => SpanLinkView::Url { url: url.clone() },
                SpanLink::Note(target) => SpanLinkView::Note {
                    target: target.target.clone(),
                    section: target.section.clone(),
                    resolved_path: resolved_text(target),
                    obsidian_link: obsidian_link(target, context),
                },
            }),
        })
        .collect()
}

fn resolved_text(target: &LinkTarget) -> Option<String> {
    target
        .resolved
        .as_ref()
        .map(|path| path.to_string_lossy().into_owned())
}

/// The target's `obsidian://` link, at its section when it names one.
fn obsidian_link(target: &LinkTarget, context: &NodeContext<'_>) -> Option<String> {
    let path = target.resolved.as_deref()?;
    let workspace = &context.workspace.workspace;
    Some(match &target.section {
        Some(section) => workspace.obsidian_link_at(path, section),
        None => workspace.obsidian_link(path),
    })
}

fn app_view_kind(view: AppView) -> AppViewKind {
    match view {
        AppView::NextSteps => AppViewKind::NextSteps,
        AppView::PendingInput => AppViewKind::PendingInput,
        AppView::Activity => AppViewKind::Activity,
    }
}

fn alignment_view(alignment: ColumnAlignment) -> ColumnAlignmentView {
    match alignment {
        ColumnAlignment::Default => ColumnAlignmentView::Natural,
        ColumnAlignment::Left => ColumnAlignmentView::Leading,
        ColumnAlignment::Center => ColumnAlignmentView::Center,
        ColumnAlignment::Right => ColumnAlignmentView::Trailing,
    }
}

fn step_view(task: &Task) -> StoryStepView {
    StoryStepView {
        task_id: task.id().map(str::to_owned),
        text: task.text.clone(),
        state: row_state(task.status),
        depth: count(task.depth),
        scheduled: task.field("scheduled").map(str::to_owned),
        due: task.field("due").map(str::to_owned),
        estimate_minutes: task
            .field("estimate")
            .and_then(domain::quick_add::parse_estimate)
            .map(|estimate| crate::home::whole_minutes(estimate.as_duration())),
        completion: task.field("completion").map(str::to_owned),
        assigned_to: task.field("assigned_to").map(str::to_owned),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use super::*;
    use crate::{TaskEditInput, edit_task, open_workspace};

    const TODAY: &str = "2026-10-22";
    const STORY: &str = "Projects/Website/WEB-142 Checkout redesign/WEB-142 Checkout redesign.md";

    fn mock_vault() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/mock-vault")
    }

    fn screen(root: &Path) -> StoryScreenView {
        let workspace = open_workspace(root.to_string_lossy().into_owned(), Vec::new()).unwrap();
        story_screen(&workspace, "WEB-142".to_owned(), Some(TODAY.to_owned())).unwrap()
    }

    #[test]
    fn the_screen_holds_the_story_block_values_and_the_steps() {
        let view = screen(&mock_vault());
        assert_eq!(view.number, "WEB-142");
        assert_eq!(view.title.as_deref(), Some("Checkout redesign"));
        assert_eq!(view.source_host.as_deref(), Some("tracker.example.test"));
        assert_eq!(view.status.as_deref(), Some("in-progress"));
        assert_eq!(view.priority.as_deref(), Some("high"));
        assert_eq!(view.due.as_deref(), Some("2026-10-30"));
        assert_eq!(view.project.as_deref(), Some("Website"));
        assert_eq!(view.authorship.kind, AuthorshipKindView::Mixed);
        assert_eq!(
            view.authorship.latest_model_date.as_deref(),
            Some("2026-10-20")
        );
        let steps: Vec<(&str, u32)> = view
            .next_steps
            .iter()
            .map(|step| (step.task_id.as_deref().unwrap_or(""), step.depth))
            .collect();
        assert_eq!(
            steps,
            [("task-2", 0), ("task-3", 1), ("task-4", 1), ("task-5", 0)]
        );
        assert_eq!(view.next_steps[0].estimate_minutes, Some(120));
        assert_eq!(
            view.pending_input[1].completion.as_deref(),
            Some("2026-10-19")
        );
        assert!(!view.activity.is_empty());
    }

    #[test]
    fn nodes_name_their_parents_and_hide_markers() {
        let view = screen(&mock_vault());
        for (index, node) in view.nodes.iter().enumerate() {
            if let Some(parent) = node.parent {
                assert!((parent as usize) < index, "a parent comes first");
            }
        }
        let text = format!("{:?}", view.nodes);
        assert!(!text.contains("ai:end"), "{text}");
        let heading_link = view
            .nodes
            .iter()
            .find_map(|node| match &node.kind {
                RenderNodeKind::Heading {
                    title,
                    obsidian_link,
                    ..
                } if title == "Background" => Some(obsidian_link.clone()),
                _ => None,
            })
            .unwrap();
        assert!(heading_link.ends_with("%23Background"), "{heading_link}");
        let image = view
            .nodes
            .iter()
            .find_map(|node| match &node.kind {
                RenderNodeKind::Embed {
                    kind: EmbedKindView::Image,
                    file_path,
                    ..
                } => file_path.clone(),
                _ => None,
            })
            .unwrap();
        assert!(Path::new(&image).is_file(), "{image}");
    }

    #[test]
    fn ticking_a_body_checkbox_changes_that_one_line() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("vault");
        copy_dir(&mock_vault(), &root);
        let before = fs::read_to_string(root.join(STORY)).unwrap();
        let view = screen(&root);
        let task_id = view
            .nodes
            .iter()
            .find_map(|node| match &node.kind {
                RenderNodeKind::ListItem {
                    checkbox: Some(checkbox),
                    ..
                } => checkbox.task_id.clone(),
                _ => None,
            })
            .unwrap();
        assert_eq!(task_id, "task-1");

        let workspace = open_workspace(root.to_string_lossy().into_owned(), Vec::new()).unwrap();
        edit_task(&workspace, task_id, TaskEditInput::Reopen).unwrap();

        let after = fs::read_to_string(root.join(STORY)).unwrap();
        let changed: Vec<(&str, &str)> = before
            .lines()
            .zip(after.lines())
            .filter(|(old, new)| old != new)
            .collect();
        assert_eq!(before.lines().count(), after.lines().count());
        assert_eq!(changed.len(), 1, "{changed:?}");
        assert!(
            changed[0].1.starts_with("- [ ] Extract the address form"),
            "{changed:?}"
        );
    }

    fn copy_dir(from: &Path, to: &Path) {
        fs::create_dir_all(to).unwrap();
        for entry in fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let target = to.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_dir(&entry.path(), &target);
            } else {
                fs::copy(entry.path(), &target).unwrap();
            }
        }
    }
}
