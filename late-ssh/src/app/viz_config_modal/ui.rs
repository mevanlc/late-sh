use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Flex, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::app::{common::theme, visualizer::Visualizer};

use super::state::{EditState, Field, RowHitGeom, VizConfigModalState};

const MODAL_WIDTH: u16 = 62;
const MODAL_HEIGHT: u16 = 16;
const LABEL_WIDTH: u16 = 10;
const VALUE_WIDTH: u16 = 12;
const ROW_LEFT_PAD: u16 = 2;
const TRI_GAP: u16 = 1; // space between triangle and adjacent text/triangle

pub fn draw(frame: &mut Frame, area: Rect, state: &VizConfigModalState, viz: &Visualizer) {
    let popup = centered_rect(MODAL_WIDTH, MODAL_HEIGHT, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Visualizer Config ")
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let layout = Layout::vertical([
        Constraint::Length(1), // top spacer
        Constraint::Min(6),    // fields (6 rows)
        Constraint::Length(1), // spacer
        Constraint::Length(3), // footer (top divider + 2 content rows)
    ])
    .split(inner);

    let fields_area = layout[1];
    let focused = state.focused();
    let editing = state.editing();

    let fields = state.fields().to_vec();
    let mut row_geoms: Vec<(Field, RowHitGeom)> = Vec::with_capacity(fields.len());
    for (i, &field) in fields.iter().enumerate() {
        let row_y = fields_area.y + i as u16;
        if row_y >= fields_area.y + fields_area.height {
            break;
        }
        let row_rect = Rect {
            x: fields_area.x,
            y: row_y,
            width: fields_area.width,
            height: 1,
        };
        let geom = draw_row(frame, row_rect, field, field == focused, viz, editing);
        row_geoms.push((field, geom));
    }

    state.record_geometry(popup, &row_geoms);

    draw_footer(frame, layout[3]);
}

fn draw_row(
    frame: &mut Frame,
    area: Rect,
    field: Field,
    focused: bool,
    viz: &Visualizer,
    editing: Option<&EditState>,
) -> RowHitGeom {
    let editing_this = editing.map(|e| e.field) == Some(field);
    let label_style = if focused {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };
    let tri_style = Style::default().fg(if focused {
        theme::AMBER_GLOW()
    } else {
        theme::AMBER_DIM()
    });
    let value_style = Style::default()
        .fg(theme::AMBER())
        .add_modifier(if editing_this { Modifier::UNDERLINED } else { Modifier::empty() });

    // Layout: [marker(2)][label(10)][gap][◀][gap][▼][gap][value(12)][gap][▲][gap][▶]
    let marker = if focused { "▸ " } else { "  " };
    let label_text = format!("{:<width$}", field.label(), width = LABEL_WIDTH as usize);

    let value_text = format_value(field, viz, editing.filter(|e| e.field == field));

    let line = Line::from(vec![
        Span::styled(marker, label_style),
        Span::styled(label_text, label_style),
        Span::raw(" "),
        Span::styled("◀", tri_style),
        Span::raw(" "),
        Span::styled("▼", tri_style),
        Span::raw(" "),
        Span::styled(value_text, value_style),
        Span::raw(" "),
        Span::styled("▲", tri_style),
        Span::raw(" "),
        Span::styled("▶", tri_style),
    ]);
    frame.render_widget(Paragraph::new(line), area);

    // Compute X positions for hit-test, mirroring the span widths above.
    let label_x = area.x + ROW_LEFT_PAD;
    let mut x = label_x + LABEL_WIDTH + TRI_GAP;
    let small_dec_x = x;
    x += 1 + TRI_GAP;
    let large_dec_x = x;
    x += 1 + TRI_GAP;
    let value_x = x;
    x = value_x + VALUE_WIDTH + TRI_GAP;
    let large_inc_x = x;
    x += 1 + TRI_GAP;
    let small_inc_x = x;

    RowHitGeom {
        y: area.y,
        label_x,
        label_width: LABEL_WIDTH,
        small_dec_x,
        large_dec_x,
        large_inc_x,
        small_inc_x,
    }
}

fn format_value(field: Field, viz: &Visualizer, editing: Option<&EditState>) -> String {
    let width = VALUE_WIDTH as usize;
    if let Some(state) = editing {
        let with_cursor = format!("{}_", state.buffer);
        return format!("{:^width$}", with_cursor, width = width);
    }
    let text = match field {
        Field::Scale => format!("{:.2}", viz.scale()),
        Field::Mode => viz.mode().label().to_string(),
        Field::Gain => format!("{:.2}", viz.gain()),
        Field::Attack => format!("{:.2}", viz.attack()),
        Field::Release => format!("{:.2}", viz.release()),
        Field::Tilt => if viz.tilt_enabled() { "on" } else { "off" }.to_string(),
    };
    format!("{:^width$}", text, width = width)
}

fn draw_footer(frame: &mut Frame, area: Rect) {
    let footer_block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = footer_block.inner(area);
    frame.render_widget(footer_block, area);

    let cells = [
        ("⇥ / S+⇥", "Focus"),
        ("↑↓", "Large Step"),
        ("←→", "Small Step"),
        ("⏎", "Edit Number"),
        ("Esc", "close"),
    ];
    let widths = [
        Constraint::Length(9),
        Constraint::Length(1),
        Constraint::Length(12),
        Constraint::Length(1),
        Constraint::Length(12),
        Constraint::Length(1),
        Constraint::Length(13),
        Constraint::Length(1),
        Constraint::Min(7),
    ];
    let columns = Layout::horizontal(widths).split(inner);

    let key_style = Style::default()
        .fg(theme::AMBER_DIM())
        .add_modifier(Modifier::BOLD);
    let label_style = Style::default().fg(theme::TEXT_DIM());
    let sep_style = Style::default().fg(theme::BORDER_ACTIVE());

    let sep_paragraph = Paragraph::new(vec![
        Line::from(Span::styled("│", sep_style)),
        Line::from(Span::styled("│", sep_style)),
    ]);

    for (cell_idx, (key, label)) in cells.iter().enumerate() {
        let column = columns[cell_idx * 2];
        let key_line =
            Line::from(Span::styled((*key).to_string(), key_style)).alignment(Alignment::Center);
        let label_line =
            Line::from(Span::styled((*label).to_string(), label_style)).alignment(Alignment::Center);
        frame.render_widget(Paragraph::new(vec![key_line, label_line]), column);

        if cell_idx > 0 {
            let sep_col = columns[cell_idx * 2 - 1];
            frame.render_widget(sep_paragraph.clone(), sep_col);
        }
    }
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Length(height.min(area.height))])
        .flex(Flex::Center)
        .split(area);
    let horizontal = Layout::horizontal([Constraint::Length(width.min(area.width))])
        .flex(Flex::Center)
        .split(vertical[0]);
    horizontal[0]
}
