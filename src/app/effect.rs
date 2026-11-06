//! Side effects requested by key handling, run by the main loop.
//!
//! Handlers only change state and return an `Effect`; talking to the
//! clipboard or the terminal happens in one place (`App::apply`). That keeps
//! handlers testable and is how the editor gets the terminal it needs.

use super::Flash;
use crate::editor::EditRequest;

pub enum Effect {
    /// Copy `text`; `label` describes it in the status bar. On success,
    /// `flash` highlights what was copied.
    Copy { text: String, label: String, flash: Option<Flash> },
    Edit(EditRequest),
}

impl Effect {
    pub fn edit(text: String, is_json: bool, line: Option<usize>) -> Self {
        let suffix = if is_json { ".json" } else { ".txt" };
        Effect::Edit(EditRequest { text, suffix, line })
    }
}
