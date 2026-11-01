//! Application state, drawing and key handling.

use std::ops::Range;
use std::time::{Duration, Instant};

use anyhow::Result;
use ratatui::{
    DefaultTerminal, Frame,
    crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    layout::{Constraint, Layout, Margin, Position},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap,
    },
};

use crate::cell::{self, Formatted};
use crate::clipboard;
use crate::editor::{self, EditRequest};
use crate::table::{Table, slice_cols};
use unicode_width::UnicodeWidthStr;

/// How many terminal columns H / L scroll the view.
const VIEW_STEP: usize = 4;

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

/// How long a copied cell/row stays highlighted (like Neovim's highlight-on-yank).
const FLASH_TIME: Duration = Duration::from_millis(300);

/// A short highlight of what was just copied.
struct Flash {
    row: usize,
    /// `None` = the whole row (`Y`).
    col: Option<usize>,
    until: Instant,
}

/// The cell detail popup (`x` / Enter).
struct Popup {
    title: String,
    content: Formatted,
    scroll: usize,
    /// Lines that fit inside the popup / total wrapped lines (set by draw).
    page_height: usize,
    line_count: usize,
}

/// What the screen is showing. An enum variant can carry data: the popup
/// state only exists while the popup is open.
enum Mode {
    Table,
    Detail(Popup),
    /// Typing a search query in the status bar (`/` or `?`).
    Search(Search),
}

/// A search: what to look for and in which direction.
/// Also remembered after Enter so `n` / `N` can repeat it.
#[derive(Clone)]
struct Search {
    query: String,
    forward: bool,
}

pub struct App {
    table: Table,
    mode: Mode,
    /// Cursor: data-row index and column index.
    row: usize,
    col: usize,
    /// First visible line of the scrolling area (0 = first line after header).
    top: usize,
    /// Horizontal scroll offset of the scrolling area, in terminal columns.
    left: usize,
    /// true: the view follows the cursor. false: the user scrolled the view
    /// away (H/L/Ctrl-e/Ctrl-y); the next cursor move turns it back on.
    follow_cursor: bool,
    view: View,
    /// One-shot message for the status bar ("copied ...", errors).
    message: Option<String>,
    /// Editor to open after this key press (needs the terminal, which only
    /// the main loop has).
    pending_edit: Option<EditRequest>,
    last_search: Option<Search>,
    flash: Option<Flash>,
    quit: bool,
}

impl App {
    pub fn new(table: Table) -> Self {
        Self {
            table,
            mode: Mode::Table,
            row: 0,
            col: 0,
            top: 0,
            left: 0,
            follow_cursor: true,
            view: View::default(),
            message: None,
            pending_edit: None,
            last_search: None,
            flash: None,
            quit: false,
        }
    }

    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        while !self.quit {
            terminal.draw(|frame| self.draw(frame))?;

            // While a flash is showing, wait for a key only until it expires;
            // on timeout, drop the flash and loop around to redraw.
            if let Some(flash) = &self.flash {
                let remaining = flash.until.saturating_duration_since(Instant::now());
                if !event::poll(remaining)? {
                    self.flash = None;
                    continue;
                }
            }
            self.handle_event(event::read()?);

            // `take()` moves the request out and leaves `None` behind.
            if let Some(req) = self.pending_edit.take()
                && let Err(e) = editor::open(terminal, &req)
            {
                self.message = Some(format!("editor: {e:#}"));
            }
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
        if self.follow_cursor {
            self.scroll_to_cursor();
        }
        self.clamp_view();

        // `split_at` gives two borrowed slices of the same Vec — no copying.
        let (header, rest) = self.table.lines.split_at(self.table.header_rows);

        let header_style = Style::new().add_modifier(Modifier::BOLD);
        let mut lines: Vec<Line> = header
            .iter()
            .take(height)
            .map(|l| self.render_line(l, header_style, None))
            .collect();

        for (i, line) in rest.iter().enumerate().skip(self.top).take(self.view.page_height) {
            let flash = self.flash.as_ref().filter(|f| f.row == i);
            let flash_style = Style::new().bg(Color::Yellow).fg(Color::Black);
            let cursor_style = Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD);
            let cell = |col: usize| self.table.columns.get(col).cloned();

            let (style, cursor) = match flash {
                // `Y`: the whole row lights up.
                Some(Flash { col: None, .. }) => (flash_style, None),
                // `y`: just the copied cell.
                Some(Flash { col: Some(c), .. }) => (
                    Style::new().bg(Color::Indexed(237)),
                    cell(*c).map(|r| (r, flash_style)),
                ),
                None if i == self.row && self.table.data_rows() > 0 => (
                    Style::new().bg(Color::Indexed(237)),
                    cell(self.col).map(|r| (r, cursor_style)),
                ),
                None => (Style::new(), None),
            };
            lines.push(self.render_line(line, style, cursor));
        }

        frame.render_widget(Paragraph::new(lines), body);
        frame.render_widget(
            Paragraph::new(self.status_text()).style(Style::new().add_modifier(Modifier::REVERSED)),
            status,
        );

        match &mut self.mode {
            Mode::Detail(popup) => draw_popup(frame, popup),
            // Show the real terminal cursor after the typed query.
            Mode::Search(search) => frame.set_cursor_position(Position::new(
                status.x + 1 + search.query.width() as u16,
                status.y,
            )),
            Mode::Table => {}
        }

    }

    /// Build one screen line: [frozen part][scrolled part], with one cell
    /// range (the cursor, or a copy flash) drawn in its own style.
    fn render_line(
        &self,
        line: &str,
        style: Style,
        cursor: Option<(Range<usize>, Style)>,
    ) -> Line<'static> {
        let View { frozen, scroll_width, .. } = self.view;
        let mut spans = Vec::new();
        let frozen_style = style.add_modifier(Modifier::BOLD);
        push_segment(&mut spans, line, 0..frozen, frozen_style, cursor.clone());
        let from = frozen + self.left;
        push_segment(&mut spans, line, from..from + scroll_width, style, cursor);
        Line::from(spans)
    }

    fn status_text(&self) -> String {
        if let Mode::Search(search) = &self.mode {
            let prompt = if search.forward { '/' } else { '?' };
            return format!("{prompt}{}", search.query);
        }
        if let Some(msg) = &self.message {
            return format!(" {msg}");
        }
        let help = "hjkl:move  HL:scroll  x:cell  e/E:vim  y/Y:copy  /:search  q:quit";
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

        // Horizontal: the frozen first column is always visible.
        if let Some(cell) = self.table.columns.get(self.col)
            && !(frozen > 0 && self.col == 0)
        {
            // + 1 to also show the cell's right border.
            let right = (cell.end + 1).min(self.table.max_width);
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

    /// Keep the view inside the content (also after a terminal resize).
    fn clamp_view(&mut self) {
        let View { frozen, scroll_width, .. } = self.view;
        self.top = self.top.min(self.max_top());
        let max_left = self.table.max_width.saturating_sub(frozen + scroll_width);
        self.left = self.left.min(max_left);
    }

    /// Largest useful `top`: the last screen shows the final line.
    fn max_top(&self) -> usize {
        let scroll_lines = self.table.lines.len() - self.table.header_rows;
        scroll_lines.saturating_sub(self.view.page_height)
    }

    /// Move only the view, not the cursor.
    fn scroll_view(&mut self, rows: isize, cols: isize) {
        self.follow_cursor = false;
        self.top = self.top.checked_add_signed(rows).unwrap_or(0);
        self.left = self.left.checked_add_signed(cols).unwrap_or(0);
        // Upper bounds are applied by `clamp_view` in the next draw.
    }

    // ---------------------------------------------------------------- keys

    fn handle_event(&mut self, event: Event) {
        let Event::Key(key) = event else { return };
        if key.kind != KeyEventKind::Press {
            return;
        }
        // A message stays until the next key press (redraws, e.g. when a
        // flash ends, keep it).
        self.message = None;
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.quit = true;
            return;
        }
        // `_` patterns only look at the variant and don't borrow what's
        // inside, so the arms are free to call `&mut self` methods.
        match self.mode {
            Mode::Table => self.table_key(key),
            Mode::Detail(_) => self.popup_key(key),
            Mode::Search(_) => self.search_key(key),
        }
    }

    /// Keys while typing a search query.
    fn search_key(&mut self, key: KeyEvent) {
        let Mode::Search(search) = &mut self.mode else { return };
        match key.code {
            KeyCode::Esc => self.mode = Mode::Table,
            // Backspace on an empty query cancels, like vim.
            KeyCode::Backspace => {
                if search.query.pop().is_none() {
                    self.mode = Mode::Table;
                }
            }
            KeyCode::Enter => {
                // `mem::take` moves the query out, leaving an empty String.
                let query = std::mem::take(&mut search.query);
                let forward = search.forward;
                self.mode = Mode::Table;
                if !query.is_empty() {
                    self.last_search = Some(Search { query, forward });
                } else if let Some(last) = &mut self.last_search {
                    // Empty `/` + Enter repeats the last query, in the new direction.
                    last.forward = forward;
                }
                self.search_next(false);
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                search.query.push(c);
            }
            _ => {}
        }
    }

    /// Jump to the next match of the last search (`reverse` = `N`).
    fn search_next(&mut self, reverse: bool) {
        let Some(Search { query, forward }) = self.last_search.clone() else {
            self.message = Some("no previous search".into());
            return;
        };
        let forward = forward != reverse; // `!=` on bools is XOR
        match self.table.find(self.row, self.col, &query, forward) {
            Some(hit) => {
                self.row = hit.row;
                self.set_col(hit.col);
                if hit.wrapped {
                    self.message = Some(if forward {
                        "search hit BOTTOM, continuing at TOP".into()
                    } else {
                        "search hit TOP, continuing at BOTTOM".into()
                    });
                }
            }
            None => self.message = Some(format!("Pattern not found: {query}")),
        }
    }

    /// `*` / `#`: search for the current cell's value.
    fn search_cell(&mut self, forward: bool) {
        match self.table.cell(self.row, self.col) {
            Some(query) if !query.is_empty() => {
                self.last_search = Some(Search { query, forward });
                self.search_next(false);
            }
            _ => self.message = Some("empty cell".into()),
        }
    }

    fn table_key(&mut self, key: KeyEvent) {
        let page = self.view.page_height.max(1) as isize;
        let half = (page / 2).max(1);
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        let step = VIEW_STEP as isize;

        // Arms are tried top to bottom, so the Ctrl/Shift variants must come
        // before the plain keys they share a KeyCode with.
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.quit = true,

            // View only.
            KeyCode::Char('e') if ctrl => self.scroll_view(1, 0),
            KeyCode::Char('y') if ctrl => self.scroll_view(-1, 0),
            KeyCode::Left if shift => self.scroll_view(0, -step),
            KeyCode::Right if shift => self.scroll_view(0, step),
            KeyCode::Char('H') => self.scroll_view(0, -step),
            KeyCode::Char('L') => self.scroll_view(0, step),

            // Cursor.
            KeyCode::Char('d') if ctrl => self.move_page(half),
            KeyCode::Char('u') if ctrl => self.move_page(-half),
            KeyCode::Down | KeyCode::Char('j' | 'w') => self.move_row(1),
            KeyCode::Up | KeyCode::Char('k' | 'b') => self.move_row(-1),
            KeyCode::Right | KeyCode::Char('l') => self.move_col(1),
            KeyCode::Left | KeyCode::Char('h') => self.move_col(-1),
            KeyCode::PageDown | KeyCode::Char(' ') => self.move_page(page),
            KeyCode::PageUp => self.move_page(-page),
            KeyCode::Home | KeyCode::Char('g') => {
                self.row = 0;
                self.top = 0;
                self.follow_cursor = true;
            }
            KeyCode::End | KeyCode::Char('G') => {
                self.row = self.table.data_rows().saturating_sub(1);
                self.top = self.max_top(); // also reveals the footer
                self.follow_cursor = true;
            }
            KeyCode::Char('0' | '^') => self.set_col(0),
            KeyCode::Char('$') => self.set_col(self.table.columns.len().saturating_sub(1)),

            // Actions on the current cell.
            KeyCode::Char('y') => {
                if let Some(value) = self.table.cell(self.row, self.col) {
                    let label = format!("cell: {value}");
                    // Plain text has no cells: flash the whole line.
                    let col = self.cell_mode().then_some(self.col);
                    if self.copy(&value, &label) {
                        self.start_flash(col);
                    }
                }
            }
            KeyCode::Char('Y') => {
                if let Some(row) = self.table.row(self.row) {
                    let cells = self.table.columns.len().max(1);
                    let label = format!("row {} ({cells} cells)", self.row + 1);
                    if self.copy(&row, &label) {
                        self.start_flash(None);
                    }
                }
            }
            KeyCode::Char('x') | KeyCode::Enter => self.open_popup(),

            // Search.
            KeyCode::Char(c @ ('/' | '?')) => {
                let search = Search { query: String::new(), forward: c == '/' };
                self.mode = Mode::Search(search);
            }
            KeyCode::Char('n') => self.search_next(false),
            KeyCode::Char('N') => self.search_next(true),
            KeyCode::Char('*') => self.search_cell(true),
            KeyCode::Char('#') => self.search_cell(false),
            KeyCode::Char('e') => {
                if let Some(value) = self.table.cell(self.row, self.col) {
                    let f = cell::format(&value);
                    self.edit(f.text, f.is_json, None);
                }
            }
            KeyCode::Char('E') => {
                let text = self.table.lines.join("\n");
                let line = self.table.header_rows + self.row + 1;
                self.edit(text, false, Some(line));
            }
            _ => {}
        }
    }

    fn popup_key(&mut self, key: KeyEvent) {
        // `let ... else` again: we know we're in Detail mode, get the popup.
        let Mode::Detail(popup) = &mut self.mode else { return };
        let page = popup.page_height.max(1) as isize;
        let half = (page / 2).max(1);
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let max_scroll = popup.line_count.saturating_sub(popup.page_height);
        let mut scroll_by = |n: isize| {
            popup.scroll = popup.scroll.checked_add_signed(n).unwrap_or(0).min(max_scroll);
        };

        match key.code {
            KeyCode::Char('q' | 'x') | KeyCode::Esc | KeyCode::Enter => self.mode = Mode::Table,
            KeyCode::Char('d') if ctrl => scroll_by(half),
            KeyCode::Char('u') if ctrl => scroll_by(-half),
            KeyCode::Down | KeyCode::Char('j' | 'w') => scroll_by(1),
            KeyCode::Up | KeyCode::Char('k' | 'b') => scroll_by(-1),
            KeyCode::PageDown | KeyCode::Char(' ') => scroll_by(page),
            KeyCode::PageUp => scroll_by(-page),
            KeyCode::Home | KeyCode::Char('g') => popup.scroll = 0,
            KeyCode::End | KeyCode::Char('G') => popup.scroll = max_scroll,
            KeyCode::Char('y') => {
                // Clone first: `self.copy` needs all of `self`, so the
                // borrow of `popup` (part of `self`) must end before it.
                let text = popup.content.text.clone();
                self.copy(&text, "formatted cell");
            }
            KeyCode::Char('e') => {
                let (text, is_json) = (popup.content.text.clone(), popup.content.is_json);
                self.edit(text, is_json, None);
            }
            _ => {}
        }
    }

    fn move_row(&mut self, delta: isize) {
        let last = self.table.data_rows().saturating_sub(1);
        // `checked_add_signed` returns None on underflow; clamp to 0 instead.
        self.row = self.row.checked_add_signed(delta).unwrap_or(0).min(last);
        self.follow_cursor = true;
    }

    fn move_page(&mut self, delta: isize) {
        // Move the view and the cursor together, like less/pspg.
        self.top = self.top.checked_add_signed(delta).unwrap_or(0).min(self.max_top());
        self.move_row(delta);
    }

    fn move_col(&mut self, delta: isize) {
        if self.cell_mode() {
            let last = self.table.columns.len() - 1;
            self.set_col(self.col.checked_add_signed(delta).unwrap_or(0).min(last));
        } else {
            // Plain text has no cells: h/l scroll sideways instead.
            self.scroll_view(0, delta * VIEW_STEP as isize);
        }
    }

    fn set_col(&mut self, col: usize) {
        self.col = col;
        self.follow_cursor = true;
    }

    // ---------------------------------------------------------------- actions

    fn open_popup(&mut self) {
        let Some(value) = self.table.cell(self.row, self.col) else { return };
        let column = self.table.column_names.get(self.col).map_or("line", String::as_str);
        self.mode = Mode::Detail(Popup {
            title: format!("{column} · row {}", self.row + 1),
            content: cell::format(&value),
            scroll: 0,
            page_height: 0,
            line_count: 0,
        });
    }

    /// Copy `text`; `label` describes it in the status bar.
    /// Returns whether it worked.
    fn copy(&mut self, text: &str, label: &str) -> bool {
        let result = clipboard::copy(text);
        self.message = Some(match &result {
            Ok(()) => format!("copied {label}"),
            Err(e) => format!("copy failed: {e}"),
        });
        result.is_ok()
    }

    fn start_flash(&mut self, col: Option<usize>) {
        self.flash = Some(Flash { row: self.row, col, until: Instant::now() + FLASH_TIME });
    }

    fn edit(&mut self, text: String, is_json: bool, line: Option<usize>) {
        let suffix = if is_json { ".json" } else { ".txt" };
        self.pending_edit = Some(EditRequest { text, suffix, line });
    }
}

/// Draw the detail popup on top of the table, using ratatui's building
/// blocks: `Rect::centered`, `Clear`, `Block`, `Paragraph` + `Scrollbar`.
fn draw_popup(frame: &mut Frame, popup: &mut Popup) {
    let area = frame
        .area()
        .centered(Constraint::Percentage(80), Constraint::Percentage(80));

    let mut block = Block::bordered()
        .title(format!(" {} ", popup.title))
        .title_bottom(" j/k:scroll  y:copy  e:vim  q:close ");
    if let Some(note) = &popup.content.note {
        block = block.title(Line::from(format!(" {note} ")).right_aligned().style(Style::new().fg(Color::Red)));
    }

    let paragraph = Paragraph::new(popup.content.text.as_str())
        .block(block)
        .wrap(Wrap { trim: false });

    // Let ratatui tell us how tall the wrapped text is (block borders included).
    popup.line_count = paragraph.line_count(area.width).saturating_sub(2);
    popup.page_height = area.height.saturating_sub(2) as usize;
    let max_scroll = popup.line_count.saturating_sub(popup.page_height);
    popup.scroll = popup.scroll.min(max_scroll); // e.g. after a resize

    frame.render_widget(Clear, area); // wipe the table underneath
    frame.render_widget(paragraph.scroll((popup.scroll as u16, 0)), area);

    if max_scroll > 0 {
        let mut state = ScrollbarState::new(max_scroll).position(popup.scroll);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight),
            area.inner(Margin { vertical: 1, horizontal: 0 }),
            &mut state,
        );
    }
}

/// Append the screen columns `range` of `line` as spans, drawing the part
/// that overlaps `cursor` in the cursor's style.
fn push_segment(
    spans: &mut Vec<Span<'static>>,
    line: &str,
    range: Range<usize>,
    style: Style,
    cursor: Option<(Range<usize>, Style)>,
) {
    if range.is_empty() {
        return;
    }
    // Intersect the cursor cell with this segment.
    let (hl, cursor_style) = match cursor {
        Some((c, s)) => (Some(c.start.max(range.start)..c.end.min(range.end)), s),
        None => (None, style),
    };
    let hl = hl.filter(|c| !c.is_empty());

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
