//! All key bindings in one place.
//!
//! Each screen mode has an action enum. The functions here map a key press
//! to an action, and `app.rs` decides what each action does. To change a
//! binding, edit the `match` below. If you change it, update the help
//! strings at the bottom too.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Actions in the table view.
pub enum TableAction {
    Quit,
    /// Clear search highlights; quit if nothing is highlighted.
    Cancel,

    // View only (the cursor stays put).
    ScrollDown,
    ScrollUp,
    ScrollLeft,
    ScrollRight,

    // Cursor.
    Down,
    Up,
    Right,
    Left,
    HalfPageDown,
    HalfPageUp,
    PageDown,
    PageUp,
    Top,
    Bottom,
    FirstCol,
    LastCol,

    // Actions on the current cell / row.
    CopyCell,
    CopyRow,
    OpenPopup,
    EditCell,
    EditAll,

    // Search.
    SearchForward,
    SearchBackward,
    NextMatch,
    PrevMatch,
    SearchCellForward,
    SearchCellBackward,
}

/// Actions in the cell detail popup.
pub enum PopupAction {
    Close,
    Down,
    Up,
    HalfPageDown,
    HalfPageUp,
    PageDown,
    PageUp,
    Top,
    Bottom,
    Copy,
    Edit,
}

/// Actions while typing a search query.
pub enum SearchAction {
    Cancel,
    Backspace,
    Submit,
    Insert(char),
}

/// Ctrl-c quits from any mode.
pub fn is_force_quit(key: KeyEvent) -> bool {
    key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL)
}

pub fn table_action(key: KeyEvent) -> Option<TableAction> {
    use TableAction::*;
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);

    // Arms are tried top to bottom, so the Ctrl/Shift variants must come
    // before the plain keys they share a KeyCode with.
    let action = match key.code {
        KeyCode::Char('q') => Quit,
        KeyCode::Esc => Cancel,

        // View only.
        KeyCode::Char('e') if ctrl => ScrollDown,
        KeyCode::Char('y') if ctrl => ScrollUp,
        KeyCode::Left if shift => ScrollLeft,
        KeyCode::Right if shift => ScrollRight,
        KeyCode::Char('H') => ScrollLeft,
        KeyCode::Char('L') => ScrollRight,

        // Cursor.
        KeyCode::Char('d') if ctrl => HalfPageDown,
        KeyCode::Char('u') if ctrl => HalfPageUp,
        KeyCode::Down | KeyCode::Char('j') => Down,
        KeyCode::Up | KeyCode::Char('k') => Up,
        // w / b: next / previous cell, like vim's word motions.
        KeyCode::Right | KeyCode::Char('l' | 'w') => Right,
        KeyCode::Left | KeyCode::Char('h' | 'b') => Left,
        KeyCode::PageDown | KeyCode::Char(' ') => PageDown,
        KeyCode::PageUp => PageUp,
        KeyCode::Home | KeyCode::Char('g') => Top,
        KeyCode::End | KeyCode::Char('G') => Bottom,
        KeyCode::Char('0' | '^') => FirstCol,
        KeyCode::Char('$') => LastCol,

        // Current cell / row.
        KeyCode::Char('y') => CopyCell,
        KeyCode::Char('Y') => CopyRow,
        KeyCode::Char('f') | KeyCode::Enter => OpenPopup,
        KeyCode::Char('e') => EditCell,
        KeyCode::Char('E') => EditAll,

        // Search.
        KeyCode::Char('/') => SearchForward,
        KeyCode::Char('?') => SearchBackward,
        KeyCode::Char('n') => NextMatch,
        KeyCode::Char('N') => PrevMatch,
        KeyCode::Char('*') => SearchCellForward,
        KeyCode::Char('#') => SearchCellBackward,

        _ => return None,
    };
    Some(action)
}

pub fn popup_action(key: KeyEvent) -> Option<PopupAction> {
    use PopupAction::*;
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    let action = match key.code {
        KeyCode::Char('q' | 'f') | KeyCode::Esc | KeyCode::Enter => Close,
        KeyCode::Char('d') if ctrl => HalfPageDown,
        KeyCode::Char('u') if ctrl => HalfPageUp,
        KeyCode::Down | KeyCode::Char('j') => Down,
        KeyCode::Up | KeyCode::Char('k') => Up,
        KeyCode::PageDown | KeyCode::Char(' ') => PageDown,
        KeyCode::PageUp => PageUp,
        KeyCode::Home | KeyCode::Char('g') => Top,
        KeyCode::End | KeyCode::Char('G') => Bottom,
        KeyCode::Char('y') => Copy,
        KeyCode::Char('e') => Edit,
        _ => return None,
    };
    Some(action)
}

pub fn search_action(key: KeyEvent) -> Option<SearchAction> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let action = match key.code {
        KeyCode::Esc => SearchAction::Cancel,
        KeyCode::Backspace => SearchAction::Backspace,
        KeyCode::Enter => SearchAction::Submit,
        KeyCode::Char(c) if !ctrl => SearchAction::Insert(c),
        _ => return None,
    };
    Some(action)
}

/// Help shown in the status bar of the table view.
pub const TABLE_HELP: &str = "hjkl:move  HL:scroll  f:cell  e/E:vim  y/Y:copy  /:search  q:quit";

/// Help shown at the bottom of the cell detail popup.
pub const POPUP_HELP: &str = " j/k:scroll  y:copy  e:vim  q:close ";
