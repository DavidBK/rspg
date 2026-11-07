//! Keys in the table view: cursor movement and actions on cells.

use super::effect::Effect;
use super::popup::Popup;
use super::{App, Flash, Mode};
use crate::cell;
use crate::keys::TableAction;

/// How many terminal columns H / L scroll the view.
const VIEW_STEP: isize = 4;

impl App {
    pub(super) fn on_table(&mut self, action: TableAction) -> Option<Effect> {
        let page = self.viewport.page_height.max(1) as isize;
        let half = (page / 2).max(1);

        match action {
            TableAction::Quit => self.quit = true,
            // Esc first clears search highlights, then quits.
            TableAction::Cancel if self.search.highlight => self.search.highlight = false,
            TableAction::Cancel => self.quit = true,

            // View only.
            TableAction::ScrollDown => self.viewport.scroll_by(1, 0),
            TableAction::ScrollUp => self.viewport.scroll_by(-1, 0),
            TableAction::ScrollLeft => self.viewport.scroll_by(0, -VIEW_STEP),
            TableAction::ScrollRight => self.viewport.scroll_by(0, VIEW_STEP),

            // Cursor.
            TableAction::Down => self.move_row(1),
            TableAction::Up => self.move_row(-1),
            TableAction::Right => self.move_col(1),
            TableAction::Left => self.move_col(-1),
            TableAction::HalfPageDown => self.move_page(half),
            TableAction::HalfPageUp => self.move_page(-half),
            TableAction::PageDown => self.move_page(page),
            TableAction::PageUp => self.move_page(-page),
            TableAction::Top => self.set_row(0),
            TableAction::Bottom => {
                self.set_row(self.table.data_rows().saturating_sub(1));
                self.viewport.scroll_to_end(&self.table); // also reveals the footer
            }
            TableAction::FirstCol => self.set_col(0),
            TableAction::LastCol => self.set_col(self.table.columns.len().saturating_sub(1)),

            // Current cell / row.
            TableAction::CopyCell => return self.copy_cell(),
            TableAction::CopyRow => return self.copy_row(),
            TableAction::OpenPopup => {
                if let Some(popup) = Popup::open(&self.table, self.cursor) {
                    self.mode = Mode::Detail(popup);
                }
            }
            TableAction::EditCell => {
                let value = self.table.cell(self.cursor.row, self.cursor.col)?;
                let f = cell::format(&value);
                return Some(Effect::edit(f.text, f.is_json, None));
            }
            TableAction::EditAll => {
                let text = self.table.lines.join("\n");
                let line = self.table.header_rows + self.cursor.row + 1;
                return Some(Effect::edit(text, false, Some(line)));
            }

            // Search.
            TableAction::SearchForward => self.start_search(true),
            TableAction::SearchBackward => self.start_search(false),
            TableAction::NextMatch => self.search_next(false),
            TableAction::PrevMatch => self.search_next(true),
            TableAction::SearchCellForward => self.search_cell(true),
            TableAction::SearchCellBackward => self.search_cell(false),
        }
        None
    }

    fn copy_cell(&self) -> Option<Effect> {
        let row = self.cursor.row;
        let text = self.table.cell(row, self.cursor.col)?;
        // Plain text has no cells: flash the whole line.
        let col = self.table.has_cells().then_some(self.cursor.col);
        Some(Effect::Copy { label: format!("cell: {text}"), text, flash: Some(Flash::new(row, col)) })
    }

    fn copy_row(&self) -> Option<Effect> {
        let row = self.cursor.row;
        let text = self.table.row(row)?;
        let cells = self.table.columns.len().max(1);
        let label = format!("row {} ({cells} cells)", row + 1);
        Some(Effect::Copy { text, label, flash: Some(Flash::new(row, None)) })
    }

    fn move_row(&mut self, delta: isize) {
        self.set_row(self.cursor.row.checked_add_signed(delta).unwrap_or(0));
    }

    fn move_page(&mut self, delta: isize) {
        // Move the view and the cursor together, like less/pspg.
        self.viewport.page_by(delta, &self.table);
        self.move_row(delta);
    }

    fn move_col(&mut self, delta: isize) {
        if self.table.has_cells() {
            let last = self.table.columns.len() - 1;
            self.set_col(self.cursor.col.checked_add_signed(delta).unwrap_or(0).min(last));
        } else {
            // Plain text has no cells: h/l scroll sideways instead.
            self.viewport.scroll_by(0, delta * VIEW_STEP);
        }
    }

    fn set_row(&mut self, row: usize) {
        self.cursor.row = row.min(self.table.data_rows().saturating_sub(1));
        self.viewport.follow_cursor();
    }

    pub(super) fn set_col(&mut self, col: usize) {
        self.cursor.col = col;
        self.viewport.follow_cursor();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table::Table;

    fn app() -> App {
        App::new(Table::parse(" id | name\n----+------\n  1 | a\n  2 | b\n  3 | c\n(3 rows)"))
    }

    #[test]
    fn cursor_stays_in_table() {
        let mut a = app();
        a.on_table(TableAction::Up);
        a.on_table(TableAction::Left);
        assert_eq!((a.cursor.row, a.cursor.col), (0, 0));
        a.on_table(TableAction::Bottom);
        a.on_table(TableAction::Down);
        a.on_table(TableAction::LastCol);
        a.on_table(TableAction::Right);
        assert_eq!((a.cursor.row, a.cursor.col), (2, 1));
    }

    #[test]
    fn copy_returns_an_effect() {
        let mut a = app();
        a.on_table(TableAction::Down);
        a.on_table(TableAction::Right);
        let Some(Effect::Copy { text, flash, .. }) = a.on_table(TableAction::CopyCell) else {
            panic!("expected a copy");
        };
        assert_eq!(text, "b");
        assert!(flash.is_some_and(|f| f.row == 1 && f.col == Some(1)));
        let Some(Effect::Copy { text, .. }) = a.on_table(TableAction::CopyRow) else {
            panic!("expected a copy");
        };
        assert_eq!(text, "2\tb");
    }

    #[test]
    fn esc_clears_highlight_then_quits() {
        let mut a = app();
        a.on_table(TableAction::SearchCellForward);
        assert!(a.search.highlight);
        a.on_table(TableAction::Cancel);
        assert!(!a.search.highlight && !a.quit);
        a.on_table(TableAction::Cancel);
        assert!(a.quit);
    }
}
