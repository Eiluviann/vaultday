//! The story screen's render model: a note cut into a tree of blocks with
//! inline spans, the way Obsidian's Reading view shows it. It is built from
//! the line scanner's classification ([`Document`]) and never changes it.
//!
//! What the model leaves out or replaces:
//! - the frontmatter, the title (the first level-1 heading) and every
//!   provenance marker are never blocks; inline `%% … %%` comments and
//!   `%%`-fenced comment blocks are hidden as Obsidian hides them;
//! - the Next Steps and Pending Input sections keep their heading and become
//!   one [`RenderBlock::AppView`]: the app draws those tasks itself;
//! - a Dataview, Tasks or search fence becomes a [`RenderBlock::Query`],
//!   tied to the app's Activity list when it sits under an `Activity` heading;
//! - `![[…]]` embeds become [`RenderBlock::Embed`], classed as an image, a
//!   note, a plugin's file (Excalidraw, Canvas) or another file.
//!
//! The model is read-only: it carries line numbers and task ids so the app
//! can change a checkbox through the writer, never text.

use std::path::{Component, Path, PathBuf};

use crate::date::Date;
use crate::note::{ACTIVITY_HEADING, NEXT_STEPS_HEADING, PENDING_INPUT_HEADING};
use crate::scan::{Document, LineKind, MarkerKind, TaskStatus};
use crate::vault::Vault;

/// How a run of text is drawn.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SpanStyle {
    /// `**bold**` or `__bold__`.
    pub strong: bool,
    /// `*italic*` or `_italic_`.
    pub emphasis: bool,
    /// `~~struck~~`.
    pub strikethrough: bool,
    /// `==highlighted==`.
    pub highlight: bool,
    /// `` `code` ``.
    pub code: bool,
}

/// A wikilink or embed target: `[[target#section|label]]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkTarget {
    /// The note or file as written, without the section; empty for a link
    /// to a heading of the same note.
    pub target: String,
    /// The heading after `#`, when one is named.
    pub section: Option<String>,
    /// The vault file the target resolves to, relative to the root; filled
    /// by [`Vault::resolve_links`].
    pub resolved: Option<PathBuf>,
}

/// Where a span links to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpanLink {
    /// A markdown link `[text](url)`.
    Url(String),
    /// A wikilink to a note of the vault.
    Note(LinkTarget),
}

/// A run of text with one style.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    /// The text as shown.
    pub text: String,
    /// How it is drawn.
    pub style: SpanStyle,
    /// The link it carries.
    pub link: Option<SpanLink>,
}

/// What an embed shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmbedKind {
    /// An image file of the vault.
    Image,
    /// Another note, transcluded in Obsidian.
    Note,
    /// A file a plugin draws (`Excalidraw`, `Canvas`): a placeholder with a link.
    Plugin(String),
    /// Any other file (a PDF, audio): a placeholder with a link.
    File,
}

/// A view the app draws in place of a part of the note.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppView {
    /// The Next Steps section: the app's steps accordion.
    NextSteps,
    /// The Pending Input section: the app's waiting list.
    PendingInput,
    /// A query under an `Activity` heading: the app's activity list.
    Activity,
}

/// How a callout opens: `[!note]`, `[!note]+` or `[!note]-`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CalloutFold {
    /// Always open.
    Static,
    /// Foldable, open at first.
    Open,
    /// Foldable, folded at first.
    Folded,
}

/// A table column's alignment from its delimiter row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColumnAlignment {
    /// `---`.
    Default,
    /// `:---`.
    Left,
    /// `:---:`.
    Center,
    /// `---:`.
    Right,
}

/// The checkbox of a task line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkbox {
    /// The state between the brackets.
    pub status: TaskStatus,
    /// The `[id:: …]` the writer addresses the line by; a line without one
    /// is shown but cannot be ticked from the app.
    pub task_id: Option<String>,
}

/// One item of a list, with the items nested under it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListEntry {
    /// Line index in the document.
    pub line: usize,
    /// The bullet as written (`-`, `*`, `1.`).
    pub marker: String,
    /// The text without inline fields.
    pub spans: Vec<Span>,
    /// Set on a task line.
    pub checkbox: Option<Checkbox>,
    /// Items indented under this one.
    pub children: Vec<ListEntry>,
}

/// One block of the rendered note. `line` is the block's first line in the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderBlock {
    /// A heading below the title.
    Heading {
        /// Line index.
        line: usize,
        /// 1 to 6.
        level: u8,
        /// The heading as written, the anchor Obsidian opens at.
        title: String,
        /// The heading's text as drawn.
        spans: Vec<Span>,
    },
    /// Consecutive text lines; a line break inside is kept as `\n`.
    Paragraph {
        /// Line index.
        line: usize,
        /// The text.
        spans: Vec<Span>,
    },
    /// A bulleted or numbered list.
    List {
        /// Line index.
        line: usize,
        /// Whether the first item is numbered.
        is_ordered: bool,
        /// The top-level items.
        items: Vec<ListEntry>,
    },
    /// A `>` blockquote.
    Quote {
        /// Line index.
        line: usize,
        /// The quoted blocks.
        children: Vec<RenderBlock>,
    },
    /// An Obsidian callout `> [!kind] Title`.
    Callout {
        /// Line index.
        line: usize,
        /// The kind, lower case (`note`, `warning`).
        kind: String,
        /// The title as written; empty for none.
        title: Vec<Span>,
        /// How it opens.
        fold: CalloutFold,
        /// The blocks inside.
        children: Vec<RenderBlock>,
    },
    /// A fenced code block.
    Code {
        /// Line index.
        line: usize,
        /// The info string's first word.
        language: Option<String>,
        /// The lines between the fences.
        text: String,
    },
    /// A pipe table.
    Table {
        /// Line index.
        line: usize,
        /// One per column.
        alignments: Vec<ColumnAlignment>,
        /// The header cells.
        header: Vec<Vec<Span>>,
        /// The body rows' cells.
        rows: Vec<Vec<Vec<Span>>>,
    },
    /// An `![[…]]` embed on a line of its own.
    Embed {
        /// Line index.
        line: usize,
        /// What it shows.
        kind: EmbedKind,
        /// The file or note embedded.
        target: LinkTarget,
    },
    /// A Dataview, Tasks or search query, never run by the app.
    Query {
        /// Line index.
        line: usize,
        /// `dataview`, `dataviewjs`, `tasks` or `query`.
        language: String,
        /// The query as written.
        source: String,
        /// The app's own view that stands for it, when there is one.
        view: Option<AppView>,
    },
    /// A part of the note the app draws with its own view.
    AppView {
        /// Line index of the section's first line.
        line: usize,
        /// The view.
        view: AppView,
    },
    /// A thematic break.
    Rule {
        /// Line index.
        line: usize,
    },
}

/// Whose words the note's body is, from its provenance markers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuthorshipKind {
    /// No line lies inside a provenance unit.
    Human,
    /// Every non-blank body line lies inside one.
    Model,
    /// Some do.
    Mixed,
}

/// The provenance summary of a note.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Authorship {
    /// Non-blank body lines inside a provenance unit.
    pub model_lines: usize,
    /// Non-blank body lines outside every unit, markers left out.
    pub human_lines: usize,
    /// How many units the note holds.
    pub model_units: usize,
    /// The latest date a unit's marker names.
    pub latest_model_date: Option<Date>,
}

impl Authorship {
    /// Human, model or mixed.
    #[must_use]
    pub fn kind(&self) -> AuthorshipKind {
        match (self.model_lines, self.human_lines) {
            (0, _) => AuthorshipKind::Human,
            (_, 0) => AuthorshipKind::Model,
            _ => AuthorshipKind::Mixed,
        }
    }
}

/// The Dataview, Tasks and search fences: queries Obsidian runs, the app never does.
const QUERY_LANGUAGES: [&str; 4] = ["dataview", "dataviewjs", "tasks", "query"];
const IMAGE_EXTENSIONS: [&str; 12] = [
    "png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "avif", "heic", "heif", "tif", "tiff",
];
const COMMENT_FENCE: &str = "%%";

/// One line as the block builder reads it: its index in the document, its
/// text with the enclosing quote levels taken off, and its class.
#[derive(Debug, Clone)]
struct SourceLine {
    index: usize,
    text: String,
    kind: LineKind,
    /// The `[id:: …]` and checkbox state of a task line.
    task: Option<(TaskStatus, Option<String>, String)>,
}

/// One entry of the builder's input: a line, or a section the app draws.
#[derive(Debug, Clone)]
enum SourceItem {
    Line(SourceLine),
    AppView { line: usize, view: AppView },
}

/// Renders the body of `document` into blocks. Wikilinks and embeds come
/// back unresolved; [`Vault::resolve_links`] fills them.
#[must_use]
pub fn render_note(document: &Document) -> Vec<RenderBlock> {
    let items = top_level_items(document);
    let mut has_skipped_title = false;
    build_blocks(&items, None, &mut has_skipped_title)
}

/// The provenance summary of `document`.
#[must_use]
pub fn authorship(document: &Document) -> Authorship {
    let units = document.provenance_units();
    let mut summary = Authorship {
        model_lines: 0,
        human_lines: 0,
        model_units: units.len(),
        latest_model_date: None,
    };
    for (index, line) in document
        .lines()
        .iter()
        .enumerate()
        .skip(document.body_start())
    {
        if line.text.trim().is_empty() || matches!(line.kind, LineKind::Marker(_)) {
            continue;
        }
        let is_model = units
            .iter()
            .any(|unit| unit.body_start <= index && index < unit.body_end);
        if is_model {
            summary.model_lines += 1;
        } else {
            summary.human_lines += 1;
        }
    }
    summary.latest_model_date = document
        .markers()
        .filter(|(_, marker)| matches!(marker.kind, MarkerKind::Note | MarkerKind::Begin))
        .filter_map(|(_, marker)| {
            marker
                .attributes
                .split('·')
                .find_map(|part| Date::parse(part.trim()))
        })
        .max();
    summary
}

/// The text of `text` as spans: emphasis, code, highlights, strikes,
/// markdown links and wikilinks. Inline `%% … %%` comments are dropped.
#[must_use]
pub fn inline_spans(text: &str) -> Vec<Span> {
    InlineParser::new(text).parse()
}

impl Vault {
    /// Fills every wikilink and embed target in `blocks`, written in the note
    /// at `note_path`: a note by its path or file name (the note's own folder
    /// first), a file by `file_exists` beside the note, then from the root.
    pub fn resolve_links(
        &self,
        blocks: &mut [RenderBlock],
        note_path: &Path,
        file_exists: &dyn Fn(&Path) -> bool,
    ) {
        let resolve = |target: &mut LinkTarget, is_file: bool| {
            target.resolved = if target.target.is_empty() {
                Some(note_path.to_path_buf())
            } else if is_file {
                self.resolve_file(&target.target, note_path, file_exists)
            } else {
                self.resolve_note(&target.target, note_path)
                    .or_else(|| self.resolve_file(&target.target, note_path, file_exists))
            };
        };
        for block in blocks {
            resolve_block(block, &resolve);
        }
    }

    /// The project hub above `note_path`: the nearest folder up the path
    /// that holds a project note. Its title, else its file name.
    #[must_use]
    pub fn project_of(&self, note_path: &Path) -> Option<String> {
        let projects = self.projects();
        note_path.ancestors().skip(1).find_map(|folder| {
            projects
                .iter()
                .find(|(path, _)| path.parent() == Some(folder) && *path != note_path)
                .map(|(path, _)| {
                    self.note(path)
                        .and_then(|note| note.document.title().map(str::to_owned))
                        .or_else(|| {
                            path.file_stem()
                                .map(|stem| stem.to_string_lossy().into_owned())
                        })
                        .unwrap_or_default()
                })
        })
    }

    fn resolve_note(&self, target: &str, note_path: &Path) -> Option<PathBuf> {
        let wanted = target.trim_end_matches(".md");
        if wanted.contains('/') {
            let path = PathBuf::from(format!("{wanted}.md"));
            return self.note(&path).map(|_| path);
        }
        let folder = note_path.parent();
        let mut matches: Vec<&Path> = self
            .notes()
            .map(|note| note.relative_path.as_path())
            .filter(|path| {
                path.file_stem()
                    .is_some_and(|stem| stem.to_string_lossy().eq_ignore_ascii_case(wanted))
            })
            .collect();
        matches.sort_by_key(|path| (path.parent() != folder, path.components().count()));
        matches.first().map(|path| path.to_path_buf())
    }

    fn resolve_file(
        &self,
        target: &str,
        note_path: &Path,
        file_exists: &dyn Fn(&Path) -> bool,
    ) -> Option<PathBuf> {
        let beside = note_path
            .parent()
            .map_or_else(|| PathBuf::from(target), |folder| folder.join(target));
        [beside, PathBuf::from(target)]
            .into_iter()
            .filter_map(|candidate| inside_vault(&candidate))
            .find(|candidate| file_exists(&self.root().join(candidate)))
    }
}

/// `path` with `.` and `..` folded away, when it stays inside the vault: a
/// relative path that never climbs above the root. An absolute path or one
/// that climbs out is `None`, so an embed never reads a file outside the vault.
fn inside_vault(path: &Path) -> Option<PathBuf> {
    let mut kept = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => kept.push(part),
            Component::CurDir => {}
            Component::ParentDir => {
                if !kept.pop() {
                    return None;
                }
            }
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    (!kept.as_os_str().is_empty()).then_some(kept)
}

fn resolve_block(block: &mut RenderBlock, resolve: &dyn Fn(&mut LinkTarget, bool)) {
    match block {
        RenderBlock::Heading { spans, .. } | RenderBlock::Paragraph { spans, .. } => {
            resolve_spans(spans, resolve);
        }
        RenderBlock::List { items, .. } => resolve_entries(items, resolve),
        RenderBlock::Quote { children, .. } => {
            for child in children {
                resolve_block(child, resolve);
            }
        }
        RenderBlock::Callout {
            title, children, ..
        } => {
            resolve_spans(title, resolve);
            for child in children {
                resolve_block(child, resolve);
            }
        }
        RenderBlock::Table { header, rows, .. } => {
            for cell in header.iter_mut().chain(rows.iter_mut().flatten()) {
                resolve_spans(cell, resolve);
            }
        }
        RenderBlock::Embed { kind, target, .. } => {
            let is_file = matches!(kind, EmbedKind::Image | EmbedKind::File)
                || matches!(kind, EmbedKind::Plugin(_)) && !target.target.ends_with(".md");
            resolve(target, is_file);
        }
        RenderBlock::Code { .. }
        | RenderBlock::Query { .. }
        | RenderBlock::AppView { .. }
        | RenderBlock::Rule { .. } => {}
    }
}

fn resolve_entries(entries: &mut [ListEntry], resolve: &dyn Fn(&mut LinkTarget, bool)) {
    for entry in entries {
        resolve_spans(&mut entry.spans, resolve);
        resolve_entries(&mut entry.children, resolve);
    }
}

fn resolve_spans(spans: &mut [Span], resolve: &dyn Fn(&mut LinkTarget, bool)) {
    for span in spans {
        if let Some(SpanLink::Note(target)) = &mut span.link {
            resolve(target, false);
        }
    }
}

/// The document's body lines, with the two task sections each replaced by
/// their heading and one app view, and the frontmatter left out.
fn top_level_items(document: &Document) -> Vec<SourceItem> {
    let app_sections: Vec<(usize, usize, AppView)> = document
        .sections()
        .into_iter()
        .filter_map(|section| {
            let heading_line = section.heading_line?;
            let view = if section.title.eq_ignore_ascii_case(NEXT_STEPS_HEADING) {
                AppView::NextSteps
            } else if section.title.eq_ignore_ascii_case(PENDING_INPUT_HEADING) {
                AppView::PendingInput
            } else {
                return None;
            };
            Some((heading_line, section.end, view))
        })
        .collect();
    let mut items = Vec::new();
    let mut skip_until = 0;
    for (index, line) in document
        .lines()
        .iter()
        .enumerate()
        .skip(document.body_start())
    {
        if index < skip_until {
            continue;
        }
        let task = match &line.kind {
            LineKind::Task(task_index) => document
                .tasks()
                .get(*task_index)
                .map(|task| (task.status, task.id().map(str::to_owned), task.text.clone())),
            _ => None,
        };
        items.push(SourceItem::Line(SourceLine {
            index,
            text: line.text.clone(),
            kind: line.kind.clone(),
            task,
        }));
        if let Some((_, end, view)) = app_sections.iter().find(|(start, ..)| *start == index) {
            items.push(SourceItem::AppView {
                line: index + 1,
                view: *view,
            });
            skip_until = *end;
        }
    }
    items
}

/// Builds blocks from `items`. `section` is the heading the items sit under,
/// for a query's app view; `has_skipped_title` drops the first level-1 heading once.
fn build_blocks(
    items: &[SourceItem],
    section: Option<&str>,
    has_skipped_title: &mut bool,
) -> Vec<RenderBlock> {
    let mut blocks = Vec::new();
    let mut current_section = section.map(str::to_owned);
    let mut position = 0;
    while let Some(item) = items.get(position) {
        let line = match item {
            SourceItem::AppView { line, view } => {
                blocks.push(RenderBlock::AppView {
                    line: *line,
                    view: *view,
                });
                position += 1;
                continue;
            }
            SourceItem::Line(line) => line,
        };
        if is_quoted(line) {
            let end = run_end(items, position, is_quoted);
            blocks.push(quote_block(
                &items[position..end],
                current_section.as_deref(),
            ));
            position = end;
            continue;
        }
        match &line.kind {
            LineKind::Fence => {
                let end = fence_end(items, position);
                blocks.push(fence_block(
                    &items[position..end],
                    current_section.as_deref(),
                ));
                position = end;
            }
            LineKind::Heading(heading) => {
                position += 1;
                if heading.level == 1 && !*has_skipped_title {
                    *has_skipped_title = true;
                    continue;
                }
                current_section = Some(heading.title.clone());
                blocks.push(RenderBlock::Heading {
                    line: line.index,
                    level: heading.level,
                    title: heading.title.clone(),
                    spans: inline_spans(&heading.title),
                });
            }
            LineKind::Task(_) | LineKind::ListItem(_) => {
                let end = run_end(items, position, |line| {
                    matches!(line.kind, LineKind::Task(_) | LineKind::ListItem(_))
                });
                blocks.push(list_block(&items[position..end]));
                position = end;
            }
            LineKind::Text if line.text.trim() == COMMENT_FENCE => {
                // A `%%` line opens a comment block Obsidian hides up to the next one.
                let closer = items[position + 1..]
                    .iter()
                    .position(|item| {
                        matches!(item, SourceItem::Line(line) if line.text.trim() == COMMENT_FENCE)
                    })
                    .map_or(items.len(), |offset| position + 1 + offset + 1);
                position = closer;
            }
            LineKind::Text => {
                let (block, end) = text_block(items, position);
                if let Some(block) = block {
                    blocks.push(block);
                }
                position = end;
            }
            LineKind::Blank
            | LineKind::Marker(_)
            | LineKind::Frontmatter
            | LineKind::FrontmatterDelimiter
            | LineKind::Fenced => position += 1,
        }
    }
    blocks
}

fn is_quoted(line: &SourceLine) -> bool {
    !matches!(line.kind, LineKind::Fenced | LineKind::Marker(_))
        && line.text.trim_start().starts_with('>')
}

/// One past the last item from `start` whose line satisfies `keep`.
fn run_end(items: &[SourceItem], start: usize, keep: impl Fn(&SourceLine) -> bool) -> usize {
    items[start..]
        .iter()
        .position(|item| match item {
            SourceItem::Line(line) => !keep(line),
            SourceItem::AppView { .. } => true,
        })
        .map_or(items.len(), |offset| start + offset)
}

/// One past the closing fence of the fence opening at `start`; the end of
/// the items for an unclosed one.
fn fence_end(items: &[SourceItem], start: usize) -> usize {
    items[start + 1..]
        .iter()
        .position(|item| matches!(item, SourceItem::Line(line) if line.kind == LineKind::Fence))
        .map_or(items.len(), |offset| start + 1 + offset + 1)
}

fn fence_block(items: &[SourceItem], section: Option<&str>) -> RenderBlock {
    let lines: Vec<&SourceLine> = items
        .iter()
        .filter_map(|item| match item {
            SourceItem::Line(line) => Some(line),
            SourceItem::AppView { .. } => None,
        })
        .collect();
    let opener = lines.first().map(|line| line.text.as_str()).unwrap_or("");
    let start = lines.first().map_or(0, |line| line.index);
    let language = opener
        .trim_start()
        .trim_start_matches(['`', '~'])
        .split_whitespace()
        .next()
        .map(str::to_ascii_lowercase);
    let body: Vec<&str> = lines
        .iter()
        .skip(1)
        .filter(|line| line.kind == LineKind::Fenced)
        .map(|line| line.text.as_str())
        .collect();
    let source = body.join("\n");
    match language {
        Some(language) if QUERY_LANGUAGES.contains(&language.as_str()) => {
            let view = section
                .filter(|title| title.eq_ignore_ascii_case(ACTIVITY_HEADING))
                .map(|_| AppView::Activity);
            RenderBlock::Query {
                line: start,
                language,
                source,
                view,
            }
        }
        language => RenderBlock::Code {
            line: start,
            language,
            text: source,
        },
    }
}

/// A run of `>` lines: one quote level off, scanned again, built into a
/// quote or, when the first line is `[!kind]`, a callout.
fn quote_block(items: &[SourceItem], section: Option<&str>) -> RenderBlock {
    let lines: Vec<&SourceLine> = items
        .iter()
        .filter_map(|item| match item {
            SourceItem::Line(line) => Some(line),
            SourceItem::AppView { .. } => None,
        })
        .collect();
    let start = lines.first().map_or(0, |line| line.index);
    let inner: Vec<(usize, String)> = lines
        .iter()
        .map(|line| (line.index, strip_one_quote_level(&line.text).to_owned()))
        .collect();
    let callout = inner
        .first()
        .and_then(|(_, first)| parse_callout_head(first));
    let body = if callout.is_some() {
        inner.get(1..).unwrap_or_default()
    } else {
        inner.as_slice()
    };
    let children = build_blocks(&rescan(body), section, &mut true);
    match callout {
        Some((kind, fold, title)) => RenderBlock::Callout {
            line: start,
            kind,
            title: inline_spans(&title),
            fold,
            children,
        },
        None => RenderBlock::Quote {
            line: start,
            children,
        },
    }
}

/// Scans quoted lines again with the line scanner, so lists, tasks, fences
/// and markers inside a quote are classified the way the note's own are.
fn rescan(lines: &[(usize, String)]) -> Vec<SourceItem> {
    // The leading blank line keeps a quoted `---` from reading as frontmatter.
    let source: String = std::iter::once(String::new())
        .chain(lines.iter().map(|(_, text)| text.clone()))
        .collect::<Vec<_>>()
        .join("\n");
    let document = Document::parse(&source);
    document
        .lines()
        .iter()
        .enumerate()
        .skip(1)
        .filter_map(|(inner_index, line)| {
            let (index, _) = lines.get(inner_index - 1)?;
            let task = match &line.kind {
                LineKind::Task(task_index) => document
                    .tasks()
                    .get(*task_index)
                    .map(|task| (task.status, task.id().map(str::to_owned), task.text.clone())),
                _ => None,
            };
            Some(SourceItem::Line(SourceLine {
                index: *index,
                text: line.text.clone(),
                kind: line.kind.clone(),
                task,
            }))
        })
        .collect()
}

fn strip_one_quote_level(text: &str) -> &str {
    let rest = text.trim_start().strip_prefix('>').unwrap_or(text);
    rest.strip_prefix(' ').unwrap_or(rest)
}

/// `[!kind]`, `[!kind]+` or `[!kind]-`, then the title.
fn parse_callout_head(text: &str) -> Option<(String, CalloutFold, String)> {
    let rest = text.trim_start().strip_prefix("[!")?;
    let close = rest.find(']')?;
    let kind = rest[..close].trim().to_ascii_lowercase();
    if kind.is_empty() {
        return None;
    }
    let after = &rest[close + 1..];
    let (fold, title) = if let Some(title) = after.strip_prefix('+') {
        (CalloutFold::Open, title)
    } else if let Some(title) = after.strip_prefix('-') {
        (CalloutFold::Folded, title)
    } else {
        (CalloutFold::Static, after)
    };
    Some((kind, fold, title.trim().to_owned()))
}

/// A list from a run of list and task lines, nested by indent.
fn list_block(items: &[SourceItem]) -> RenderBlock {
    let mut entries: Vec<ListEntry> = Vec::new();
    let mut base_depth: Option<usize> = None;
    let mut is_ordered = false;
    let mut start = 0;
    for item in items {
        let SourceItem::Line(line) = item else {
            continue;
        };
        let (indent, marker, text, checkbox) = match (&line.kind, &line.task) {
            (LineKind::Task(_), Some((status, task_id, text))) => {
                let (indent, marker) = indent_and_marker(&line.text);
                (
                    indent,
                    marker,
                    text.clone(),
                    Some(Checkbox {
                        status: *status,
                        task_id: task_id.clone(),
                    }),
                )
            }
            (LineKind::ListItem(item), _) => (
                item.indent.clone(),
                item.bullet.clone(),
                item.text.clone(),
                None,
            ),
            _ => continue,
        };
        let depth = indent_depth(&indent);
        let base = *base_depth.get_or_insert_with(|| {
            is_ordered = marker.ends_with(['.', ')']);
            start = line.index;
            depth
        });
        let entry = ListEntry {
            line: line.index,
            marker,
            spans: inline_spans(&text),
            checkbox,
            children: Vec::new(),
        };
        insert_entry(&mut entries, depth.saturating_sub(base), entry);
    }
    RenderBlock::List {
        line: start,
        is_ordered,
        items: entries,
    }
}

/// Puts `entry` `depth` levels down the last branch of `entries`.
fn insert_entry(entries: &mut Vec<ListEntry>, depth: usize, entry: ListEntry) {
    match entries.last_mut() {
        Some(last) if depth > 0 => insert_entry(&mut last.children, depth - 1, entry),
        _ => entries.push(entry),
    }
}

/// The indent and bullet of a task line, which the scanner keeps on the task.
fn indent_and_marker(text: &str) -> (String, String) {
    let indent_length = text.len() - text.trim_start_matches([' ', '\t']).len();
    let rest = &text[indent_length..];
    let marker_length = rest.find(char::is_whitespace).unwrap_or(rest.len());
    (
        text[..indent_length].to_owned(),
        rest[..marker_length].to_owned(),
    )
}

/// Nesting from an indent: a tab or two spaces per level.
fn indent_depth(indent: &str) -> usize {
    let tabs = indent.matches('\t').count();
    let spaces = indent.matches(' ').count();
    tabs + spaces / 2
}

/// A text line and what follows it: a table, an embed, a rule or a paragraph.
fn text_block(items: &[SourceItem], start: usize) -> (Option<RenderBlock>, usize) {
    let Some(SourceItem::Line(line)) = items.get(start) else {
        return (None, start + 1);
    };
    let trimmed = line.text.trim();
    if let Some(alignments) = table_delimiter(items.get(start + 1))
        && trimmed.starts_with('|')
    {
        let end = run_end(items, start + 2, |line| {
            line.kind == LineKind::Text && line.text.trim().starts_with('|')
        });
        let rows = items[start + 2..end]
            .iter()
            .filter_map(|item| match item {
                SourceItem::Line(line) => Some(table_cells(&line.text)),
                SourceItem::AppView { .. } => None,
            })
            .collect();
        let block = RenderBlock::Table {
            line: line.index,
            alignments,
            header: table_cells(&line.text),
            rows,
        };
        return (Some(block), end);
    }
    if let Some(target) = whole_line_embed(trimmed) {
        let block = RenderBlock::Embed {
            line: line.index,
            kind: embed_kind(&target.target),
            target,
        };
        return (Some(block), start + 1);
    }
    if is_rule(trimmed) {
        return (Some(RenderBlock::Rule { line: line.index }), start + 1);
    }
    let end = run_end(items, start, |line| {
        line.kind == LineKind::Text
            && line.text.trim() != COMMENT_FENCE
            && whole_line_embed(line.text.trim()).is_none()
            && !is_rule(line.text.trim())
    })
    .max(start + 1);
    let text: Vec<&str> = items[start..end]
        .iter()
        .filter_map(|item| match item {
            SourceItem::Line(line) => Some(line.text.trim()),
            SourceItem::AppView { .. } => None,
        })
        .collect();
    let spans = inline_spans(&text.join("\n"));
    let block = (!spans.is_empty()).then_some(RenderBlock::Paragraph {
        line: line.index,
        spans,
    });
    (block, end)
}

/// The alignments of a `| --- | :-: |` delimiter row.
fn table_delimiter(item: Option<&SourceItem>) -> Option<Vec<ColumnAlignment>> {
    let Some(SourceItem::Line(line)) = item else {
        return None;
    };
    let trimmed = line.text.trim();
    if line.kind != LineKind::Text || !trimmed.contains('-') || !trimmed.contains('|') {
        return None;
    }
    split_cells(trimmed)
        .into_iter()
        .map(|cell| {
            let cell = cell.trim();
            let dashes = cell.trim_matches(':');
            if dashes.is_empty() || !dashes.chars().all(|c| c == '-') {
                return None;
            }
            Some(match (cell.starts_with(':'), cell.ends_with(':')) {
                (true, true) => ColumnAlignment::Center,
                (true, false) => ColumnAlignment::Left,
                (false, true) => ColumnAlignment::Right,
                (false, false) => ColumnAlignment::Default,
            })
        })
        .collect()
}

fn table_cells(text: &str) -> Vec<Vec<Span>> {
    split_cells(text.trim())
        .into_iter()
        .map(|cell| inline_spans(cell.trim()))
        .collect()
}

/// The cells of a pipe row: outer pipes dropped, `\|` kept inside a cell.
fn split_cells(row: &str) -> Vec<String> {
    let inner = row.strip_prefix('|').unwrap_or(row);
    let inner = inner.strip_suffix('|').unwrap_or(inner);
    let mut cells = vec![String::new()];
    let mut characters = inner.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '\\' if characters.peek() == Some(&'|') => {
                if let Some(cell) = cells.last_mut() {
                    cell.push('|');
                }
                characters.next();
            }
            '|' => cells.push(String::new()),
            other => {
                if let Some(cell) = cells.last_mut() {
                    cell.push(other);
                }
            }
        }
    }
    cells
}

/// `![[target#section|label]]` as the whole line.
fn whole_line_embed(trimmed: &str) -> Option<LinkTarget> {
    let inner = trimmed.strip_prefix("![[")?.strip_suffix("]]")?;
    if inner.contains("]]") {
        return None;
    }
    Some(link_target(inner).0)
}

fn is_rule(trimmed: &str) -> bool {
    let Some(marker) = trimmed
        .chars()
        .next()
        .filter(|c| matches!(c, '-' | '*' | '_'))
    else {
        return false;
    };
    trimmed.chars().all(|c| c == marker || c == ' ')
        && trimmed.chars().filter(|c| *c == marker).count() >= 3
}

/// What an embed of `target` shows, from its extension.
fn embed_kind(target: &str) -> EmbedKind {
    let lower = target.to_ascii_lowercase();
    if lower.ends_with(".excalidraw") || lower.ends_with(".excalidraw.md") {
        return EmbedKind::Plugin("Excalidraw".to_owned());
    }
    let extension = Path::new(&lower)
        .extension()
        .map(|extension| extension.to_string_lossy().into_owned());
    match extension.as_deref() {
        None | Some("md") => EmbedKind::Note,
        Some("canvas") => EmbedKind::Plugin("Canvas".to_owned()),
        Some(extension) if IMAGE_EXTENSIONS.contains(&extension) => EmbedKind::Image,
        Some(_) => EmbedKind::File,
    }
}

/// `target#section|label` → the target and the label, when there is one.
fn link_target(inner: &str) -> (LinkTarget, Option<String>) {
    let (path, label) = match inner.split_once('|') {
        Some((path, label)) => (path, Some(label.trim().to_owned())),
        None => (inner, None),
    };
    let (target, section) = match path.split_once('#') {
        Some((target, section)) => (target.trim(), Some(section.trim().to_owned())),
        None => (path.trim(), None),
    };
    (
        LinkTarget {
            target: target.to_owned(),
            section: section.filter(|section| !section.is_empty()),
            resolved: None,
        },
        label.filter(|label| !label.is_empty()),
    )
}

/// A small inline parser: enough of CommonMark and Obsidian's extensions
/// for a note's prose. Unmatched delimiters stay as written.
struct InlineParser {
    characters: Vec<char>,
    position: usize,
    style: SpanStyle,
    buffer: String,
    spans: Vec<Span>,
}

impl InlineParser {
    fn new(text: &str) -> InlineParser {
        InlineParser {
            characters: text.chars().collect(),
            position: 0,
            style: SpanStyle::default(),
            buffer: String::new(),
            spans: Vec::new(),
        }
    }

    fn parse(mut self) -> Vec<Span> {
        while let Some(&character) = self.characters.get(self.position) {
            let is_handled = match character {
                '\\' => self.escape(),
                '`' => self.code(),
                '%' => self.comment(),
                '!' => self.embed_or_image(),
                '[' => self.wikilink() || self.markdown_link(),
                '*' | '_' => self.emphasis(character),
                '~' => self.toggle_pair('~', |style| &mut style.strikethrough),
                '=' => self.toggle_pair('=', |style| &mut style.highlight),
                _ => false,
            };
            if !is_handled {
                self.buffer.push(character);
                self.position += 1;
            }
        }
        self.flush();
        self.spans
    }

    fn starts_with(&self, text: &str) -> bool {
        text.chars()
            .enumerate()
            .all(|(offset, c)| self.characters.get(self.position + offset) == Some(&c))
    }

    /// The offset from `from` of the next `needle`, in characters.
    fn find_from(&self, from: usize, needle: &str) -> Option<usize> {
        let needle: Vec<char> = needle.chars().collect();
        (from..self.characters.len())
            .find(|start| self.characters.get(*start..*start + needle.len()) == Some(&needle[..]))
    }

    fn flush(&mut self) {
        if self.buffer.is_empty() {
            return;
        }
        let text = std::mem::take(&mut self.buffer);
        self.push(Span {
            text,
            style: self.style,
            link: None,
        });
    }

    fn push(&mut self, span: Span) {
        if let Some(last) = self.spans.last_mut()
            && last.link.is_none()
            && span.link.is_none()
            && last.style == span.style
        {
            last.text.push_str(&span.text);
            return;
        }
        self.spans.push(span);
    }

    fn escape(&mut self) -> bool {
        match self.characters.get(self.position + 1) {
            Some(next) if next.is_ascii_punctuation() => {
                self.buffer.push(*next);
                self.position += 2;
                true
            }
            _ => false,
        }
    }

    fn code(&mut self) -> bool {
        let run = self.characters[self.position..]
            .iter()
            .take_while(|c| **c == '`')
            .count();
        let fence: String = "`".repeat(run);
        let mut search = self.position + run;
        while let Some(close) = self.find_from(search, &fence) {
            let close_run = self.characters[close..]
                .iter()
                .take_while(|c| **c == '`')
                .count();
            if close_run == run {
                let inner: String = self.characters[self.position + run..close].iter().collect();
                let inner = match inner.strip_prefix(' ').and_then(|s| s.strip_suffix(' ')) {
                    Some(stripped) if !stripped.trim().is_empty() => stripped.to_owned(),
                    _ => inner,
                };
                self.flush();
                self.push(Span {
                    text: inner,
                    style: SpanStyle {
                        code: true,
                        ..self.style
                    },
                    link: None,
                });
                self.position = close + run;
                return true;
            }
            search = close + close_run;
        }
        self.buffer.push_str(&fence);
        self.position += run;
        true
    }

    /// `%% hidden %%`: an Obsidian comment, never shown.
    fn comment(&mut self) -> bool {
        if !self.starts_with("%%") {
            return false;
        }
        match self.find_from(self.position + 2, "%%") {
            Some(close) => {
                self.position = close + 2;
                true
            }
            None => false,
        }
    }

    /// An inline `![[…]]` reads as a link to its target; `![alt](url)` as a link.
    fn embed_or_image(&mut self) -> bool {
        if !self.starts_with("![") {
            return false;
        }
        self.position += 1;
        if self.wikilink() || self.markdown_link() {
            return true;
        }
        self.position -= 1;
        false
    }

    fn wikilink(&mut self) -> bool {
        if !self.starts_with("[[") {
            return false;
        }
        let Some(close) = self.find_from(self.position + 2, "]]") else {
            return false;
        };
        let inner: String = self.characters[self.position + 2..close].iter().collect();
        let (target, label) = link_target(&inner);
        let text = label.unwrap_or_else(|| match (&target.section, target.target.is_empty()) {
            (Some(section), true) => section.clone(),
            (Some(section), false) => format!("{} > {section}", target.target),
            (None, _) => target.target.clone(),
        });
        self.flush();
        self.push(Span {
            text,
            style: self.style,
            link: Some(SpanLink::Note(target)),
        });
        self.position = close + 2;
        true
    }

    fn markdown_link(&mut self) -> bool {
        if !self.starts_with("[") {
            return false;
        }
        let Some(close) = self.find_from(self.position + 1, "](") else {
            return false;
        };
        let Some(end) = self.find_from(close + 2, ")") else {
            return false;
        };
        let text: String = self.characters[self.position + 1..close].iter().collect();
        let url: String = self.characters[close + 2..end].iter().collect();
        if url.contains(char::is_whitespace) && !url.starts_with('<') {
            return false;
        }
        let url = url.trim_start_matches('<').trim_end_matches('>').to_owned();
        self.flush();
        let shown: String = inline_spans(&text)
            .into_iter()
            .map(|span| span.text)
            .collect();
        self.push(Span {
            text: if shown.is_empty() { url.clone() } else { shown },
            style: self.style,
            link: Some(SpanLink::Url(url)),
        });
        self.position = end + 1;
        true
    }

    /// `*`, `**`, `***` and the `_` forms: open when a closer follows, close
    /// when the style is open. `_` never opens or closes inside a word.
    fn emphasis(&mut self, marker: char) -> bool {
        let run = self.characters[self.position..]
            .iter()
            .take_while(|c| **c == marker)
            .count()
            .min(3);
        let before = self
            .position
            .checked_sub(1)
            .and_then(|index| self.characters.get(index));
        let after = self.characters.get(self.position + run);
        let is_word = |c: Option<&char>| c.is_some_and(|c| c.is_alphanumeric());
        let can_open =
            after.is_some_and(|c| !c.is_whitespace()) && (marker == '*' || !is_word(before));
        let can_close =
            before.is_some_and(|c| !c.is_whitespace()) && (marker == '*' || !is_word(after));
        let delimiter: String = std::iter::repeat_n(marker, run).collect();
        let wants_strong = run >= 2;
        let wants_emphasis = run != 2;
        let is_open =
            (!wants_strong || self.style.strong) && (!wants_emphasis || self.style.emphasis);
        if is_open && can_close {
            self.flush();
            if wants_strong {
                self.style.strong = false;
            }
            if wants_emphasis {
                self.style.emphasis = false;
            }
        } else if can_open
            && self
                .find_from(self.position + run + 1, &delimiter)
                .is_some()
            && !(wants_strong && self.style.strong)
            && !(wants_emphasis && self.style.emphasis)
        {
            self.flush();
            if wants_strong {
                self.style.strong = true;
            }
            if wants_emphasis {
                self.style.emphasis = true;
            }
        } else {
            self.buffer.push_str(&delimiter);
        }
        self.position += run;
        true
    }

    /// `~~` and `==`: a style that opens when a closer follows.
    fn toggle_pair(&mut self, marker: char, flag: fn(&mut SpanStyle) -> &mut bool) -> bool {
        let pair: String = [marker, marker].iter().collect();
        if !self.starts_with(&pair) {
            return false;
        }
        let is_on = *flag(&mut self.style);
        if is_on || self.find_from(self.position + 2, &pair).is_some() {
            self.flush();
            let style = flag(&mut self.style);
            *style = !is_on;
        } else {
            self.buffer.push_str(&pair);
        }
        self.position += 2;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(spans: &[Span]) -> String {
        spans.iter().map(|span| span.text.as_str()).collect()
    }

    fn render(source: &str) -> Vec<RenderBlock> {
        render_note(&Document::parse(source))
    }

    #[test]
    fn inline_styles_become_spans() {
        let spans = inline_spans("a **bold** and *it* `co*de` ~~gone~~ ==mark== end");
        assert_eq!(plain(&spans), "a bold and it co*de gone mark end");
        let styled: Vec<(&str, SpanStyle)> = spans
            .iter()
            .filter(|span| span.style != SpanStyle::default())
            .map(|span| (span.text.as_str(), span.style))
            .collect();
        assert_eq!(styled[0].0, "bold");
        assert!(styled[0].1.strong);
        assert!(styled[1].1.emphasis);
        assert!(styled[2].1.code);
        assert!(styled[3].1.strikethrough);
        assert!(styled[4].1.highlight);
    }

    #[test]
    fn stray_delimiters_stay_as_written() {
        assert_eq!(
            plain(&inline_spans("2 * 3 = 6 and snake_case_name")),
            "2 * 3 = 6 and snake_case_name"
        );
        assert!(
            inline_spans("2 * 3")
                .iter()
                .all(|span| !span.style.emphasis)
        );
        assert_eq!(plain(&inline_spans("a ~ b == c")), "a ~ b == c");
        assert_eq!(plain(&inline_spans(r"\*not\*")), "*not*");
    }

    #[test]
    fn links_and_wikilinks_carry_their_targets() {
        let spans = inline_spans(
            "See [[Research notes]], [[Research notes#Sticky|the sweep]], [[#Notes]] and [site](https://x.test).",
        );
        let links: Vec<(&str, &SpanLink)> = spans
            .iter()
            .filter_map(|span| span.link.as_ref().map(|link| (span.text.as_str(), link)))
            .collect();
        assert_eq!(links.len(), 4);
        assert_eq!(links[0].0, "Research notes");
        assert_eq!(links[1].0, "the sweep");
        assert!(
            matches!(links[1].1, SpanLink::Note(target) if target.section.as_deref() == Some("Sticky"))
        );
        assert_eq!(links[2].0, "Notes");
        assert!(matches!(links[2].1, SpanLink::Note(target) if target.target.is_empty()));
        assert_eq!(links[3].1, &SpanLink::Url("https://x.test".to_owned()));
    }

    #[test]
    fn comments_and_markers_are_never_shown() {
        let blocks = render(
            "# T\n%% ai:begin fable · 2026-10-20 · abc %%\nKept %%hidden%% text.\n%% ai:end %%\n%%\nblock comment\n%%\nAfter.\n",
        );
        let text = format!("{blocks:?}");
        assert!(!text.contains("ai:"), "{text}");
        assert!(!text.contains("hidden"), "{text}");
        assert!(!text.contains("block comment"), "{text}");
        assert!(text.contains("Kept"), "{text}");
        assert!(text.contains("After."), "{text}");
    }

    #[test]
    fn the_task_sections_become_app_views_and_activity_queries_too() {
        let blocks = render(
            "# T\n\n## Next Steps\n- [ ] a  [id:: task-1]\n### Sub\nhidden\n\n## Pending Input\n- [ ] b\n\n## Activity\n```dataview\nLIST\n```\n\n## Other\n```dataview\nTABLE x\n```\n",
        );
        assert!(
            matches!(blocks[0], RenderBlock::Heading { ref title, .. } if title == "Next Steps")
        );
        assert!(matches!(
            blocks[1],
            RenderBlock::AppView {
                view: AppView::NextSteps,
                line: 3
            }
        ));
        assert!(matches!(
            blocks[3],
            RenderBlock::AppView {
                view: AppView::PendingInput,
                ..
            }
        ));
        assert!(
            matches!(blocks[5], RenderBlock::Query { view: Some(AppView::Activity), ref language, .. } if language == "dataview")
        );
        assert!(
            matches!(blocks[7], RenderBlock::Query { view: None, ref source, .. } if source == "TABLE x")
        );
        let text = format!("{blocks:?}");
        assert!(!text.contains("hidden"));
    }

    #[test]
    fn callouts_quotes_tables_rules_and_embeds() {
        let blocks = render(
            "# T\n> [!warning]- Mind\n> - [ ] inside  [id:: task-9]\n> text\n\n> plain quote\n\n| A | B |\n| :-- | --: |\n| 1 | **2** |\n\n---\n![[sketch.png|300]]\n![[Flow.excalidraw]]\n![[Other note#Part]]\n![[file.pdf]]\n",
        );
        let RenderBlock::Callout {
            kind,
            fold,
            title,
            children,
            ..
        } = &blocks[0]
        else {
            panic!("{blocks:?}");
        };
        assert_eq!(kind, "warning");
        assert_eq!(*fold, CalloutFold::Folded);
        assert_eq!(plain(title), "Mind");
        let RenderBlock::List { items, .. } = &children[0] else {
            panic!("{children:?}");
        };
        assert_eq!(
            items[0].line, 2,
            "lines inside a quote keep their place in the note"
        );
        assert_eq!(
            items[0].checkbox,
            Some(Checkbox {
                status: TaskStatus::Open,
                task_id: Some("task-9".to_owned())
            })
        );
        assert!(matches!(&blocks[1], RenderBlock::Quote { children, .. } if children.len() == 1));
        let RenderBlock::Table {
            alignments,
            header,
            rows,
            ..
        } = &blocks[2]
        else {
            panic!("{blocks:?}");
        };
        assert_eq!(alignments, &[ColumnAlignment::Left, ColumnAlignment::Right]);
        assert_eq!(plain(&header[1]), "B");
        assert!(rows[0][1][0].style.strong);
        assert!(matches!(blocks[3], RenderBlock::Rule { .. }));
        assert!(
            matches!(&blocks[4], RenderBlock::Embed { kind: EmbedKind::Image, target, .. } if target.target == "sketch.png")
        );
        assert!(
            matches!(&blocks[5], RenderBlock::Embed { kind: EmbedKind::Plugin(name), .. } if name == "Excalidraw")
        );
        assert!(
            matches!(&blocks[6], RenderBlock::Embed { kind: EmbedKind::Note, target, .. } if target.section.as_deref() == Some("Part"))
        );
        assert!(matches!(
            &blocks[7],
            RenderBlock::Embed {
                kind: EmbedKind::File,
                ..
            }
        ));
    }

    #[test]
    fn lists_nest_by_indent_and_number() {
        let blocks = render("# T\n1. one\n\t- [x] done  [id:: task-2]\n\t\t- deep\n2. two\n");
        let RenderBlock::List {
            is_ordered, items, ..
        } = &blocks[0]
        else {
            panic!("{blocks:?}");
        };
        assert!(is_ordered);
        assert_eq!(items.len(), 2);
        assert_eq!(
            items[0].children[0].checkbox.as_ref().map(|c| c.status),
            Some(TaskStatus::Done)
        );
        assert_eq!(
            plain(&items[0].children[0].spans),
            "done",
            "inline fields are not prose"
        );
        assert_eq!(plain(&items[0].children[0].children[0].spans), "deep");
    }

    #[test]
    fn paragraph_lines_keep_their_breaks_and_code_keeps_its_language() {
        let blocks = render("# T\nfirst line\nsecond line\n\n```swift\nlet x = 1\n```\n");
        assert!(
            matches!(&blocks[0], RenderBlock::Paragraph { spans, .. } if plain(spans) == "first line\nsecond line")
        );
        assert!(
            matches!(&blocks[1], RenderBlock::Code { language: Some(language), text, .. } if language == "swift" && text == "let x = 1")
        );
    }

    #[test]
    fn authorship_counts_lines_inside_units() {
        let human = authorship(&Document::parse("# T\ntext\n"));
        assert_eq!(human.kind(), AuthorshipKind::Human);
        let mixed = authorship(&Document::parse(
            "# T\n%% ai:begin fable · 2026-10-20 · abc %%\nmodel\n%% ai:end %%\n%% ai:begin fable · 2026-10-21 · def %%\nmore\n%% ai:end %%\n",
        ));
        assert_eq!(mixed.kind(), AuthorshipKind::Mixed);
        assert_eq!(mixed.model_units, 2);
        assert_eq!(mixed.model_lines, 2);
        assert_eq!(mixed.latest_model_date, Date::parse("2026-10-21"));
        let model = authorship(&Document::parse(
            "%% ai:note fable · 2026-10-19 · x %%\n# T\nall model\n",
        ));
        assert_eq!(model.kind(), AuthorshipKind::Model);
    }
}
