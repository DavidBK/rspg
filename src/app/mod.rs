//! Application state and the main loop.
//!
//! Flow per key press: `keys` maps it to an action, an `on_*` handler
//! updates the state and may return an `Effect` (clipboard, editor), which
//! `apply` runs. Before each draw, `layout` fits the state to the screen
//! size; drawing itself (`ui`) only reads.

mod effect;
pub mod popup;
pub mod search;
mod table_mode;
pub mod viewport;

use std::time::{Duration, Instant};

use anyhow::Result;
use ratatui::{
    DefaultTerminal,
    crossterm::event::{self, Event, KeyEventKind},
    layout::Rect,
};

use crate::table::Table;
use crate::{clipboard, editor, keys, ui};
use effect::Effect;
use popup::Popup;
use search::{Search, SearchState};
use viewport::Viewport;

/// How long a copied cell/row stays highlighted (like Neovim's highlight-on-yank).
const FLASH_TIME: Duration = Duration::from_millis(300);

/// Cursor position: data-row index and column index.
#[derive(Default, Clone, Copy)]
pub struct Cursor {
    pub row: usize,
    pub col: usize,
}

/// A short highlight of what was just copied.
pub struct Flash {
    pub row: usize,
    /// `None` = the whole row (`Y`).
    pub col: Option<usize>,
    until: Instant,
}

impl Flash {
    fn new(row: usize, col: Option<usize>) -> Self {
        Self { row, col, until: Instant::now() + FLASH_TIME }
    }
}

/// What the screen is showing. Each variant carries the state that only
/// exists in that mode.
pub enum Mode {
    Table,
    Detail(Popup),
    /// Typing a search query in the status bar (`/` or `?`).
    Search(Search),
}

pub struct App {
    pub table: Table,
    pub mode: Mode,
    pub cursor: Cursor,
    pub viewport: Viewport,
    pub search: SearchState,
    /// One-shot message for the status bar ("copied ...", errors).
    pub message: Option<String>,
    pub flash: Option<Flash>,
    quit: bool,
}

impl App {
    pub fn new(table: Table) -> Self {
        Self {
            table,
            mode: Mode::Table,
            cursor: Cursor::default(),
            viewport: Viewport::default(),
            search: SearchState::default(),
            message: None,
            flash: None,
            quit: false,
        }
    }

    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        while !self.quit {
            terminal.draw(|frame| {
                self.layout(frame.area());
                ui::draw(frame, self);
            })?;

            // While a flash is showing, wait for a key only until it expires;
            // on timeout, drop the flash and loop around to redraw.
            if let Some(flash) = &self.flash {
                let remaining = flash.until.saturating_duration_since(Instant::now());
                if !event::poll(remaining)? {
                    self.flash = None;
                    continue;
                }
            }

            // Handle every queued event before the next draw, so a held-down
            // key doesn't lag behind redrawing each step.
            loop {
                if let Some(effect) = self.handle_event(event::read()?) {
                    self.apply(effect, terminal);
                }
                if self.quit || !event::poll(Duration::ZERO)? {
                    break;
                }
            }
        }
        Ok(())
    }

    /// Fit the state to the screen size (also after a terminal resize).
    fn layout(&mut self, area: Rect) {
        let (body, _) = ui::split(area);
        self.viewport.fit(body.width as usize, body.height as usize, self.cursor, &self.table);
        if let Mode::Detail(popup) = &mut self.mode {
            popup.fit(ui::popup::area(area));
        }
    }

    fn handle_event(&mut self, event: Event) -> Option<Effect> {
        let Event::Key(key) = event else { return None };
        if key.kind != KeyEventKind::Press {
            return None;
        }
        // A message stays until the next key press (redraws, e.g. when a
        // flash ends, keep it).
        self.message = None;
        if keys::is_force_quit(key) {
            self.quit = true;
            return None;
        }
        match self.mode {
            Mode::Table => self.on_table(keys::table_action(key)?),
            Mode::Detail(_) => self.on_popup(keys::popup_action(key)?),
            Mode::Search(_) => {
                self.on_search(keys::search_action(key)?);
                None
            }
        }
    }

    fn apply(&mut self, effect: Effect, terminal: &mut DefaultTerminal) {
        match effect {
            Effect::Copy { text, label, flash } => match clipboard::copy(&text) {
                Ok(()) => {
                    self.message = Some(format!("copied {label}"));
                    self.flash = flash;
                }
                Err(e) => self.message = Some(format!("copy failed: {e}")),
            },
            Effect::Edit(req) => {
                if let Err(e) = editor::open(terminal, &req) {
                    self.message = Some(format!("editor: {e:#}"));
                }
            }
        }
    }
}
