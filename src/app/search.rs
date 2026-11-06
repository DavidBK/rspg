//! `/` `?` `n` `N` `*` `#`: vim-style search.

use super::{App, Mode};
use crate::keys::SearchAction;
use crate::table::Matcher;

/// A search: what to look for and in which direction. Used both for the
/// query being typed (`Mode::Search`) and the remembered one for `n` / `N`.
#[derive(Clone)]
pub struct Search {
    pub query: String,
    pub forward: bool,
}

/// What survives after the prompt closes.
#[derive(Default)]
pub struct SearchState {
    last: Option<Search>,
    /// Highlight matches of `last` (vim's hlsearch); Esc turns it off.
    pub highlight: bool,
}

impl App {
    /// What to highlight: the query being typed (incsearch), otherwise the
    /// last search while highlighting is on.
    pub fn active_matcher(&self) -> Option<Matcher> {
        match &self.mode {
            Mode::Search(s) if !s.query.is_empty() => Matcher::new(&s.query),
            _ if self.search.highlight => Matcher::new(&self.search.last.as_ref()?.query),
            _ => None,
        }
    }

    pub(super) fn start_search(&mut self, forward: bool) {
        self.mode = Mode::Search(Search { query: String::new(), forward });
    }

    /// Keys while typing a search query.
    pub(super) fn on_search(&mut self, action: SearchAction) {
        let Mode::Search(prompt) = &mut self.mode else { return };
        match action {
            SearchAction::Cancel => self.mode = Mode::Table,
            // Backspace on an empty query cancels, like vim.
            SearchAction::Backspace => {
                if prompt.query.pop().is_none() {
                    self.mode = Mode::Table;
                }
            }
            SearchAction::Insert(c) => prompt.query.push(c),
            SearchAction::Submit => {
                // `mem::take` moves the query out, leaving an empty String.
                let query = std::mem::take(&mut prompt.query);
                let forward = prompt.forward;
                self.mode = Mode::Table;
                if !query.is_empty() {
                    self.search.last = Some(Search { query, forward });
                } else if let Some(last) = &mut self.search.last {
                    // Empty `/` + Enter repeats the last query, in the new direction.
                    last.forward = forward;
                }
                self.search_next(false);
            }
        }
    }

    /// Jump to the next match of the last search (`reverse` = `N`).
    pub(super) fn search_next(&mut self, reverse: bool) {
        let Some(Search { query, forward }) = self.search.last.clone() else {
            self.message = Some("no previous search".into());
            return;
        };
        let forward = forward != reverse; // `!=` on bools is XOR
        self.search.highlight = true;
        match self.table.find(self.cursor.row, self.cursor.col, &query, forward) {
            Some(hit) => {
                self.cursor.row = hit.row;
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
    pub(super) fn search_cell(&mut self, forward: bool) {
        match self.table.cell(self.cursor.row, self.cursor.col) {
            Some(query) if !query.is_empty() => {
                self.search.last = Some(Search { query, forward });
                self.search_next(false);
            }
            _ => self.message = Some("empty cell".into()),
        }
    }
}
