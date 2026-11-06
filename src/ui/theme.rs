//! All colours and styles in one place.

use ratatui::style::{Color, Modifier, Style};

/// pgcli's default `output.header = "#00ff5f bold"`; #00ff5f is exactly
/// xterm-256 colour 47, which works without truecolor too.
const GREEN: Color = Color::Indexed(47);

pub const HEADER: Style = Style::new().fg(GREEN).add_modifier(Modifier::BOLD);
pub const MATCH: Style = Style::new().bg(Color::LightYellow).fg(Color::Black);
pub const FLASH: Style = Style::new().bg(Color::Green).fg(Color::Black);
pub const CURSOR: Style = Style::new().add_modifier(Modifier::REVERSED.union(Modifier::BOLD));
pub const CURSOR_ROW: Style = Style::new().bg(Color::Indexed(237));
pub const STATUS: Style = Style::new().add_modifier(Modifier::REVERSED);
pub const POPUP_BORDER: Style = Style::new().fg(GREEN);
pub const POPUP_NOTE: Style = Style::new().fg(Color::Red);
