//! Smart-case search over table cells.

use std::borrow::Cow;
use std::ops::Range;

use unicode_width::UnicodeWidthStr;

use super::Table;
use super::text::slice_cols;

/// A search result: cursor position, and whether we went past the end.
#[derive(Debug, PartialEq)]
pub struct Hit {
    pub row: usize,
    pub col: usize,
    pub wrapped: bool,
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

impl Table {
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

        // Cheap pre-filter: a cell can only match if its whole line does,
        // so most rows are skipped without cutting them into cells.
        // (row, line matches) of the last line checked.
        let mut checked: Option<(usize, bool)> = None;

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
            let line_matches = match checked {
                Some((cr, m)) if cr == r => m,
                _ => {
                    let m = matcher.is_match(&self.lines[self.header_rows + r]);
                    checked = Some((r, m));
                    m
                }
            };
            if !line_matches {
                continue;
            }
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table::tests::PGCLI;

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
        // a match across a border is not a cell match
        assert_eq!(t.find(0, 0, "1  | a", true), None);
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
}
