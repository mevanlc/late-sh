use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Flex, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::app::{common::theme, visualizer::Visualizer};

use super::state::{EditState, Field, VizConfigModalState};

const MODAL_WIDTH: u16 = 62;
const MODAL_HEIGHT: u16 = 14;
const LABEL_WIDTH: usize = 10;
const VALUE_WIDTH: usize = 12;

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
        Constraint::Min(5),    // fields
        Constraint::Length(1), // spacer
        Constraint::Length(3), // footer (top divider + 2 content rows)
    ])
    .split(inner);

    let focused = state.focused();
    let editing = state.editing();
    let rows: Vec<Line<'static>> = state
        .fields()
        .iter()
        .map(|&f| build_row(f, f == focused, viz, editing))
        .collect();
    frame.render_widget(Paragraph::new(rows), layout[1]);

    draw_footer(frame, layout[3]);
}

fn build_row(
    field: Field,
    focused: bool,
    viz: &Visualizer,
    editing: Option<&EditState>,
) -> Line<'static> {
    let editing_this = editing.map(|e| e.field) == Some(field);
    let label_style = if focused {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };
    let value_style = Style::default().fg(theme::AMBER());
    let bracket_style = Style::default().fg(if focused {
        theme::AMBER_GLOW()
    } else {
        theme::TEXT_DIM()
    });

    let marker = if focused { "▸ " } else { "  " };
    let label_text = format!("{}{:<width$}", marker, field.label(), width = LABEL_WIDTH);

    let (lb, rb) = if editing_this { ("[ ", " ]") } else { ("< ", " >") };
    let value_text = format_value(field, viz, editing.filter(|e| e.field == field));

    Line::from(vec![
        Span::styled(label_text, label_style),
        Span::styled(lb, bracket_style),
        Span::styled(value_text, value_style),
        Span::styled(rb, bracket_style),
    ])
}

fn format_value(field: Field, viz: &Visualizer, editing: Option<&EditState>) -> String {
    if let Some(state) = editing {
        // Show buffer with a trailing underscore "cursor", centered in the value column.
        let with_cursor = format!("{}_", state.buffer);
        return format!("{:^width$}", with_cursor, width = VALUE_WIDTH);
    }
    let text = match field {
        Field::Mode => viz.mode().label().to_string(),
        Field::Gain => format!("{:.2}", viz.gain()),
        Field::Attack => format!("{:.2}", viz.attack()),
        Field::Release => format!("{:.2}", viz.release()),
        Field::Tilt => if viz.tilt_enabled() { "on" } else { "off" }.to_string(),
    };
    format!("{:^width$}", text, width = VALUE_WIDTH)
}

fn draw_footer(frame: &mut Frame, area: Rect) {
    let footer_block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = footer_block.inner(area);
    frame.render_widget(footer_block, area);

    // Five content cells with dedicated 1-col separator cells between them.
    // Lengths sum to <= inner width; the last cell is `Min(...)` so it
    // absorbs any leftover space.
    let cells = [
        ("⇥ / S+⇥", "Focus"),
        ("↑↓", "Large Step"),
        ("←→", "Small Step"),
        ("⏎", "Edit Number"),
        ("Esc", "close"),
    ];
    let widths = [
        Constraint::Length(9),  // Focus
        Constraint::Length(1),  // sep
        Constraint::Length(12), // Large Step
        Constraint::Length(1),  // sep
        Constraint::Length(12), // Small Step
        Constraint::Length(1),  // sep
        Constraint::Length(13), // Edit Number
        Constraint::Length(1),  // sep
        Constraint::Min(7),     // close (absorbs leftover)
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
