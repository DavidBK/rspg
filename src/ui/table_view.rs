//! The table body: frozen header and first column, cursor, search
//! matches and the copy flash.

use std::ops::Range;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::theme;
use crate::app::viewport::Viewport;
use crate::app::{App, Flash};
use crate::table::{is_border_line, slice_cols};

/// Styled screen-column ranges, painted in order (later ones win).
type Highlights = Vec<(Range<usize>, Style)>;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let table = &app.table;
    let view = &app.viewport;

    // `split_at` gives two borrowed slices of the same Vec — no copying.
    let (header, body) = table.lines.split_at(table.header_rows);

    let mut lines: Vec<Line> = header
        .iter()
        .take(area.height as usize)
        .map(|l| {
            // Like pgcli: only the column names are green; the `|`
            // separators and border lines (`+----+`) keep the default colour.
            let names: Highlights = if is_border_line(l) {
                Vec::new()
            } else {
                table.columns.iter().map(|r| (r.clone(), theme::HEADER)).collect()
            };
            render_line(view, l, Style::new(), &names)
        })
        .collect();

    let matcher = app.active_matcher();
    let cell = |col: usize| table.columns.get(col).cloned();

    for (i, line) in body.iter().enumerate().skip(view.top).take(view.page_height) {
        let is_data = i < table.data_rows();

        let mut highlights = Highlights::new();
        if is_data && let Some(m) = &matcher {
            highlights.extend(table.match_ranges(line, m).into_iter().map(|r| (r, theme::MATCH)));
        }

        let flash = app.flash.as_ref().filter(|f| f.row == i);
        let style = match flash {
            // `Y`: the whole row lights up.
            Some(Flash { col: None, .. }) => theme::FLASH,
            // `y`: just the copied cell.
            Some(Flash { col: Some(c), .. }) => {
                highlights.extend(cell(*c).map(|r| (r, theme::FLASH)));
                theme::CURSOR_ROW
            }
            None if i == app.cursor.row && is_data => {
                highlights.extend(cell(app.cursor.col).map(|r| (r, theme::CURSOR)));
                theme::CURSOR_ROW
            }
            None => Style::new(),
        };
        lines.push(render_line(view, line, style, &highlights));
    }

    frame.render_widget(Paragraph::new(lines), area);
}

/// Build one screen line: [frozen part][scrolled part].
fn render_line(view: &Viewport, line: &str, style: Style, highlights: &[(Range<usize>, Style)]) -> Line<'static> {
    let mut spans = Vec::new();
    let frozen_style = style.add_modifier(Modifier::BOLD);
    push_segment(&mut spans, line, 0..view.frozen, frozen_style, highlights);
    let from = view.frozen + view.left;
    push_segment(&mut spans, line, from..from + view.scroll_width, style, highlights);
    Line::from(spans)
}

/// Append the screen columns `range` of `line` as spans.
///
/// Every column starts in `style`; each highlight then paints its own style
/// over the columns it covers (later highlights win). Finally, neighbouring
/// columns with the same style are joined into one span.
fn push_segment(
    spans: &mut Vec<Span<'static>>,
    line: &str,
    range: Range<usize>,
    style: Style,
    highlights: &[(Range<usize>, Style)],
) {
    if range.is_empty() {
        return;
    }
    // `vec![x; n]` = n copies of x (Style is Copy, so that's cheap).
    let mut styles = vec![style; range.len()];
    for (r, s) in highlights {
        // Clip the highlight to this segment (empty range = no-op loop).
        for col in r.start.max(range.start)..r.end.min(range.end) {
            styles[col - range.start] = *s;
        }
    }

    // Emit one span per run of equal styles.
    let mut run = 0;
    for i in 1..=styles.len() {
        if i == styles.len() || styles[i] != styles[run] {
            let text = slice_cols(line, range.start + run, i - run);
            spans.push(Span::styled(text, styles[run]));
            run = i;
        }
    }
}
