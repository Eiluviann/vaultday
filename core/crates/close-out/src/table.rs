//! The copyable table of a close-out step: one per time sink, formatted by
//! that sink's feature set. No feature set formats one yet (there is no
//! ServiceNow feature set), so every table is plain TSV.
//!
//! The TSV is byte-exact and tested: cells joined by one tab, rows ended by
//! one `\n` (the header too, and the last row), no quoting; a tab, carriage
//! return or newline inside a cell becomes a space. Hours are decimal with
//! two places (`1.50`); a day with nothing is an empty cell.

/// A table of rows the user copies into a time sink, ticking each as entered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    /// The sink the table is for; ticks are kept per sink.
    pub sink: String,
    /// The table's title.
    pub title: String,
    /// The column headers.
    pub columns: Vec<String>,
    /// The rows.
    pub rows: Vec<TableRow>,
}

/// One row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableRow {
    /// What the row stands for (a story, a work item); ticks are kept by it.
    pub key: String,
    /// One cell per column.
    pub cells: Vec<String>,
    /// Whether the user ticked it as entered.
    pub is_ticked: bool,
}

/// Turns a table into the text a sink takes.
pub trait TableFormatter {
    /// The table as text.
    fn format(&self, table: &Table) -> String;
}

/// Plain tab-separated values, the format when no feature set formats a sink.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PlainTsv;

/// The sink id of a plain TSV table.
pub const PLAIN_SINK: &str = "plain";

impl TableFormatter for PlainTsv {
    fn format(&self, table: &Table) -> String {
        std::iter::once(&table.columns)
            .chain(table.rows.iter().map(|row| &row.cells))
            .map(|cells| {
                let mut line = cells
                    .iter()
                    .map(|cell| clean(cell))
                    .collect::<Vec<_>>()
                    .join("\t");
                line.push('\n');
                line
            })
            .collect()
    }
}

impl Table {
    /// The rows not ticked yet.
    #[must_use]
    pub fn open_rows(&self) -> usize {
        self.rows.iter().filter(|row| !row.is_ticked).count()
    }
}

/// Minutes as decimal hours with two places: 90 is `1.50`, 0 is `0.00`.
#[must_use]
pub fn hours(minutes: u64) -> String {
    let hundredths = (minutes * 100 + 30) / 60;
    format!("{}.{:02}", hundredths / 100, hundredths % 100)
}

/// [`hours`], or an empty cell for nothing.
#[must_use]
pub fn hours_or_blank(minutes: u64) -> String {
    if minutes == 0 {
        String::new()
    } else {
        hours(minutes)
    }
}

fn clean(cell: &str) -> String {
    cell.replace(['\t', '\r', '\n'], " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hours_round_to_the_hundredth() {
        for (minutes, text) in [
            (0, "0.00"),
            (1, "0.02"),
            (20, "0.33"),
            (40, "0.67"),
            (90, "1.50"),
            (390, "6.50"),
            (1950, "32.50"),
        ] {
            assert_eq!(hours(minutes), text, "{minutes}");
        }
        assert_eq!(hours_or_blank(0), "");
    }

    #[test]
    fn plain_tsv_is_byte_exact() {
        let table = Table {
            sink: PLAIN_SINK.to_owned(),
            title: "Time sheet".to_owned(),
            columns: vec!["Story".to_owned(), "Title".to_owned(), "Total".to_owned()],
            rows: vec![
                TableRow {
                    key: "WEB-142".to_owned(),
                    cells: vec![
                        "WEB-142".to_owned(),
                        "Checkout\tredesign\r\nv2".to_owned(),
                        "6.50".to_owned(),
                    ],
                    is_ticked: true,
                },
                TableRow {
                    key: "WEB-157".to_owned(),
                    cells: vec!["WEB-157".to_owned(), String::new(), "1.25".to_owned()],
                    is_ticked: false,
                },
            ],
        };
        assert_eq!(
            PlainTsv.format(&table),
            "Story\tTitle\tTotal\nWEB-142\tCheckout redesign  v2\t6.50\nWEB-157\t\t1.25\n"
        );
        assert_eq!(table.open_rows(), 1);
    }
}
