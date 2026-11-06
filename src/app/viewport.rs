//! Which part of the table is on screen.

use super::Cursor;
use crate::table::Table;

/// Scroll position plus the screen geometry it depends on.
/// `Default` = top-left corner, following the cursor.
#[derive(Default)]
pub struct Viewport {
    /// First visible line of the scrolling area (0 = first line after header).
    pub top: usize,
    /// Horizontal scroll offset of the scrolling area, in terminal columns.
    pub left: usize,
    /// Visible body lines (screen height minus header).
    pub page_height: usize,
    /// Width of the frozen first column (0 = nothing frozen).
    pub frozen: usize,
    /// Width of the horizontally scrolling part.
    pub scroll_width: usize,
    /// The user scrolled the view away from the cursor (H/L/Ctrl-e/Ctrl-y);
    /// the next cursor move brings it back.
    detached: bool,
}

impl Viewport {
    /// Recompute the geometry for a `width` x `height` body area, then bring
    /// the cursor into view (unless detached) and keep the view in bounds.
    /// Called before every draw, so it also handles terminal resizes.
    pub fn fit(&mut self, width: usize, height: usize, cursor: Cursor, table: &Table) {
        // Like pspg: freeze the first column only if it leaves enough room.
        let mut frozen = match table.columns.as_slice() {
            [first, _, ..] => first.end + 1, // + 1 = include its right border
            _ => 0,
        };
        if frozen * 2 > width {
            frozen = 0;
        }
        self.frozen = frozen;
        self.scroll_width = width - frozen;
        self.page_height = height.saturating_sub(table.header_rows).max(1);

        if !self.detached {
            self.scroll_to(cursor, table);
        }
        self.clamp(table);
    }

    /// Re-attach the view to the cursor.
    pub fn follow_cursor(&mut self) {
        self.detached = false;
    }

    /// Move only the view, not the cursor.
    pub fn scroll_by(&mut self, rows: isize, cols: isize) {
        self.detached = true;
        self.top = self.top.checked_add_signed(rows).unwrap_or(0);
        self.left = self.left.checked_add_signed(cols).unwrap_or(0);
        // Upper bounds are applied by `clamp` in the next `fit`.
    }

    /// Move the view by whole lines, together with the cursor (paging).
    pub fn page_by(&mut self, rows: isize, table: &Table) {
        self.top = self.top.checked_add_signed(rows).unwrap_or(0).min(self.max_top(table));
    }

    /// Show the last screen, including the footer below the data.
    pub fn scroll_to_end(&mut self, table: &Table) {
        self.top = self.max_top(table);
    }

    /// Largest useful `top`: the last screen shows the final line.
    fn max_top(&self, table: &Table) -> usize {
        table.body_lines().saturating_sub(self.page_height)
    }

    /// Adjust `top` and `left` so the cursor cell is on screen.
    fn scroll_to(&mut self, cursor: Cursor, table: &Table) {
        let Self { frozen, scroll_width, page_height, .. } = *self;

        // Vertical.
        if cursor.row < self.top {
            self.top = cursor.row;
        } else if cursor.row >= self.top + page_height {
            self.top = cursor.row + 1 - page_height;
        }

        // Horizontal: the frozen first column is always visible.
        if let Some(cell) = table.columns.get(cursor.col)
            && !(frozen > 0 && cursor.col == 0)
        {
            // + 1 to also show the cell's right border.
            let right = (cell.end + 1).min(table.max_width);
            if right > frozen + self.left + scroll_width {
                self.left = right - frozen - scroll_width;
            }
            if cell.start < frozen + self.left {
                // With a frozen column its border already separates the two
                // parts; without one, show the cell's left border too.
                self.left = if frozen > 0 {
                    cell.start - frozen
                } else {
                    cell.start.saturating_sub(1)
                };
            }
        }
    }

    /// Keep the view inside the content.
    fn clamp(&mut self, table: &Table) {
        self.top = self.top.min(self.max_top(table));
        let max_left = table.max_width.saturating_sub(self.frozen + self.scroll_width);
        self.left = self.left.min(max_left);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 100 rows of `| id | wide value |`, 30 columns wide.
    fn table() -> Table {
        let mut text = String::from("| id | value\n|----+-------------------------\n");
        for i in 0..100 {
            text.push_str(&format!("| {i:<2} | {:<23}\n", "x"));
        }
        Table::parse(&text)
    }

    fn at(row: usize, col: usize) -> Cursor {
        Cursor { row, col }
    }

    #[test]
    fn follows_cursor_down_and_up() {
        let t = table();
        let mut v = Viewport::default();
        v.fit(80, 12, at(50, 0), &t); // 12 - 2 header lines = 10 visible
        assert_eq!(v.page_height, 10);
        assert_eq!(v.top, 41); // row 50 is the last visible line
        v.fit(80, 12, at(5, 0), &t);
        assert_eq!(v.top, 5);
    }

    #[test]
    fn detached_view_stays_until_cursor_moves() {
        let t = table();
        let mut v = Viewport::default();
        v.fit(80, 12, at(0, 0), &t);
        v.scroll_by(30, 0);
        v.fit(80, 12, at(0, 0), &t);
        assert_eq!(v.top, 30);
        v.follow_cursor();
        v.fit(80, 12, at(0, 0), &t);
        assert_eq!(v.top, 0);
    }

    #[test]
    fn clamps_to_content() {
        let t = table();
        let mut v = Viewport::default();
        v.fit(80, 12, at(0, 0), &t);
        v.scroll_by(1000, 1000);
        v.fit(80, 12, at(0, 0), &t);
        assert_eq!(v.top, 90); // 100 rows - 10 visible
        assert_eq!(v.left, 0); // everything fits horizontally
    }

    #[test]
    fn frozen_column_and_horizontal_scroll() {
        let t = table();
        let mut v = Viewport::default();
        // `| id |` = 6 columns frozen; 16 left for the scrolling part.
        v.fit(22, 12, at(0, 1), &t);
        assert_eq!(v.frozen, 6);
        // The value cell (6..31) is wider than the 16 columns left: its
        // start wins, right after the frozen column.
        assert_eq!(v.left, 0);
        v.scroll_by(0, 4);
        v.fit(22, 12, at(0, 1), &t);
        assert_eq!(v.left, 4);
        v.scroll_by(0, 100);
        v.fit(22, 12, at(0, 1), &t);
        assert_eq!(v.left, 31 - 6 - 16); // clamped: last column at the right edge
        // Too narrow: nothing frozen.
        v.fit(10, 12, at(0, 0), &t);
        assert_eq!(v.frozen, 0);
    }
}
