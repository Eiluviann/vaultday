//! The line-level markdown scanner. Every line keeps its bytes; the scanner
//! only classifies lines (frontmatter, fences, provenance markers, headings,
//! tasks, list items) and lifts the structure the app needs out of them. There
//! is no CommonMark tree: the writer edits lines, so line fidelity is the model.

use crate::frontmatter::Frontmatter;
use crate::line::{LineEnding, split_lines};

/// A parsed note. `render` reproduces the source byte for byte.
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    lines: Vec<Line>,
    frontmatter: Option<Frontmatter>,
    body_start: usize,
    tasks: Vec<Task>,
}

/// One source line: its text without the terminator, the terminator, and what
/// the scanner made of it.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    /// The line's content without its terminator.
    pub text: String,
    /// The terminator that followed the content.
    pub ending: LineEnding,
    /// The scanner's classification of the line.
    pub kind: LineKind,
}

/// What a line is. Only lines outside frontmatter and fenced code can be
/// markers, headings, tasks or list items.
#[derive(Debug, Clone, PartialEq)]
pub enum LineKind {
    /// The opening or closing `---` of the frontmatter block.
    FrontmatterDelimiter,
    /// A line inside the frontmatter block.
    Frontmatter,
    /// A line that opens or closes a fenced code block.
    Fence,
    /// A line inside a fenced code block.
    Fenced,
    /// A `%% ai:… %%` provenance marker line, passed through untouched.
    Marker(Marker),
    /// An ATX heading.
    Heading(Heading),
    /// A task line; the index points into [`Document::tasks`].
    Task(usize),
    /// A list item that is not a task.
    ListItem(ListItem),
    /// A line holding only whitespace.
    Blank,
    /// Any other line.
    Text,
}

/// A `%% ai:… %%` line. The scanner reads only the kind; the hash, author and
/// date stay opaque text for the provenance tooling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Marker {
    /// Which marker this is.
    pub kind: MarkerKind,
    /// Blockquote prefix in front of the marker, such as `> `.
    pub quote_prefix: String,
    /// Everything between the kind word and the closing `%%`, trimmed.
    pub attributes: String,
}

/// The provenance marker kinds the hook writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarkerKind {
    /// `%% ai:note … %%`: the whole rest of the note is model text.
    Note,
    /// `%% ai:begin … %%`: opens a model-text block.
    Begin,
    /// `%% ai:end %%`: closes a model-text block.
    End,
    /// Any other `%% ai:<word> … %%`, kept as written.
    Other(String),
}

/// An ATX heading outside fences and frontmatter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    /// Number of `#`, 1 to 6.
    pub level: u8,
    /// The heading text, trimmed, without closing `#`s.
    pub title: String,
}

/// A heading and the lines beneath it up to the next heading of the same or a
/// higher level. The preamble before the first heading is a level-0 section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    /// Line index of the heading; `None` for the preamble.
    pub heading_line: Option<usize>,
    /// Heading level, 0 for the preamble.
    pub level: u8,
    /// Heading title, empty for the preamble.
    pub title: String,
    /// First body line index.
    pub start: usize,
    /// One past the last body line index.
    pub end: usize,
}

/// A list item that is not a task: `- 09:05 Did a thing [kind:: done]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListItem {
    /// Leading blockquote prefixes and whitespace, as written.
    pub indent: String,
    /// The bullet, `-`, `*`, `+` or `1.`/`1)`.
    pub bullet: String,
    /// The text after the bullet with the inline fields removed, trimmed.
    pub text: String,
    /// Inline `[key:: value]` fields in source order.
    pub fields: Vec<InlineField>,
}

/// The status character between the brackets of a task line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TaskStatus {
    /// `[ ]`
    Open,
    /// `[x]` or `[X]`
    Done,
    /// `[-]`
    Cancelled,
    /// `[/]`
    InProgress,
    /// Any other single character, kept as written.
    Other(char),
}

/// A task line with its nesting resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    /// Index of the line in the document.
    pub line: usize,
    /// Leading blockquote prefixes and whitespace, as written.
    pub indent: String,
    /// The bullet, `-`, `*`, `+` or `1.`/`1)`.
    pub bullet: String,
    /// The status character, classified.
    pub status: TaskStatus,
    /// The description with the inline fields removed and spaces collapsed.
    pub text: String,
    /// Inline `[key:: value]` fields in source order.
    pub fields: Vec<InlineField>,
    /// Nesting depth, 0 for a top-level task.
    pub depth: usize,
    /// Index into [`Document::tasks`] of the enclosing task, if any.
    pub parent: Option<usize>,
}

/// An inline Dataview field `[key:: value]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineField {
    /// The key, lowercase letters and underscores.
    pub key: String,
    /// The value, trimmed.
    pub value: String,
    /// Byte offset of the opening `[` within the line text.
    pub start: usize,
    /// Byte offset one past the closing `]` within the line text.
    pub end: usize,
}

/// A provenance unit: the lines a `%% ai:note %%` or a `%% ai:begin %% … %% ai:end %%`
/// pair claims as model text. Unclosed blocks are not units.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvenanceUnit {
    /// Line index of the `ai:note` or `ai:begin` marker.
    pub marker_line: usize,
    /// Line index of the `ai:end` marker; `None` for a note-level unit.
    pub end_line: Option<usize>,
    /// First body line index.
    pub body_start: usize,
    /// One past the last body line index.
    pub body_end: usize,
}

/// The opening run of a fenced code block, kept to match its closer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FenceRun {
    marker: char,
    length: usize,
    has_info: bool,
}

impl FenceRun {
    /// A closer uses the same character, is at least as long and carries no info string.
    fn closes(self, open: FenceRun) -> bool {
        self.marker == open.marker && self.length >= open.length && !self.has_info
    }
}

const FRONTMATTER_OPEN: &str = "---";
const FRONTMATTER_CLOSERS: [&str; 2] = ["---", "..."];
const MARKER_HEAD: &str = "%% ai:";
const MARKER_TAIL: &str = "%%";
const QUOTE_PREFIX: &str = "> ";
const TAB_WIDTH: usize = 4;
const MAX_BULLET_GAP_SPACES: usize = 4;
const BYTE_ORDER_MARK: char = '\u{FEFF}';

impl Document {
    /// Scans a note. Scanning never fails: unexpected shapes become plain
    /// `Text` lines and the frontmatter records its own parse error.
    #[must_use]
    pub fn parse(source: &str) -> Document {
        let raw_lines = split_lines(source);
        let body_start = find_body_start(&raw_lines);
        let frontmatter = (body_start > 0).then(|| {
            let yaml: Vec<&str> = raw_lines
                .get(1..body_start - 1)
                .unwrap_or_default()
                .iter()
                .map(|(text, _)| text.as_str())
                .collect();
            Frontmatter::parse(&yaml.join("\n"))
        });

        let mut lines = Vec::with_capacity(raw_lines.len());
        let mut tasks = Vec::new();
        let mut open_fence: Option<FenceRun> = None;
        for (index, (text, ending)) in raw_lines.into_iter().enumerate() {
            let kind = if index < body_start {
                if index == 0 || index == body_start - 1 {
                    LineKind::FrontmatterDelimiter
                } else {
                    LineKind::Frontmatter
                }
            } else if let Some(open) = open_fence {
                if fence_run(&text).is_some_and(|run| run.closes(open)) {
                    open_fence = None;
                    LineKind::Fence
                } else {
                    LineKind::Fenced
                }
            } else if let Some(run) = fence_run(&text) {
                open_fence = Some(run);
                LineKind::Fence
            } else {
                classify_body_line(&text, index, &mut tasks)
            };
            lines.push(Line { text, ending, kind });
        }
        resolve_task_nesting(&lines, &mut tasks);
        Document {
            lines,
            frontmatter,
            body_start,
            tasks,
        }
    }

    /// Reassembles the source from the lines, byte for byte.
    #[must_use]
    pub fn render(&self) -> String {
        let mut output = String::new();
        for line in &self.lines {
            output.push_str(&line.text);
            output.push_str(line.ending.as_str());
        }
        output
    }

    /// Every line in order.
    #[must_use]
    pub fn lines(&self) -> &[Line] {
        &self.lines
    }

    /// The parsed frontmatter, when the note opens with a `---` block.
    #[must_use]
    pub fn frontmatter(&self) -> Option<&Frontmatter> {
        self.frontmatter.as_ref()
    }

    /// Index of the first line after the frontmatter block, 0 without one.
    #[must_use]
    pub fn body_start(&self) -> usize {
        self.body_start
    }

    /// Every task line in document order, nesting resolved.
    #[must_use]
    pub fn tasks(&self) -> &[Task] {
        &self.tasks
    }

    /// Direct subtasks of the task at `index` into [`Document::tasks`].
    #[must_use]
    pub fn subtasks(&self, index: usize) -> Vec<&Task> {
        self.tasks
            .iter()
            .filter(|task| task.parent == Some(index))
            .collect()
    }

    /// Headings with their line indices, in order.
    pub fn headings(&self) -> impl Iterator<Item = (usize, &Heading)> {
        self.lines
            .iter()
            .enumerate()
            .filter_map(|(index, line)| match &line.kind {
                LineKind::Heading(heading) => Some((index, heading)),
                _ => None,
            })
    }

    /// The first heading of level 1, usually the note title.
    #[must_use]
    pub fn title(&self) -> Option<&str> {
        self.headings()
            .find(|(_, heading)| heading.level == 1)
            .map(|(_, heading)| heading.title.as_str())
    }

    /// Marker lines with their line indices, in order.
    pub fn markers(&self) -> impl Iterator<Item = (usize, &Marker)> {
        self.lines
            .iter()
            .enumerate()
            .filter_map(|(index, line)| match &line.kind {
                LineKind::Marker(marker) => Some((index, marker)),
                _ => None,
            })
    }

    /// List items that are not tasks, with their line indices.
    pub fn list_items(&self) -> impl Iterator<Item = (usize, &ListItem)> {
        self.lines
            .iter()
            .enumerate()
            .filter_map(|(index, line)| match &line.kind {
                LineKind::ListItem(item) => Some((index, item)),
                _ => None,
            })
    }

    /// The note split into sections: the preamble, then one per heading. A
    /// section runs until the next heading of the same or a higher level.
    #[must_use]
    pub fn sections(&self) -> Vec<Section> {
        let headings: Vec<(usize, u8, &str)> = self
            .headings()
            .map(|(index, heading)| (index, heading.level, heading.title.as_str()))
            .collect();
        let first_heading = headings
            .first()
            .map_or(self.lines.len(), |(index, _, _)| *index);
        let mut sections = vec![Section {
            heading_line: None,
            level: 0,
            title: String::new(),
            start: self.body_start.min(first_heading),
            end: first_heading,
        }];
        for (position, (line, level, title)) in headings.iter().enumerate() {
            let end = headings
                .get(position + 1..)
                .unwrap_or_default()
                .iter()
                .find(|(_, other_level, _)| other_level <= level)
                .map_or(self.lines.len(), |(other_line, _, _)| *other_line);
            sections.push(Section {
                heading_line: Some(*line),
                level: *level,
                title: (*title).to_owned(),
                start: line + 1,
                end,
            });
        }
        sections
    }

    /// The first section whose heading title equals `title` (trimmed, exact case).
    #[must_use]
    pub fn section(&self, title: &str) -> Option<Section> {
        let wanted = title.trim();
        self.sections()
            .into_iter()
            .find(|section| section.heading_line.is_some() && section.title == wanted)
    }

    /// Tasks whose line lies inside `section`, nested ones included.
    #[must_use]
    pub fn tasks_in(&self, section: &Section) -> Vec<&Task> {
        self.tasks
            .iter()
            .filter(|task| section.start <= task.line && task.line < section.end)
            .collect()
    }

    /// Tasks of the first section titled `title`; empty when there is none.
    #[must_use]
    pub fn tasks_under(&self, title: &str) -> Vec<&Task> {
        self.section(title)
            .map(|section| self.tasks_in(&section))
            .unwrap_or_default()
    }

    /// The provenance units: an `ai:note` claims the rest of the note, an
    /// `ai:begin` claims the lines up to its `ai:end`. Nested or unclosed
    /// blocks are ignored, like the hook ignores them.
    #[must_use]
    pub fn provenance_units(&self) -> Vec<ProvenanceUnit> {
        let mut units = Vec::new();
        let mut open_block: Option<usize> = None;
        for (index, marker) in self.markers() {
            match (&marker.kind, open_block) {
                (MarkerKind::Note, None) => units.push(ProvenanceUnit {
                    marker_line: index,
                    end_line: None,
                    body_start: index + 1,
                    body_end: self.lines.len(),
                }),
                (MarkerKind::Begin, None) => open_block = Some(index),
                (MarkerKind::End, Some(begin)) => {
                    units.push(ProvenanceUnit {
                        marker_line: begin,
                        end_line: Some(index),
                        body_start: begin + 1,
                        body_end: index,
                    });
                    open_block = None;
                }
                _ => {}
            }
        }
        units
    }

    /// True when the line at `index` lies inside a provenance unit's body.
    #[must_use]
    pub fn is_model_text(&self, index: usize) -> bool {
        self.provenance_units()
            .iter()
            .any(|unit| unit.body_start <= index && index < unit.body_end)
    }
}

impl TaskStatus {
    /// Classifies the character between the brackets.
    #[must_use]
    pub fn from_char(status: char) -> TaskStatus {
        match status {
            ' ' => TaskStatus::Open,
            'x' | 'X' => TaskStatus::Done,
            '-' => TaskStatus::Cancelled,
            '/' => TaskStatus::InProgress,
            other => TaskStatus::Other(other),
        }
    }

    /// The character to write between the brackets.
    #[must_use]
    pub fn as_char(self) -> char {
        match self {
            TaskStatus::Open => ' ',
            TaskStatus::Done => 'x',
            TaskStatus::Cancelled => '-',
            TaskStatus::InProgress => '/',
            TaskStatus::Other(other) => other,
        }
    }

    /// True for `[ ]` and `[/]`.
    #[must_use]
    pub fn is_open(self) -> bool {
        matches!(self, TaskStatus::Open | TaskStatus::InProgress)
    }
}

impl Task {
    /// The value of the first field with `key`.
    #[must_use]
    pub fn field(&self, key: &str) -> Option<&str> {
        field_value(&self.fields, key)
    }

    /// The `[id:: …]` value.
    #[must_use]
    pub fn id(&self) -> Option<&str> {
        self.field("id")
    }

    /// True for `[ ]` and `[/]`.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.status.is_open()
    }
}

impl ListItem {
    /// The value of the first field with `key`.
    #[must_use]
    pub fn field(&self, key: &str) -> Option<&str> {
        field_value(&self.fields, key)
    }
}

impl Section {
    /// True when the line index lies in the section body.
    #[must_use]
    pub fn contains(&self, line: usize) -> bool {
        self.start <= line && line < self.end
    }
}

/// Parses every `[key:: value]` outside inline code spans. Keys are lowercase
/// letters and underscores; the value runs to the matching `]`, so a wikilink
/// value such as `[[Sam Lee]]` is kept whole. Offsets are into `text`.
#[must_use]
pub fn inline_fields(text: &str) -> Vec<InlineField> {
    inline_fields_at(text, 0)
}

/// Like [`inline_fields`], scanning only from byte offset `from`; offsets stay
/// relative to the start of `text`.
fn inline_fields_at(text: &str, from: usize) -> Vec<InlineField> {
    let mut fields = Vec::new();
    let mut offset = from;
    while offset < text.len() {
        let Some(rest) = text.get(offset..) else {
            break;
        };
        if rest.starts_with('`') {
            let run = rest.len() - rest.trim_start_matches('`').len();
            // A code span closes on a run of exactly the same length; without
            // one the backticks are literal text.
            offset += match find_closing_backticks(rest, run) {
                Some(span_end) => span_end,
                None => run,
            };
            continue;
        }
        if rest.starts_with('[')
            && let Some((field_length, key, value)) = parse_field_at(rest)
        {
            fields.push(InlineField {
                key,
                value,
                start: offset,
                end: offset + field_length,
            });
            offset += field_length;
            continue;
        }
        offset += rest.chars().next().map_or(1, char::len_utf8);
    }
    fields
}

/// Byte length of a code span that opens `text` with `run` backticks and
/// closes on the next run of exactly `run` backticks; `None` when unclosed.
fn find_closing_backticks(text: &str, run: usize) -> Option<usize> {
    let mut offset = run;
    while offset < text.len() {
        let rest = text.get(offset..)?;
        if rest.starts_with('`') {
            let length = rest.len() - rest.trim_start_matches('`').len();
            if length == run {
                return Some(offset + length);
            }
            offset += length;
            continue;
        }
        offset += rest.chars().next().map_or(1, char::len_utf8);
    }
    None
}

/// Index of the first body line: one past the closing delimiter of a
/// frontmatter block that opens on line 0, else 0. A byte order mark before
/// the opener is tolerated (Windows editors write one); it stays in the line.
fn find_body_start(lines: &[(String, LineEnding)]) -> usize {
    let opener = lines
        .first()
        .map(|(text, _)| text.strip_prefix(BYTE_ORDER_MARK).unwrap_or(text));
    if opener != Some(FRONTMATTER_OPEN) {
        return 0;
    }
    lines
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, (text, _))| FRONTMATTER_CLOSERS.contains(&text.as_str()))
        .map_or(0, |(index, _)| index + 1)
}

/// A fence line: three or more of the same character, backtick or tilde,
/// after optional quote prefixes and any run of spaces or tabs. The indent
/// is not limited to three spaces as in CommonMark: a fence nested under a
/// list item is indented by a tab or four spaces in a vault, and a task line
/// inside it must never count as a task.
fn fence_run(text: &str) -> Option<FenceRun> {
    let unquoted = strip_quote_prefixes(text).1;
    let after_indent = unquoted.trim_start_matches([' ', '\t']);
    let marker = after_indent
        .chars()
        .next()
        .filter(|c| *c == '`' || *c == '~')?;
    let length = after_indent.len() - after_indent.trim_start_matches(marker).len();
    if length < 3 {
        return None;
    }
    let info = after_indent.get(length..)?;
    // A backtick info string may not hold backticks (CommonMark 4.5).
    if marker == '`' && info.contains('`') {
        return None;
    }
    Some(FenceRun {
        marker,
        length,
        has_info: !info.trim().is_empty(),
    })
}

fn classify_body_line(text: &str, index: usize, tasks: &mut Vec<Task>) -> LineKind {
    if text.trim().is_empty() {
        return LineKind::Blank;
    }
    if let Some(marker) = parse_marker(text) {
        return LineKind::Marker(marker);
    }
    if let Some(heading) = parse_heading(text) {
        return LineKind::Heading(heading);
    }
    if let Some((indent, bullet, rest)) = split_list_item(text) {
        if is_thematic_break(bullet, rest) {
            return LineKind::Text;
        }
        if let Some((status, description)) = split_task_checkbox(rest) {
            let fields = inline_fields_at(text, text.len() - description.len());
            tasks.push(Task {
                line: index,
                indent: indent.to_owned(),
                bullet: bullet.to_owned(),
                status: TaskStatus::from_char(status),
                text: strip_fields(text, text.len() - description.len(), &fields),
                fields,
                depth: 0,
                parent: None,
            });
            return LineKind::Task(tasks.len() - 1);
        }
        let fields = inline_fields_at(text, text.len() - rest.len());
        return LineKind::ListItem(ListItem {
            indent: indent.to_owned(),
            bullet: bullet.to_owned(),
            text: strip_fields(text, text.len() - rest.len(), &fields),
            fields,
        });
    }
    LineKind::Text
}

/// `- - -` and `* * *` are thematic breaks, not list items.
fn is_thematic_break(bullet: &str, rest: &str) -> bool {
    let Some(marker) = bullet
        .chars()
        .next()
        .filter(|c| matches!(c, '-' | '*' | '+'))
    else {
        return false;
    };
    if marker == '+' {
        return false;
    }
    let is_only_markers = rest.chars().all(|c| c == marker || c == ' ' || c == '\t');
    is_only_markers && rest.chars().filter(|c| *c == marker).count() >= 2
}

/// Splits `> > ` style prefixes off a line: `(prefix, rest)`.
fn strip_quote_prefixes(text: &str) -> (&str, &str) {
    let mut consumed = 0;
    while text
        .get(consumed..)
        .is_some_and(|rest| rest.starts_with(QUOTE_PREFIX))
    {
        consumed += QUOTE_PREFIX.len();
    }
    text.split_at(consumed)
}

fn parse_marker(text: &str) -> Option<Marker> {
    let (quote_prefix, unquoted) = strip_quote_prefixes(text);
    let after_head = unquoted.strip_prefix(MARKER_HEAD)?;
    let inner = after_head.trim_end().strip_suffix(MARKER_TAIL)?.trim();
    let mut words = inner.splitn(2, char::is_whitespace);
    let word = words.next().unwrap_or_default();
    let attributes = words.next().unwrap_or_default();
    let kind = match word {
        "note" => MarkerKind::Note,
        "begin" => MarkerKind::Begin,
        "end" => MarkerKind::End,
        other => MarkerKind::Other(other.to_owned()),
    };
    Some(Marker {
        kind,
        quote_prefix: quote_prefix.to_owned(),
        attributes: attributes.trim().to_owned(),
    })
}

fn parse_heading(text: &str) -> Option<Heading> {
    let hashes = text.len() - text.trim_start_matches('#').len();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let after_hashes = text.get(hashes..)?;
    if !after_hashes.starts_with([' ', '\t']) {
        return None;
    }
    let trimmed = after_hashes.trim();
    // CommonMark: a closing run of `#` counts only after a space; `C#` keeps its hash.
    let without_closer = trimmed.trim_end_matches('#');
    let title = if without_closer.is_empty() || without_closer.ends_with([' ', '\t']) {
        without_closer.trim_end()
    } else {
        trimmed
    };
    Some(Heading {
        level: u8::try_from(hashes).ok()?,
        title: title.to_owned(),
    })
}

/// Splits `(indent, bullet, rest)` off a list line: optional `> ` prefixes,
/// whitespace, a bullet or `N.`/`N)`, one to four spaces or a tab, then the
/// rest. Five or more spaces make the rest an indented code block, so the
/// surplus stays in `rest` and a checkbox there is not a task.
fn split_list_item(text: &str) -> Option<(&str, &str, &str)> {
    let (quote_prefix, unquoted) = strip_quote_prefixes(text);
    let whitespace_length = unquoted.len() - unquoted.trim_start_matches([' ', '\t']).len();
    let indent_length = quote_prefix.len() + whitespace_length;
    let after_indent = text.get(indent_length..)?;
    let bullet_length = if after_indent.starts_with(['-', '*', '+']) {
        1
    } else {
        let digits = after_indent.len()
            - after_indent
                .trim_start_matches(|c: char| c.is_ascii_digit())
                .len();
        if digits == 0 || !after_indent.get(digits..)?.starts_with(['.', ')']) {
            return None;
        }
        digits + 1
    };
    let after_bullet = after_indent.get(bullet_length..)?;
    let rest = split_bullet_gap(after_bullet)?;
    Some((
        text.get(..indent_length)?,
        after_indent.get(..bullet_length)?,
        rest,
    ))
}

/// The text after the gap that follows a bullet: a tab, or one to four
/// spaces; `None` without any.
fn split_bullet_gap(after_bullet: &str) -> Option<&str> {
    if let Some(rest) = after_bullet.strip_prefix('\t') {
        return Some(rest);
    }
    let spaces = after_bullet.len() - after_bullet.trim_start_matches(' ').len();
    if spaces == 0 {
        return None;
    }
    after_bullet.get(spaces.min(MAX_BULLET_GAP_SPACES)..)
}

/// Splits `[x] description` into the status character and the description.
/// The description may be empty (`- [ ]`), otherwise one space follows `]`.
fn split_task_checkbox(rest: &str) -> Option<(char, &str)> {
    let after_open = rest.strip_prefix('[')?;
    let status = after_open.chars().next()?;
    let after_status = after_open.get(status.len_utf8()..)?;
    let after_close = after_status.strip_prefix(']')?;
    if after_close.is_empty() {
        return Some((status, ""));
    }
    after_close
        .strip_prefix(' ')
        .map(|description| (status, description))
}

/// Parses `[key:: value]` at the start of `rest`; returns the field length and
/// parts. Brackets inside the value nest, so `[who:: [[Sam]]]` closes at the end.
fn parse_field_at(rest: &str) -> Option<(usize, String, String)> {
    let after_open = rest.strip_prefix('[')?;
    let key_length = after_open.len()
        - after_open
            .trim_start_matches(|c: char| c.is_ascii_lowercase() || c == '_')
            .len();
    if key_length == 0 {
        return None;
    }
    let after_key = after_open.get(key_length..)?.strip_prefix("::")?;
    let mut depth = 1usize;
    let mut close_at = None;
    for (position, character) in after_key.char_indices() {
        match character {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    close_at = Some(position);
                    break;
                }
            }
            _ => {}
        }
    }
    let close_at = close_at?;
    let key = after_open.get(..key_length)?.to_owned();
    let value = after_key.get(..close_at)?.trim().to_owned();
    let field_length = 1 + key_length + 2 + close_at + 1;
    Some((field_length, key, value))
}

/// The text from byte `from` with the field spans cut out, runs of spaces
/// collapsed and the result trimmed.
fn strip_fields(text: &str, from: usize, fields: &[InlineField]) -> String {
    let mut remaining = String::with_capacity(text.len());
    let mut cursor = from;
    for field in fields {
        if let Some(segment) = text.get(cursor..field.start) {
            remaining.push_str(segment);
        }
        cursor = field.end;
    }
    if let Some(tail) = text.get(cursor..) {
        remaining.push_str(tail);
    }
    let mut collapsed = String::with_capacity(remaining.len());
    let mut previous_was_space = false;
    for character in remaining.trim().chars() {
        let is_space = character == ' ';
        if !(is_space && previous_was_space) {
            collapsed.push(character);
        }
        previous_was_space = is_space;
    }
    collapsed
}

fn field_value<'a>(fields: &'a [InlineField], key: &str) -> Option<&'a str> {
    fields
        .iter()
        .find(|field| field.key == key)
        .map(|field| field.value.as_str())
}

/// Display width of an indent string: tabs count four columns, every other
/// character one. Quote prefixes count like text so quoted lists nest among
/// themselves.
fn indent_width(indent: &str) -> usize {
    indent
        .chars()
        .map(|character| if character == '\t' { TAB_WIDTH } else { 1 })
        .sum()
}

/// Assigns `parent` and `depth` by indent width. A task nests under the
/// nearest preceding task with a narrower indent in the same list; any
/// unindented non-list line, a heading or a change of blockquote depth ends
/// the list.
fn resolve_task_nesting(lines: &[Line], tasks: &mut [Task]) {
    let mut stack: Vec<(usize, usize)> = Vec::new();
    let mut list_quote_depth = 0usize;
    for line in lines {
        match &line.kind {
            LineKind::Task(task_index) => {
                let Some(task) = tasks.get_mut(*task_index) else {
                    continue;
                };
                let quote_depth = quote_depth(&task.indent);
                if quote_depth != list_quote_depth {
                    stack.clear();
                    list_quote_depth = quote_depth;
                }
                let width = indent_width(&task.indent);
                while stack
                    .last()
                    .is_some_and(|(top_width, _)| *top_width >= width)
                {
                    stack.pop();
                }
                task.parent = stack.last().map(|(_, parent)| *parent);
                task.depth = stack.len();
                stack.push((width, *task_index));
            }
            LineKind::ListItem(item) => {
                let depth = quote_depth(&item.indent);
                if depth != list_quote_depth {
                    stack.clear();
                    list_quote_depth = depth;
                }
                let width = indent_width(&item.indent);
                while stack
                    .last()
                    .is_some_and(|(top_width, _)| *top_width >= width)
                {
                    stack.pop();
                }
            }
            LineKind::Blank | LineKind::Fence | LineKind::Fenced | LineKind::Marker(_) => {}
            LineKind::Text => {
                let (quote_prefix, unquoted) = strip_quote_prefixes(&line.text);
                let depth = quote_prefix.len() / QUOTE_PREFIX.len();
                if depth != list_quote_depth || !unquoted.starts_with([' ', '\t']) {
                    stack.clear();
                    list_quote_depth = depth;
                }
            }
            LineKind::Heading(_) | LineKind::Frontmatter | LineKind::FrontmatterDelimiter => {
                stack.clear();
            }
        }
    }
}

/// Number of `> ` prefixes at the start of an indent string.
fn quote_depth(indent: &str) -> usize {
    strip_quote_prefixes(indent).0.len() / QUOTE_PREFIX.len()
}

#[cfg(test)]
mod tests {
    use super::{Document, LineKind, MarkerKind, TaskStatus, inline_fields};

    #[test]
    fn classifies_frontmatter_fences_markers_headings_and_tasks() {
        let source = "---\ntype: story\n---\n# Title\n\n```md\n- [ ] not a task\n%% ai:begin %%\n```\n%% ai:begin fable · 2026-10-22 · 0123456789ab %%\n- [x] done  [id:: task-1]\n%% ai:end %%\n> %% ai:note %%\n";
        let document = Document::parse(source);
        assert_eq!(document.render(), source);
        let kinds: Vec<&LineKind> = document.lines().iter().map(|line| &line.kind).collect();
        assert!(matches!(kinds[0], LineKind::FrontmatterDelimiter));
        assert!(matches!(kinds[1], LineKind::Frontmatter));
        assert!(matches!(kinds[2], LineKind::FrontmatterDelimiter));
        assert!(
            matches!(kinds[3], LineKind::Heading(heading) if heading.level == 1 && heading.title == "Title")
        );
        assert!(matches!(kinds[4], LineKind::Blank));
        assert!(matches!(kinds[5], LineKind::Fence));
        assert!(matches!(kinds[6], LineKind::Fenced));
        assert!(matches!(kinds[7], LineKind::Fenced));
        assert!(matches!(kinds[8], LineKind::Fence));
        assert!(
            matches!(kinds[9], LineKind::Marker(marker) if marker.kind == MarkerKind::Begin && marker.attributes == "fable · 2026-10-22 · 0123456789ab")
        );
        assert!(matches!(kinds[10], LineKind::Task(0)));
        assert!(matches!(kinds[11], LineKind::Marker(marker) if marker.kind == MarkerKind::End));
        assert!(
            matches!(kinds[12], LineKind::Marker(marker) if marker.kind == MarkerKind::Note && marker.quote_prefix == "> ")
        );
        assert_eq!(document.tasks().len(), 1);
        assert_eq!(document.tasks()[0].status, TaskStatus::Done);
        assert_eq!(document.tasks()[0].id(), Some("task-1"));
        assert_eq!(document.tasks()[0].text, "done");
        assert_eq!(document.body_start(), 3);
        assert!(document.is_model_text(10));
        assert!(!document.is_model_text(9));
    }

    #[test]
    fn an_unclosed_frontmatter_is_body_text() {
        let document = Document::parse("---\ntype: x\n# Heading\n");
        assert!(document.frontmatter().is_none());
        assert_eq!(document.body_start(), 0);
        assert_eq!(document.headings().count(), 1);
    }

    #[test]
    fn fences_close_only_on_a_matching_run() {
        let source = "````\n```\nx\n~~~\n- [ ] inside\n````\n- [ ] outside\n~~~ rust\n- [ ] fenced\n~~~\n```js\n- [ ] still fenced\n``` not a closer\n```\n- [ ] after\n";
        let document = Document::parse(source);
        assert_eq!(document.tasks().len(), 2);
        assert_eq!(document.tasks()[0].text, "outside");
        assert_eq!(document.tasks()[1].text, "after");
        let fenced = document
            .lines()
            .iter()
            .filter(|line| matches!(line.kind, LineKind::Fenced))
            .count();
        assert_eq!(fenced, 7);
    }

    #[test]
    fn fences_nested_under_a_list_item_still_fence_their_tasks() {
        for (source, label) in [
            (
                "- [ ] parent\n\t```\n\t- [ ] inside fence\n\t```\n- [ ] after\n",
                "tab",
            ),
            (
                "1. [ ] parent\n    ```\n    - [ ] inside fence\n    ```\n2. [ ] after\n",
                "four spaces",
            ),
            (
                "- [ ] parent\n  ~~~text\n  - [ ] inside fence\n  ~~~\n- [ ] after\n",
                "two spaces, tilde",
            ),
        ] {
            let document = Document::parse(source);
            assert_eq!(document.render(), source, "{label}");
            let texts: Vec<&str> = document
                .tasks()
                .iter()
                .map(|task| task.text.as_str())
                .collect();
            assert_eq!(texts, ["parent", "after"], "{label}");
            assert!(
                matches!(document.lines()[2].kind, LineKind::Fenced),
                "{label}: the task line inside the fence is fenced"
            );
        }
    }

    #[test]
    fn a_bullet_may_be_followed_by_up_to_four_spaces_or_a_tab() {
        let source = "-   [ ] three spaces [id:: a]\n-\t[x] tab [id:: b]\n-    [ ] four spaces\n-     [ ] five spaces\n* [ ] one space\n";
        let document = Document::parse(source);
        let texts: Vec<&str> = document
            .tasks()
            .iter()
            .map(|task| task.text.as_str())
            .collect();
        assert_eq!(texts, ["three spaces", "tab", "four spaces", "one space"]);
        assert_eq!(document.tasks()[0].id(), Some("a"));
        assert_eq!(document.tasks()[1].status, TaskStatus::Done);
        let line = &document.lines()[0].text;
        let field = &document.tasks()[0].fields[0];
        assert_eq!(&line[field.start..field.end], "[id:: a]");
        assert!(
            matches!(&document.lines()[3].kind, LineKind::ListItem(item) if item.text == "[ ] five spaces"),
            "five spaces after the bullet start an indented code block, not a task"
        );
    }

    #[test]
    fn a_byte_order_mark_does_not_hide_the_frontmatter() {
        let source = "\u{FEFF}---\r\ntype: story\r\n---\r\n# T\r\n";
        let document = Document::parse(source);
        assert_eq!(document.render(), source);
        assert_eq!(document.body_start(), 3);
        assert_eq!(
            document
                .frontmatter()
                .and_then(|fm| fm.scalar("type"))
                .as_deref(),
            Some("story")
        );
        assert!(document.lines()[0].text.starts_with('\u{FEFF}'));
    }

    #[test]
    fn heading_closers_follow_commonmark() {
        let titles: Vec<String> =
            Document::parse("# Learn C#\n## Issue #\n### Closed ##\n#### Trailing #  \n##### #\n")
                .headings()
                .map(|(_, heading)| heading.title.clone())
                .collect();
        assert_eq!(titles, ["Learn C#", "Issue", "Closed", "Trailing", ""]);
        assert_eq!(
            Document::parse("#NoSpace\n####### seven\n")
                .headings()
                .count(),
            0
        );
    }

    #[test]
    fn inline_fields_skip_code_spans_and_odd_spacing() {
        let text =
            "Check `[id:: fake]` thing  [id::task-9] [assigned_to::  Sam ] [Bad:: x] [k:: a [b]]";
        let fields = inline_fields(text);
        let pairs: Vec<(&str, &str)> = fields
            .iter()
            .map(|field| (field.key.as_str(), field.value.as_str()))
            .collect();
        assert_eq!(
            pairs,
            [("id", "task-9"), ("assigned_to", "Sam"), ("k", "a [b]")]
        );
        assert_eq!(&text[fields[0].start..fields[0].end], "[id::task-9]");
        assert_eq!(&text[fields[2].start..fields[2].end], "[k:: a [b]]");
    }

    #[test]
    fn code_spans_use_backtick_runs_and_a_stray_backtick_is_literal() {
        let double = inline_fields("``[id:: fake]`` [id:: real]");
        assert_eq!(double.len(), 1);
        assert_eq!(double[0].value, "real");
        let stray = inline_fields("`[id:: fake]` real [id:: r] stray ` [due:: d]");
        let values: Vec<&str> = stray.iter().map(|field| field.value.as_str()).collect();
        assert_eq!(values, ["r", "d"]);
        let mismatched = inline_fields("``code ` [id:: x]``");
        assert!(mismatched.is_empty());
        assert!(inline_fields("[").is_empty());
        assert!(inline_fields("[id::").is_empty());
    }

    #[test]
    fn wikilink_values_keep_their_brackets() {
        let fields = inline_fields("- [ ] Ask [waiting_on:: [[Sam Lee]]] [id:: t1]");
        let pairs: Vec<(&str, &str)> = fields
            .iter()
            .map(|field| (field.key.as_str(), field.value.as_str()))
            .collect();
        assert_eq!(pairs, [("waiting_on", "[[Sam Lee]]"), ("id", "t1")]);
    }

    #[test]
    fn task_field_offsets_index_the_whole_line() {
        let source =
            "\t- [ ] Ship it  [id:: t1] [due:: 2026-10-30]\n- 09:05 Logged [kind:: done]\n";
        let document = Document::parse(source);
        let task = &document.tasks()[0];
        let line = &document.lines()[0].text;
        assert_eq!(&line[task.fields[0].start..task.fields[0].end], "[id:: t1]");
        assert_eq!(
            &line[task.fields[1].start..task.fields[1].end],
            "[due:: 2026-10-30]"
        );
        assert_eq!(task.text, "Ship it");
        let (_, item) = document.list_items().next().expect("list item");
        assert_eq!(
            &document.lines()[1].text[item.fields[0].start..item.fields[0].end],
            "[kind:: done]"
        );
        assert_eq!(item.text, "09:05 Logged");
    }

    #[test]
    fn tasks_nest_by_indent_and_reset_at_paragraphs() {
        let source = "- [ ] parent [id:: task-1]\n\t- [ ] child [id:: task-2]\n\t\t- [/] grandchild\n\t- a note, not a task\n\t- [ ] second child\n- [-] sibling\nParagraph\n  - [ ] indented but new list\n";
        let document = Document::parse(source);
        let tasks = document.tasks();
        assert_eq!(
            tasks.iter().map(|task| task.depth).collect::<Vec<_>>(),
            [0, 1, 2, 1, 0, 0]
        );
        assert_eq!(tasks[1].parent, Some(0));
        assert_eq!(tasks[2].parent, Some(1));
        assert_eq!(tasks[3].parent, Some(0));
        assert_eq!(tasks[4].parent, None);
        assert_eq!(tasks[5].parent, None);
        assert_eq!(tasks[2].status, TaskStatus::InProgress);
        assert_eq!(tasks[4].status, TaskStatus::Cancelled);
        assert_eq!(document.subtasks(0).len(), 2);
    }

    #[test]
    fn quoted_tasks_do_not_nest_under_unquoted_ones() {
        let document = Document::parse("- [ ] a\n> quoted\n> - [ ] b\n>   - [ ] c\n- [ ] d\n");
        let parents: Vec<Option<usize>> = document.tasks().iter().map(|task| task.parent).collect();
        assert_eq!(parents, [None, None, Some(1), None]);
    }

    #[test]
    fn sections_follow_heading_levels() {
        let source = "intro\n# A\n## Next Steps\n- [ ] one\n### Detail\n- [ ] two\n## Pending Input\n- [ ] three\n# B\n- [ ] four\n";
        let document = Document::parse(source);
        let sections = document.sections();
        assert_eq!(sections.len(), 6);
        assert_eq!((sections[0].start, sections[0].end), (0, 1));
        assert_eq!(document.tasks_under("Next Steps").len(), 2);
        assert_eq!(document.tasks_under("Pending Input").len(), 1);
        assert_eq!(document.tasks_under("A").len(), 3);
        assert_eq!(document.tasks_under("Missing").len(), 0);
        assert_eq!(document.title(), Some("A"));
    }

    #[test]
    fn odd_task_shapes_and_thematic_breaks() {
        let document = Document::parse(
            "1. [X] numbered\n- [ ]\n- [?] custom\n-[ ] not a task\n- [ ]x not a task either\n* [ ] star  [due:: 2026-10-30]  tail\n* * *\n- - -\n- -\n",
        );
        let tasks = document.tasks();
        assert_eq!(tasks.len(), 4);
        assert_eq!(tasks[0].bullet, "1.");
        assert_eq!(tasks[0].status, TaskStatus::Done);
        assert_eq!(tasks[1].text, "");
        assert_eq!(tasks[2].status, TaskStatus::Other('?'));
        assert_eq!(tasks[3].text, "star tail");
        assert_eq!(tasks[3].field("due"), Some("2026-10-30"));
        let kinds: Vec<&LineKind> = document
            .lines()
            .iter()
            .skip(6)
            .map(|line| &line.kind)
            .collect();
        assert!(matches!(kinds[0], LineKind::Text));
        assert!(matches!(kinds[1], LineKind::Text));
        assert!(matches!(kinds[2], LineKind::ListItem(item) if item.text == "-"));
    }

    #[test]
    fn markers_need_their_closer() {
        let document = Document::parse("%% ai:note\n%% ai:begin\tfable %%\n%% ai:end %%\n");
        let kinds: Vec<&LineKind> = document.lines().iter().map(|line| &line.kind).collect();
        assert!(matches!(kinds[0], LineKind::Text));
        assert!(
            matches!(kinds[1], LineKind::Marker(marker) if marker.kind == MarkerKind::Begin && marker.attributes == "fable")
        );
        assert!(matches!(kinds[2], LineKind::Marker(marker) if marker.kind == MarkerKind::End));
    }
}
