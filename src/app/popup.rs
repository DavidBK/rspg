//! The cell detail popup (`f` / Enter).

use ratatui::layout::Rect;

use super::effect::Effect;
use super::{App, Cursor, Mode};
use crate::cell::{self, Formatted};
use crate::keys::PopupAction;
use crate::table::Table;
use crate::ui;

pub struct Popup {
    pub title: String,
    pub content: Formatted,
    pub scroll: usize,
    /// Lines that fit inside the popup / total wrapped lines (set by `fit`).
    page_height: usize,
    line_count: usize,
}

impl Popup {
    pub fn open(table: &Table, cursor: Cursor) -> Option<Self> {
        let Cursor { row, col } = cursor;
        let value = table.cell(row, col)?;
        let mut title = table.column_names.get(col).map_or("line", String::as_str).to_string();
        // Also name the row by its first column (usually an id), unless
        // that's the cell we're already showing.
        if col != 0
            && let Some(key) = table.column_names.first()
            && let Some(key_value) = table.cell(row, 0)
        {
            title.push_str(&format!(" · {key}={key_value}"));
        }
        Some(Self { title, content: cell::format(&value), scroll: 0, page_height: 0, line_count: 0 })
    }

    /// Measure the wrapped text for the popup `area` (before each draw).
    pub fn fit(&mut self, area: Rect) {
        self.line_count = ui::popup::content_height(self, area.width);
        self.page_height = area.height.saturating_sub(2) as usize; // minus borders
        self.scroll = self.scroll.min(self.max_scroll()); // e.g. after a resize
    }

    pub fn max_scroll(&self) -> usize {
        self.line_count.saturating_sub(self.page_height)
    }

    fn scroll_by(&mut self, n: isize) {
        self.scroll = self.scroll.checked_add_signed(n).unwrap_or(0).min(self.max_scroll());
    }
}

impl App {
    pub(super) fn on_popup(&mut self, action: PopupAction) -> Option<Effect> {
        let Mode::Detail(popup) = &mut self.mode else { return None };
        let page = popup.page_height.max(1) as isize;
        let half = (page / 2).max(1);

        match action {
            PopupAction::Close => self.mode = Mode::Table,
            PopupAction::HalfPageDown => popup.scroll_by(half),
            PopupAction::HalfPageUp => popup.scroll_by(-half),
            PopupAction::Down => popup.scroll_by(1),
            PopupAction::Up => popup.scroll_by(-1),
            PopupAction::PageDown => popup.scroll_by(page),
            PopupAction::PageUp => popup.scroll_by(-page),
            PopupAction::Top => popup.scroll = 0,
            PopupAction::Bottom => popup.scroll = popup.max_scroll(),
            PopupAction::Copy => {
                return Some(Effect::Copy {
                    text: popup.content.text.clone(),
                    label: "formatted cell".into(),
                    flash: None,
                });
            }
            PopupAction::Edit => {
                let content = &popup.content;
                return Some(Effect::edit(content.text.clone(), content.is_json, None));
            }
        }
        None
    }
}
