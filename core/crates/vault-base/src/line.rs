//! Line splitting that keeps every byte of the source: the content of a line
//! and its terminator are stored apart so a document renders back verbatim.

/// The terminator that followed a line in the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LineEnding {
    /// The last line of a file that does not end with a newline.
    None,
    /// `\n`.
    Lf,
    /// `\r\n`.
    CrLf,
}

impl LineEnding {
    /// The bytes this terminator contributes when the line is rendered.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            LineEnding::None => "",
            LineEnding::Lf => "\n",
            LineEnding::CrLf => "\r\n",
        }
    }
}

/// Splits `source` into `(content, terminator)` pairs whose concatenation is
/// the source again. An empty source has no lines; a trailing newline does not
/// produce an extra empty line.
pub(crate) fn split_lines(source: &str) -> Vec<(String, LineEnding)> {
    let mut lines = Vec::new();
    let mut rest = source;
    while !rest.is_empty() {
        match rest.find('\n') {
            Some(newline_at) => {
                let before_newline = &rest[..newline_at];
                let (content, ending) = match before_newline.strip_suffix('\r') {
                    Some(content) => (content, LineEnding::CrLf),
                    None => (before_newline, LineEnding::Lf),
                };
                lines.push((content.to_owned(), ending));
                rest = &rest[newline_at + 1..];
            }
            None => {
                lines.push((rest.to_owned(), LineEnding::None));
                rest = "";
            }
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::{LineEnding, split_lines};

    fn render(lines: &[(String, LineEnding)]) -> String {
        lines
            .iter()
            .map(|(content, ending)| format!("{content}{}", ending.as_str()))
            .collect()
    }

    #[test]
    fn splits_and_rejoins_every_ending_shape() {
        for source in [
            "",
            "a",
            "a\n",
            "a\n\n",
            "a\r\nb",
            "a\r\nb\r\n",
            "\n",
            "\r\n",
            "a\rb\n",
        ] {
            let lines = split_lines(source);
            assert_eq!(render(&lines), source, "source {source:?}");
        }
    }

    #[test]
    fn a_trailing_newline_does_not_add_a_line() {
        assert_eq!(split_lines("a\n").len(), 1);
        assert_eq!(split_lines("a\n\n").len(), 2);
        assert_eq!(split_lines("a").len(), 1);
        assert!(split_lines("").is_empty());
    }

    #[test]
    fn a_lone_carriage_return_stays_in_the_content() {
        let lines = split_lines("a\rb\n");
        assert_eq!(lines, vec![("a\rb".to_owned(), LineEnding::Lf)]);
    }
}
