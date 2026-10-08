//! Rendering helpers for mouse surfaces. Drawing and hit geometry share one
//! buffer, including wrapped lines and textareas' own screen-coordinate map.
use super::{mouse::MouseState, theme};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Paragraph, Widget, Wrap},
};
use ratatui_textarea::{CursorMove, TextArea};

pub(crate) struct Surface<'a> {
    pub(crate) buffer: &'a mut Buffer,
}
impl Surface<'_> {
    pub(crate) fn render_widget(&mut self, widget: impl Widget, area: Rect) {
        widget.render(area, self.buffer);
    }
}

pub(crate) fn buttons_height<T>(width: u16, buttons: &[(&str, T)]) -> u16 {
    button_rects(Rect::new(0, 0, width, u16::MAX), buttons)
        .last()
        .map_or(0, |r| r.y + 1)
}
fn button_rects<T>(area: Rect, buttons: &[(&str, T)]) -> Vec<Rect> {
    if area.is_empty() {
        return Vec::new();
    }
    let mut x = 0;
    let mut y = 0;
    buttons
        .iter()
        .map(|(label, _)| {
            let width = (Span::raw(*label).width() as u16).min(area.width);
            if x > 0 && x + width > area.width {
                x = 0;
                y += 1;
            }
            let rect = Rect::new(area.x + x, area.y + y, width, u16::from(y < area.height));
            x = x.saturating_add(width).saturating_add(1);
            rect
        })
        .collect()
}
pub(crate) fn buttons<T: Clone, P: Copy + PartialEq>(
    surface: &mut Surface<'_>,
    area: Rect,
    mouse: &MouseState<T, P>,
    buttons: &[(&str, T)],
) {
    for ((label, target), rect) in buttons.iter().zip(button_rects(area, buttons)) {
        if rect.is_empty() {
            continue;
        }
        surface.render_widget(
            Paragraph::new(*label).style(Style::default().fg(theme::AMBER_GLOW())),
            rect,
        );
        mouse.hit(rect, target.clone());
    }
}

pub(crate) fn scroll<T: Clone, P: Copy + PartialEq>(
    surface: &mut Surface<'_>,
    area: Rect,
    mouse: &MouseState<T, P>,
    pane: P,
    rows: usize,
    focus: std::ops::Range<usize>,
    draw: impl FnOnce(&mut Surface<'_>, Rect),
) {
    if area.is_empty() {
        return;
    }
    let rows = rows
        .max(usize::from(area.height))
        .min(usize::from(u16::MAX));
    let offset = mouse.pane_range(area, pane, rows, focus);
    let local = Rect::new(0, 0, area.width.saturating_sub(1), rows as u16);
    if local.width == 0 {
        return;
    }
    let mut buffer = Buffer::empty(local);
    let mark = mouse.mark();
    draw(
        &mut Surface {
            buffer: &mut buffer,
        },
        local,
    );
    mouse.translate(
        mark,
        Rect {
            width: local.width,
            ..area
        },
        offset,
    );
    for y in 0..area.height {
        for x in 0..local.width {
            surface.buffer[(area.x + x, area.y + y)] = buffer[(x, y + offset as u16)].clone();
        }
    }
    if rows > usize::from(area.height) {
        let thumb = offset * usize::from(area.height.saturating_sub(1))
            / rows.saturating_sub(usize::from(area.height)).max(1);
        for y in 0..area.height {
            surface.buffer[(area.right() - 1, area.y + y)]
                .set_symbol(if usize::from(y) == thumb {
                    "█"
                } else {
                    "│"
                })
                .set_style(Style::default().fg(theme::AMBER_DIM()));
        }
    }
}

/// Each logical line carries its action, if any. Measure and paint the same
/// Paragraph, so wrapping cannot shift a target onto a different item.
pub(crate) fn line_height(line: &Line<'_>, width: u16) -> usize {
    if width == 0 {
        return 0;
    }
    Paragraph::new(line.clone())
        .wrap(Wrap { trim: false })
        .line_count(width)
        .max(1)
}
pub(crate) fn lines<T: Clone, P: Copy + PartialEq>(
    surface: &mut Surface<'_>,
    area: Rect,
    mouse: &MouseState<T, P>,
    lines: &[(Line<'static>, Option<T>)],
) {
    let mut y = area.y;
    for (line, target) in lines {
        let height = (line_height(line, area.width) as u16).min(area.bottom().saturating_sub(y));
        let rect = Rect::new(area.x, y, area.width, height);
        surface.render_widget(
            Paragraph::new(line.clone()).wrap(Wrap { trim: false }),
            rect,
        );
        if let Some(target) = target {
            mouse.hit(rect, target.clone());
        }
        y = y.saturating_add(height);
    }
}

/// Record the widget's own mapping after rendering. InViewport exposes the
/// last viewport origin without repainting or moving the live textarea.
pub(crate) fn text_hits<T: Clone, P: Copy + PartialEq>(
    input: &TextArea<'_>,
    area: Rect,
    mouse: &MouseState<T, P>,
    target: impl Fn(usize, usize) -> T,
) {
    if area.is_empty() {
        return;
    }
    let mut probe = input.clone();
    probe.cancel_selection();
    probe.move_cursor(CursorMove::Jump(0, 0));
    probe.move_cursor(CursorMove::InViewport);
    let origin = probe.screen_cursor();
    let mut positions = Vec::new();
    for (row, line) in input.lines().iter().enumerate() {
        let chars: Vec<_> = line.chars().collect();
        for col in 0..=chars.len() {
            probe.move_cursor(CursorMove::Jump(row as u16, col as u16));
            let cursor = probe.screen_cursor();
            positions.push((
                cursor.row,
                cursor.col,
                row,
                col,
                chars
                    .get(col)
                    .map_or(0, |ch| Span::raw(ch.to_string()).width()),
            ));
        }
    }
    for y in 0..area.height {
        let screen_row = origin.row + usize::from(y);
        let row_positions: Vec<_> = positions
            .iter()
            .filter(|position| position.0 == screen_row)
            .collect();
        for x in 0..area.width {
            let screen_col = origin.col + usize::from(x);
            let Some(position) = row_positions
                .iter()
                .rev()
                .find(|position| position.1 <= screen_col)
                .copied()
                .or_else(|| row_positions.first().copied())
                .or_else(|| positions.last())
            else {
                continue;
            };
            let (_, start, row, col, glyph_width) = *position;
            let col = if glyph_width > 0 && screen_col >= start + glyph_width {
                col + 1
            } else {
                col
            };
            mouse.hit(Rect::new(area.x + x, area.y + y, 1, 1), target(row, col));
        }
    }
}

#[cfg(test)]
#[path = "mouse_ui_test.rs"]
mod tests;
