//! Drawing. Everything here only reads the `App`; geometry that key
//! handling needs is computed beforehand by `App::layout`.

pub mod popup;
mod status;
mod table_view;
mod theme;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};

use crate::app::{App, Mode};

/// Split the screen into (table body, status bar).
pub fn split(area: Rect) -> (Rect, Rect) {
    let [body, status] = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(area);
    (body, status)
}

pub fn draw(frame: &mut Frame, app: &App) {
    let (body, status) = split(frame.area());
    table_view::render(frame, body, app);
    status::render(frame, status, app);
    if let Mode::Detail(p) = &app.mode {
        popup::render(frame, p);
    }
}
