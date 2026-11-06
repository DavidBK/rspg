//! Detecting the table layout: header separator, column ranges, footer.

use std::ops::Range;

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::Table;
use super::text::cell_text;

/// How many lines from the top we search for a header separator.
const HEADER_SEARCH_LINES: usize = 10;

impl Table {
    pub fn parse(text: &str) -> Self {
        let lines: Vec<String> = text.lines().map(String::from).collect();
        let max_width = lines.iter().map(|l| l.width()).max().unwrap_or(0);

        // `let ... else`: plain text (no header found) gets a trivial layout.
        let Some(sep) = find_header_separator(&lines) else {
            return Self {
                data_end: lines.len(),
                lines,
                header_rows: 0,
                columns: Vec::new(),
                column_names: Vec::new(),
                max_width,
            };
        };

        let header_rows = sep + 1;
        let columns = column_ranges(&lines[sep]);

        // Column names come from the last non-border line above the separator.
        let column_names = lines[..sep]
            .iter()
            .rev()
            .find(|l| !is_border_line(l))
            .map(|l| columns.iter().map(|r| cell_text(l, r)).collect())
            .unwrap_or_default();

        // Data stops at the first border line, empty line or `(N rows)` footer.
        let data_end = lines[header_rows..]
            .iter()
            .position(|l| is_border_line(l) || l.trim().is_empty() || is_footer(l))
            .map_or(lines.len(), |i| header_rows + i);

        Self {
            lines,
            header_rows,
            data_end,
            columns,
            column_names,
            max_width,
        }
    }
}

/// The header separator is the first border line that comes *after* some
/// content (a border line before any content is the top frame `+---+`).
fn find_header_separator(lines: &[String]) -> Option<usize> {
    let mut seen_content = false;
    for (i, line) in lines.iter().enumerate().take(HEADER_SEARCH_LINES) {
        if is_border_line(line) {
            if seen_content {
                return Some(i);
            }
        } else if !line.trim().is_empty() {
            seen_content = true;
        }
    }
    None
}

fn is_horizontal(c: char) -> bool {
    matches!(c, '-' | '=' | '─' | '━' | '═')
}

/// Box-drawing block U+2500..U+257F covers ─ │ ┼ ╪ ┌ ┐ etc.
fn is_box_char(c: char) -> bool {
    ('\u{2500}'..='\u{257F}').contains(&c)
}

/// A line made only of border characters, mostly horizontal ones.
pub fn is_border_line(line: &str) -> bool {
    let line = line.trim_end();
    let mut total = 0;
    let mut horizontal = 0;
    for c in line.chars() {
        if !(matches!(c, '+' | '|' | ':' | ' ') || is_horizontal(c) || is_box_char(c)) {
            return false;
        }
        total += 1;
        if is_horizontal(c) {
            horizontal += 1;
        }
    }
    total > 0 && horizontal * 2 > total
}

/// `(1 row)` / `(42 rows)`
fn is_footer(line: &str) -> bool {
    let line = line.trim();
    line.starts_with('(') && (line.ends_with(" row)") || line.ends_with(" rows)"))
}

/// Split a header separator into column ranges: every non-horizontal char
/// (`+`, `|`, `┼`, `╪`, ...) is a boundary between columns.
fn column_ranges(sep: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut start = 0;
    let mut col = 0;
    for c in sep.trim_end().chars() {
        let w = c.width().unwrap_or(0);
        if !is_horizontal(c) && c != ' ' {
            if col > start {
                ranges.push(start..col);
            }
            start = col + w;
        }
        col += w;
    }
    if col > start {
        ranges.push(start..col);
    }
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table::tests::{PGCLI, PSQL, UNICODE};

    #[test]
    fn parses_psql() {
        let t = Table::parse(PSQL);
        assert_eq!(t.header_rows, 2);
        assert_eq!(t.columns, vec![0..4, 5..12]);
        assert_eq!(t.column_names, vec!["id", "name"]);
        assert_eq!(t.data_rows(), 2);
        assert_eq!(t.cell(1, 1).as_deref(), Some("bob"));
        assert_eq!(t.row(1).as_deref(), Some("2\tbob"));
    }

    #[test]
    fn parses_pgcli() {
        let t = Table::parse(PGCLI);
        assert_eq!(t.header_rows, 3);
        assert_eq!(t.columns, vec![1..5, 6..13]);
        assert_eq!(t.column_names, vec!["id", "name"]);
        assert_eq!(t.data_rows(), 2);
        assert_eq!(t.cell(0, 0).as_deref(), Some("1"));
        assert_eq!(t.cell(0, 1).as_deref(), Some("alice"));
    }

    #[test]
    fn parses_unicode_borders() {
        let t = Table::parse(UNICODE);
        assert_eq!(t.header_rows, 2);
        assert_eq!(t.cell(0, 1).as_deref(), Some("żółw"));
    }

    #[test]
    fn plain_text_is_not_a_table() {
        let t = Table::parse("hello\nworld");
        assert_eq!(t.header_rows, 0);
        assert!(t.columns.is_empty());
        assert_eq!(t.cell(1, 0).as_deref(), Some("world"));
    }
}
