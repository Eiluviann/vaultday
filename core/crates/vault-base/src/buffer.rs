//! A mutable copy of a note's lines for the writer. Every replacement and
//! insertion is tracked so the writer can report exactly which lines changed,
//! and untouched lines keep their bytes and terminators.

use crate::line::{LineEnding, split_lines};
use crate::scan::{Document, Section};

/// A line the writer replaced or inserted, by index into the buffer as it
/// stands now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TrackedChange {
    pub(crate) index: usize,
    pub(crate) old_text: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct LineBuffer {
    lines: Vec<(String, LineEnding)>,
    ending: LineEnding,
    changes: Vec<TrackedChange>,
}

impl LineBuffer {
    pub(crate) fn parse(source: &str) -> LineBuffer {
        let lines = split_lines(source);
        let ending = lines
            .iter()
            .map(|(_, ending)| *ending)
            .find(|ending| *ending != LineEnding::None)
            .unwrap_or(LineEnding::Lf);
        LineBuffer {
            lines,
            ending,
            changes: Vec::new(),
        }
    }

    pub(crate) fn render(&self) -> String {
        self.lines
            .iter()
            .map(|(text, ending)| format!("{text}{}", ending.as_str()))
            .collect()
    }

    pub(crate) fn document(&self) -> Document {
        Document::parse(&self.render())
    }

    pub(crate) fn len(&self) -> usize {
        self.lines.len()
    }

    pub(crate) fn text(&self, index: usize) -> Option<&str> {
        self.lines.get(index).map(|(text, _)| text.as_str())
    }

    pub(crate) fn is_blank(&self, index: usize) -> bool {
        self.text(index).is_some_and(|text| text.trim().is_empty())
    }

    pub(crate) fn changes(&self) -> &[TrackedChange] {
        &self.changes
    }

    /// Replaces one line's content, keeping its terminator.
    pub(crate) fn replace(&mut self, index: usize, text: String) {
        let Some((current, _)) = self.lines.get_mut(index) else {
            return;
        };
        if *current == text {
            return;
        }
        let old_text = std::mem::replace(current, text);
        self.changes.push(TrackedChange {
            index,
            old_text: Some(old_text),
        });
    }

    /// Inserts a tracked line before `at` (`len()` appends).
    pub(crate) fn insert(&mut self, at: usize, text: String) {
        self.insert_line(at, text);
        for change in &mut self.changes {
            if change.index >= at {
                change.index += 1;
            }
        }
        self.changes.push(TrackedChange {
            index: at,
            old_text: None,
        });
    }

    /// Inserts a blank spacer line that is not reported as a change.
    pub(crate) fn insert_spacer(&mut self, at: usize) {
        self.insert_line(at, String::new());
        for change in &mut self.changes {
            if change.index >= at {
                change.index += 1;
            }
        }
    }

    fn insert_line(&mut self, at: usize, text: String) {
        let at = at.min(self.lines.len());
        if at == self.lines.len()
            && let Some((_, last_ending)) = self.lines.last_mut()
            && *last_ending == LineEnding::None
        {
            *last_ending = self.ending;
        }
        self.lines.insert(at, (text, self.ending));
    }
}

/// The index just after the last non-blank line of `section`'s body, or just
/// after its heading when the body is blank. This is where the next list item
/// of the section goes.
pub(crate) fn end_of_section_content(buffer: &LineBuffer, section: &Section) -> usize {
    (section.start..section.end)
        .rev()
        .find(|index| !buffer.is_blank(*index))
        .map_or(section.start, |last_content| last_content + 1)
}

/// Moves an insertion point out of a model-text block. A line inserted
/// between `%% ai:begin %%` and `%% ai:end %%` would read as model text, so
/// the point moves past the end marker. `None` when the point lies in a
/// note-level `ai:note` unit, which has no end to move past.
pub(crate) fn outside_model_text(document: &Document, at: usize) -> Option<usize> {
    let mut at = at;
    loop {
        let Some(unit) = document
            .provenance_units()
            .into_iter()
            .find(|unit| unit.body_start <= at && at <= unit.body_end)
        else {
            return Some(at);
        };
        at = unit.end_line? + 1;
    }
}

/// Splits a `{indent}{bullet}{gap}[c]{rest}` task line and returns the byte
/// range of the status character `c`.
pub(crate) fn status_char_range(
    line: &str,
    indent_len: usize,
    bullet_len: usize,
) -> Option<(usize, usize)> {
    let after_bullet = line.get(indent_len + bullet_len..)?;
    let open_at = after_bullet.find('[')?;
    let status = after_bullet.get(open_at + 1..)?.chars().next()?;
    let start = indent_len + bullet_len + open_at + 1;
    let end = start + status.len_utf8();
    (line.get(end..)?.starts_with(']')).then_some((start, end))
}

#[cfg(test)]
mod tests {
    use super::{LineBuffer, outside_model_text, status_char_range};
    use crate::scan::Document;

    #[test]
    fn tracked_changes_follow_later_inserts() {
        let mut buffer = LineBuffer::parse("a\nb\nc\n");
        buffer.replace(2, "C".to_owned());
        buffer.insert(0, "first".to_owned());
        assert_eq!(buffer.render(), "first\na\nb\nC\n");
        let indices: Vec<usize> = buffer.changes().iter().map(|change| change.index).collect();
        assert_eq!(indices, [3, 0]);
    }

    #[test]
    fn appending_after_an_unterminated_last_line_terminates_it() {
        let mut buffer = LineBuffer::parse("a\r\nb");
        buffer.insert(2, "c".to_owned());
        assert_eq!(buffer.render(), "a\r\nb\r\nc\r\n");
    }

    #[test]
    fn model_text_blocks_push_the_insertion_point_past_the_end_marker() {
        let document = Document::parse("x\n%% ai:begin a %%\nbody\n%% ai:end %%\ny\n");
        assert_eq!(outside_model_text(&document, 2), Some(4));
        assert_eq!(outside_model_text(&document, 1), Some(1));
        assert_eq!(outside_model_text(&document, 4), Some(4));
        let note_level = Document::parse("x\n%% ai:note a %%\nbody\n");
        assert_eq!(outside_model_text(&note_level, 3), None);
    }

    #[test]
    fn finds_the_status_character_even_when_multibyte() {
        let line = "\t- [é] text";
        assert_eq!(status_char_range(line, 1, 1), Some((4, 6)));
        assert_eq!(status_char_range("- [ ] text", 0, 1), Some((3, 4)));
        assert_eq!(status_char_range("- text", 0, 1), None);
    }
}
