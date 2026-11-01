//! Application state, drawing and key handling.

use std::ops::Range;

use anyhow::Result;
use ratatui::{
    DefaultTerminal, Frame,
    crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::clipboard;
use crate::table::{Table, slice_cols};

/// Screen geometry computed by the last `draw`, needed by key handling
/// (e.g. how far PageDown should jump). `Default` gives all zeros.
#[derive(Default, Clone, Copy)]
struct View {
    /// Visible data lines (screen height minus header and status bar).
    page_height: usize,
    /// Width of the frozen first column (0 = nothing frozen).
    frozen: usize,
    /// Width of the horizontally scrolling part.
    scroll_width: usize,
}

pub struct App {
    table: Table,
    /// Cursor: data-row index and column index.
    row: usize,
    col: usize,
    /// First visible line of the scrolling area (0 = first line after header).
    top: usize,
    /// Horizontal scroll offset of the scrolling area, in terminal columns.
    left: usize,
    view: View,
    /// One-shot message for the status bar ("copied ...", errors).
    message: Option<String>,
    quit: bool,
}

impl App {
    pub fn new(table: Table) -> Self {
        Self {
            table,
            row: 0,
            col: 0,
            top: 0,
            left: 0,
            view: View::default(),
            message: None,
            quit: false,
        }
    }

    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        while !self.quit {
            terminal.draw(|frame| self.draw(frame))?;
            self.handle_event(event::read()?);
        }
        Ok(())
    }

    /// Cell mode: the input is a table, so the cursor moves cell by cell.
    /// Otherwise (plain text) the cursor is just a line.
    fn cell_mode(&self) -> bool {
        !self.table.columns.is_empty()
    }

    // ---------------------------------------------------------------- drawing

    fn draw(&mut self, frame: &mut Frame) {
        let [body, status] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(frame.area());
        let width = body.width as usize;
        let height = body.height as usize;

        // Like pspg: freeze the first column only if it leaves enough room.
        let mut frozen = match self.table.columns.as_slice() {
            [first, _, ..] => first.end + 1, // + 1 = include its right border
            _ => 0,
        };
        if frozen * 2 > width {
            frozen = 0;
        }
        self.view = View {
            page_height: height.saturating_sub(self.table.header_rows).max(1),
            frozen,
            scroll_width: width - frozen,
        };
        self.scroll_to_cursor();

        // `split_at` gives two borrowed slices of the same Vec — no copying.
        let (header, rest) = self.table.lines.split_at(self.table.header_rows);

        let header_style = Style::new().add_modifier(Modifier::BOLD);
        let mut lines: Vec<Line> = header
            .iter()
            .take(height)
            .map(|l| self.render_line(l, header_style, None))
            .collect();

        for (i, line) in rest.iter().enumerate().skip(self.top).take(self.view.page_height) {
            let (style, cursor) = if i == self.row && self.table.data_rows() > 0 {
                let cursor = self.table.columns.get(self.col).cloned();
                (Style::new().bg(Color::Indexed(237)), cursor)
            } else {
                (Style::new(), None)
            };
            lines.push(self.render_line(line, style, cursor));
        }

        frame.render_widget(Paragraph::new(lines), body);
        frame.render_widget(
            Paragraph::new(self.status_text()).style(Style::new().add_modifier(Modifier::REVERSED)),
            status,
        );
        // Messages are shown once, then cleared.
        self.message = None;
    }

    /// Build one screen line: [frozen part][scrolled part], with the cursor
    /// cell (if any) highlighted.
    fn render_line(&self, line: &str, style: Style, cursor: Option<Range<usize>>) -> Line<'static> {
        let View { frozen, scroll_width, .. } = self.view;
        let mut spans = Vec::new();
        let frozen_style = style.add_modifier(Modifier::BOLD);
        push_segment(&mut spans, line, 0..frozen, frozen_style, cursor.clone());
        let from = frozen + self.left;
        push_segment(&mut spans, line, from..from + scroll_width, style, cursor);
        Line::from(spans)
    }

    fn status_text(&self) -> String {
        let help = "hjkl:move  y:copy  q:quit";
        if let Some(msg) = &self.message {
            return format!(" {msg}");
        }
        let rows = self.table.data_rows();
        if self.cell_mode() {
            let name = self.table.column_names.get(self.col).map_or("", String::as_str);
            format!(
                " row {}/{}  col {}/{} [{}]   {help}",
                (self.row + 1).min(rows),
                rows,
                self.col + 1,
                self.table.columns.len(),
                name,
            )
        } else {
            format!(" line {}/{}   {help}", (self.row + 1).min(rows), rows)
        }
    }

    // ---------------------------------------------------------------- scrolling

    /// Adjust `top` and `left` so the cursor cell is on screen.
    fn scroll_to_cursor(&mut self) {
        let View { page_height, frozen, scroll_width } = self.view;

        // Vertical.
        if self.row < self.top {
            self.top = self.row;
        } else if self.row >= self.top + page_height {
            self.top = self.row + 1 - page_height;
        }
        self.top = self.top.min(self.max_top());

        // Horizontal: the frozen first column is always visible.
        let max_left = self.table.max_width.saturating_sub(frozen + scroll_width);
        if let Some(cell) = self.table.columns.get(self.col)
            && !(frozen > 0 && self.col == 0)
        {
            // + 1 to also show the cell's right border.
            let right = (cell.end + 1).min(self.table.max_width);
            if right > frozen + self.left + scroll_width {
                self.left = right - frozen - scroll_width;
            }
            if cell.start < frozen + self.left {
                self.left = cell.start.saturating_sub(frozen + 1);
            }
        }
        self.left = self.left.min(max_left);
    }

    /// Largest useful `top`: the last screen shows the final line.
    fn max_top(&self) -> usize {
        let scroll_lines = self.table.lines.len() - self.table.header_rows;
        scroll_lines.saturating_sub(self.view.page_height)
    }

    // ---------------------------------------------------------------- keys

    fn handle_event(&mut self, event: Event) {
        let Event::Key(key) = event else { return };
        if key.kind != KeyEventKind::Press {
            return;
        }

        let page = self.view.page_height.max(1) as isize;
        match (key.code, key.modifiers) {
            (KeyCode::Char('q') | KeyCode::Esc, _) => self.quit = true,
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => self.quit = true,

            (KeyCode::Down | KeyCode::Char('j'), _) => self.move_row(1),
            (KeyCode::Up | KeyCode::Char('k'), _) => self.move_row(-1),
            (KeyCode::Right | KeyCode::Char('l'), _) => self.move_col(1),
            (KeyCode::Left | KeyCode::Char('h'), _) => self.move_col(-1),

            (KeyCode::PageDown | KeyCode::Char(' '), _) => self.move_page(page),
            (KeyCode::PageUp | KeyCode::Char('b'), _) => self.move_page(-page),
            (KeyCode::Home | KeyCode::Char('g'), _) => {
                self.row = 0;
                self.top = 0;
            }
            (KeyCode::End | KeyCode::Char('G'), _) => {
                self.row = self.table.data_rows().saturating_sub(1);
                self.top = self.max_top(); // also reveals the footer
            }
            (KeyCode::Char('0' | '^'), _) => self.col = 0,
            (KeyCode::Char('$'), _) => self.col = self.table.columns.len().saturating_sub(1),

            (KeyCode::Char('y'), _) => self.copy_cell(),
            _ => {}
        }
    }

    fn move_row(&mut self, delta: isize) {
        let last = self.table.data_rows().saturating_sub(1);
        // `checked_add_signed` returns None on underflow; clamp to 0 instead.
        self.row = self.row.checked_add_signed(delta).unwrap_or(0).min(last);
    }

    fn move_page(&mut self, delta: isize) {
        // Move the view and the cursor together, like less/pspg.
        self.top = self.top.checked_add_signed(delta).unwrap_or(0).min(self.max_top());
        self.move_row(delta);
    }

    fn move_col(&mut self, delta: isize) {
        if self.cell_mode() {
            let last = self.table.columns.len() - 1;
            self.col = self.col.checked_add_signed(delta).unwrap_or(0).min(last);
        } else {
            // Plain text has no cells: h/l scroll sideways instead.
            self.left = self.left.checked_add_signed(delta * 4).unwrap_or(0);
        }
    }

    fn copy_cell(&mut self) {
        let Some(value) = self.table.cell(self.row, self.col) else { return };
        self.message = Some(match clipboard::copy(&value) {
            Ok(()) => format!("copied: {value}"),
            Err(e) => format!("copy failed: {e}"),
        });
    }
}

/// Append the screen columns `range` of `line` as spans, highlighting the
/// part that overlaps `cursor`.
fn push_segment(
    spans: &mut Vec<Span<'static>>,
    line: &str,
    range: Range<usize>,
    style: Style,
    cursor: Option<Range<usize>>,
) {
    if range.is_empty() {
        return;
    }
    let cursor_style = Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD);
    // Intersect the cursor cell with this segment.
    let hl = cursor
        .map(|c| c.start.max(range.start)..c.end.min(range.end))
        .filter(|c| !c.is_empty());

    match hl {
        None => spans.push(Span::styled(slice_cols(line, range.start, range.len()), style)),
        Some(hl) => {
            let parts = [
                (range.start..hl.start, style),
                (hl.clone(), cursor_style),
                (hl.end..range.end, style),
            ];
            for (r, s) in parts {
                if !r.is_empty() {
                    spans.push(Span::styled(slice_cols(line, r.start, r.len()), s));
                }
            }
        }
    }
}
