//! Understanding the shape of psql / pgcli table output.
//!
//! psql:                      pgcli (table_format = psql):
//!
//!   id | name                  +----+-------+
//!  ----+-------                | id | name  |
//!   1  | alice                 |----+-------|
//!  (1 row)                     | 1  | alice |
//!                              +----+-------+
//!
//! Like pspg, we find the *header separator* (`----+----` / `|----+----|`):
//! every line up to and including it is frozen at the top, and the positions
//! of its `+` junctions tell us where each column starts and ends.

use std::borrow::Cow;
use std::ops::Range;

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// How many lines from the top we search for a header separator.
const HEADER_SEARCH_LINES: usize = 10;

pub struct Table {
    pub lines: Vec<String>,
    /// Lines `0..header_rows` are the header (frozen at the top).
    pub header_rows: usize,
    /// Lines `header_rows..data_end` are data rows the cursor can visit.
    /// Anything after (bottom border, `(3 rows)`) just scrolls along.
    pub data_end: usize,
    /// Display-column range of each cell's content (borders excluded).
    /// Empty when the input is not a table.
    pub columns: Vec<Range<usize>>,
    pub column_names: Vec<String>,
    /// Width of the widest line, in terminal columns.
    pub max_width: usize,
}

/// A search result: cursor position, and whether we went past the end.
#[derive(Debug, PartialEq)]
pub struct Hit {
    pub row: usize,
    pub col: usize,
    pub wrapped: bool,
}

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

    pub fn data_rows(&self) -> usize {
        self.data_end - self.header_rows
    }

    /// Text of one cell, trimmed. `row` is a data-row index (0 = first data row).
    pub fn cell(&self, row: usize, col: usize) -> Option<String> {
        let line = self.lines.get(self.header_rows + row)?;
        if self.columns.is_empty() {
            return Some(line.trim().to_string());
        }
        Some(cell_text(line, self.columns.get(col)?))
    }

    /// Find the next cell containing `query`, starting after (`row`, `col`)
    /// and wrapping around — like vim's `/` and `?`.
    pub fn find(&self, row: usize, col: usize, query: &str, forward: bool) -> Option<Hit> {
        // Plain text counts as one column, so the same loop works for both.
        let ncols = self.columns.len().max(1);
        let total = self.data_rows() * ncols;
        // `?` on an Option: return None early if the query is empty.
        let matcher = Matcher::new(query)?;
        if total == 0 {
            return None;
        }

        // Number the cells 0..total in reading order and walk around that
        // circle; `step == total` comes back to the start cell itself.
        let start = (row * ncols + col).min(total - 1);
        for step in 1..=total {
            let idx = if forward {
                (start + step) % total
            } else {
                (start + total - step) % total
            };
            let (r, c) = (idx / ncols, idx % ncols);
            let Some(cell) = self.cell(r, c) else { continue };
            if matcher.is_match(&cell) {
                let wrapped = if forward { idx <= start } else { idx >= start };
                return Some(Hit { row: r, col: c, wrapped });
            }
        }
        None
    }

    /// Screen-column ranges of every match in one line, searched cell by
    /// cell (so a match never spans a border). Used to highlight matches.
    pub fn match_ranges(&self, line: &str, matcher: &Matcher) -> Vec<Range<usize>> {
        if self.columns.is_empty() {
            return matcher.find_all(line);
        }
        self.columns
            .iter()
            .flat_map(|cell| {
                let text = slice_cols(line, cell.start, cell.len());
                // Shift cell-relative ranges to line positions.
                matcher.find_all(&text).into_iter().map(move |r| r.start + cell.start..r.end + cell.start)
            })
            .collect()
    }

    /// A whole data row as tab-separated cell values (pastes into
    /// spreadsheet columns). Plain text: the trimmed line.
    pub fn row(&self, row: usize) -> Option<String> {
        let line = self.lines.get(self.header_rows + row)?;
        if self.columns.is_empty() {
            return Some(line.trim().to_string());
        }
        let cells: Vec<String> = self.columns.iter().map(|r| cell_text(line, r)).collect();
        Some(cells.join("\t"))
    }
}

/// Smart-case substring matching, like vim's `smartcase`: an all-lowercase
/// query ignores case, any uppercase letter makes it case-sensitive.
pub struct Matcher {
    needle: String,
    ignore_case: bool,
}

impl Matcher {
    /// `None` for an empty query (nothing to search for).
    pub fn new(query: &str) -> Option<Self> {
        if query.is_empty() {
            return None;
        }
        let ignore_case = !query.chars().any(char::is_uppercase);
        let needle = if ignore_case { query.to_lowercase() } else { query.to_string() };
        Some(Self { needle, ignore_case })
    }

    /// `Cow` = "clone on write": borrow `text` as-is when case matters,
    /// only allocate a lowercased copy when it doesn't.
    fn haystack<'a>(&self, text: &'a str) -> Cow<'a, str> {
        if self.ignore_case {
            Cow::Owned(text.to_lowercase())
        } else {
            Cow::Borrowed(text)
        }
    }

    pub fn is_match(&self, text: &str) -> bool {
        self.haystack(text).contains(&self.needle)
    }

    /// Screen-column ranges of every match in `text`.
    /// (`match_indices` gives byte offsets; we convert them to columns.)
    pub fn find_all(&self, text: &str) -> Vec<Range<usize>> {
        let hay = self.haystack(text);
        let width = self.needle.width();
        hay.match_indices(self.needle.as_str())
            .map(|(byte, _)| {
                let start = hay[..byte].width();
                start..start + width
            })
            .collect()
    }
}

fn cell_text(line: &str, cols: &Range<usize>) -> String {
    slice_cols(line, cols.start, cols.len()).trim().to_string()
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
fn is_border_line(line: &str) -> bool {
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

/// Cut `width` terminal columns out of `s`, starting at column `start`.
/// The result is always exactly `width` columns wide (padded with spaces),
/// and a double-width char cut in half is replaced by a space.
pub fn slice_cols(s: &str, start: usize, width: usize) -> String {
    let end = start + width;
    let mut out = String::with_capacity(width);
    let mut col = 0;
    let mut used = 0;
    for c in s.chars() {
        if col >= end {
            break;
        }
        let w = c.width().unwrap_or(0);
        let next = col + w;
        if col >= start && next <= end {
            out.push(c);
            used += w;
        } else if next > start {
            // Wide char straddling an edge: fill its visible part with spaces.
            let visible = next.min(end) - col.max(start);
            out.extend(std::iter::repeat_n(' ', visible));
            used += visible;
        }
        col = next;
    }
    out.extend(std::iter::repeat_n(' ', width - used));
    out
}

// Unit tests live next to the code. Run them with `cargo test`.
#[cfg(test)]
mod tests {
    use super::*;

    const PSQL: &str = " id | name\n----+-------\n  1 | alice\n  2 | bob\n(2 rows)\n";

    const PGCLI: &str = "\
+----+-------+
| id | name  |
|----+-------|
| 1  | alice |
| 2  | bob   |
+----+-------+";

    const UNICODE: &str = " id │ name\n────┼──────\n  1 │ żółw\n(1 row)";

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
    fn finds_cells() {
        let t = Table::parse(PGCLI);
        let hit = |row, col, wrapped| Some(Hit { row, col, wrapped });
        // forward from the first cell
        assert_eq!(t.find(0, 0, "bob", true), hit(1, 1, false));
        // smart-case: uppercase makes it case-sensitive
        assert_eq!(t.find(0, 0, "Bob", true), None);
        assert_eq!(t.find(0, 0, "BO", true), None);
        // wraps around the end...
        assert_eq!(t.find(1, 1, "alice", true), hit(0, 1, true));
        // ...and the start when searching backwards
        assert_eq!(t.find(0, 0, "bob", false), hit(1, 1, true));
        // the only match is the current cell: found again after a full wrap
        assert_eq!(t.find(0, 1, "alice", true), hit(0, 1, true));
        // plain text searches lines
        let p = Table::parse("one\ntwo\nthree");
        assert_eq!(p.find(0, 0, "t", true), hit(1, 0, false));
    }

    #[test]
    fn matcher_finds_screen_columns() {
        let m = Matcher::new("ab").unwrap();
        assert_eq!(m.find_all("xAbyab"), vec![1..3, 4..6]); // smart-case
        assert_eq!(m.find_all("日ab"), vec![2..4]); // 日 is 2 columns wide
        assert_eq!(Matcher::new("Ab").unwrap().find_all("ab Ab"), vec![3..5]);
        assert!(Matcher::new("").is_none());

        // Per cell, shifted to line positions (pgcli: name is columns 6..13).
        let t = Table::parse(PGCLI);
        let line = &t.lines[t.header_rows + 1]; // "| 2  | bob   |"
        assert_eq!(t.match_ranges(line, &Matcher::new("b").unwrap()), vec![7..8, 9..10]);
    }

    #[test]
    fn plain_text_is_not_a_table() {
        let t = Table::parse("hello\nworld");
        assert_eq!(t.header_rows, 0);
        assert!(t.columns.is_empty());
        assert_eq!(t.cell(1, 0).as_deref(), Some("world"));
    }

    #[test]
    fn slices_wide_chars() {
        assert_eq!(slice_cols("ab", 0, 4), "ab  ");
        assert_eq!(slice_cols("a日b", 1, 2), "日");
        assert_eq!(slice_cols("a日b", 2, 2), " b"); // right half of 日
        assert_eq!(slice_cols("a日b", 0, 2), "a "); // left half of 日
    }
}
