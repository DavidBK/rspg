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

mod parse;
mod search;
mod text;

use std::ops::Range;

pub use parse::is_border_line;
pub use search::Matcher;
pub use text::slice_cols;

use text::cell_text;

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

impl Table {
    /// Cell mode: the input is a table, so the cursor moves cell by cell.
    /// Otherwise (plain text) the cursor is just a line.
    pub fn has_cells(&self) -> bool {
        !self.columns.is_empty()
    }

    pub fn data_rows(&self) -> usize {
        self.data_end - self.header_rows
    }

    /// Lines below the header (data rows and footer).
    pub fn body_lines(&self) -> usize {
        self.lines.len() - self.header_rows
    }

    /// Text of one cell, trimmed. `row` is a data-row index (0 = first data row).
    pub fn cell(&self, row: usize, col: usize) -> Option<String> {
        let line = self.lines.get(self.header_rows + row)?;
        if !self.has_cells() {
            return Some(line.trim().to_string());
        }
        Some(cell_text(line, self.columns.get(col)?))
    }

    /// A whole data row as tab-separated cell values (pastes into
    /// spreadsheet columns). Plain text: the trimmed line.
    pub fn row(&self, row: usize) -> Option<String> {
        let line = self.lines.get(self.header_rows + row)?;
        if !self.has_cells() {
            return Some(line.trim().to_string());
        }
        let cells: Vec<String> = self.columns.iter().map(|r| cell_text(line, r)).collect();
        Some(cells.join("\t"))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    pub const PSQL: &str = " id | name\n----+-------\n  1 | alice\n  2 | bob\n(2 rows)\n";

    pub const PGCLI: &str = "\
+----+-------+
| id | name  |
|----+-------|
| 1  | alice |
| 2  | bob   |
+----+-------+";

    pub const UNICODE: &str = " id │ name\n────┼──────\n  1 │ żółw\n(1 row)";
}
