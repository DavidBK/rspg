//! The cell detail popup, drawn over the table with ratatui's building
//! blocks: `Rect::centered`, `Clear`, `Block`, `Paragraph` + `Scrollbar`.

use ratatui::Frame;
use ratatui::layout::{Constraint, Margin, Rect};
use ratatui::text::Line;
use ratatui::widgets::{Block, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap};

use super::theme;
use crate::app::popup::Popup;
use crate::keys;

/// Where the popup goes on a `screen`-sized terminal.
pub fn area(screen: Rect) -> Rect {
    screen.centered(Constraint::Percentage(80), Constraint::Percentage(80))
}

/// Height of the wrapped text inside a popup `width` columns wide.
pub fn content_height(popup: &Popup, width: u16) -> usize {
    // ratatui counts the block borders too.
    paragraph(popup).line_count(width).saturating_sub(2)
}

pub fn render(frame: &mut Frame, popup: &Popup) {
    let area = area(frame.area());
    frame.render_widget(Clear, area); // wipe the table underneath
    frame.render_widget(paragraph(popup).scroll((popup.scroll as u16, 0)), area);

    let max_scroll = popup.max_scroll();
    if max_scroll > 0 {
        let mut state = ScrollbarState::new(max_scroll).position(popup.scroll);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight),
            area.inner(Margin { vertical: 1, horizontal: 0 }),
            &mut state,
        );
    }
}

fn paragraph(popup: &Popup) -> Paragraph<'_> {
    let mut block = Block::bordered()
        .border_style(theme::POPUP_BORDER)
        .title(format!(" {} ", popup.title))
        .title_bottom(keys::POPUP_HELP);
    if let Some(note) = &popup.content.note {
        block = block.title(Line::from(format!(" {note} ")).right_aligned().style(theme::POPUP_NOTE));
    }
    Paragraph::new(popup.content.text.as_str()).block(block).wrap(Wrap { trim: false })
}
