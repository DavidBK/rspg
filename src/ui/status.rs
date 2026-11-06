//! The status bar: position and help, a message, or the search prompt.

use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use super::theme;
use crate::app::{App, Mode};
use crate::keys;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    frame.render_widget(Paragraph::new(text(app)).style(theme::STATUS), area);

    // Show the real terminal cursor after the typed query.
    if let Mode::Search(search) = &app.mode {
        frame.set_cursor_position(Position::new(area.x + 1 + search.query.width() as u16, area.y));
    }
}

fn text(app: &App) -> String {
    if let Mode::Search(search) = &app.mode {
        let prompt = if search.forward { '/' } else { '?' };
        return format!("{prompt}{}", search.query);
    }
    if let Some(msg) = &app.message {
        return format!(" {msg}");
    }
    let help = keys::TABLE_HELP;
    let table = &app.table;
    let rows = table.data_rows();
    let row = (app.cursor.row + 1).min(rows);
    if table.has_cells() {
        let col = app.cursor.col;
        let name = table.column_names.get(col).map_or("", String::as_str);
        let cols = table.columns.len();
        format!(" row {row}/{rows}  col {}/{cols} [{name}]   {help}", col + 1)
    } else {
        format!(" line {row}/{rows}   {help}")
    }
}
