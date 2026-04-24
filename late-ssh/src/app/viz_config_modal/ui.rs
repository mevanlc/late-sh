use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::app::{common::theme, visualizer::Visualizer};

use super::state::{Field, VizConfigModalState};

const MODAL_WIDTH: u16 = 58;
const MODAL_HEIGHT: u16 = 13;
const LABEL_WIDTH: usize = 10;

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
        Constraint::Length(1),
        Constraint::Min(5),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(inner);

    let focused = state.focused();
    let rows: Vec<Line<'static>> = state
        .fields()
        .iter()
        .map(|&f| build_row(f, f == focused, viz))
        .collect();
    frame.render_widget(Paragraph::new(rows), layout[1]);

    let footer = Line::from(vec![
        Span::styled("  Tab", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" move · ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("←/→", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" adjust · ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("Esc/F12", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" close", Style::default().fg(theme::TEXT_DIM())),
    ]);
    frame.render_widget(Paragraph::new(footer), layout[3]);
}

fn build_row(field: Field, focused: bool, viz: &Visualizer) -> Line<'static> {
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

    Line::from(vec![
        Span::styled(label_text, label_style),
        Span::styled("< ", bracket_style),
        Span::styled(format_value(field, viz), value_style),
        Span::styled(" >", bracket_style),
    ])
}

fn format_value(field: Field, viz: &Visualizer) -> String {
    match field {
        Field::Mode => viz.mode().label().to_string(),
        Field::Gain => format!("{:.2}", viz.gain()),
        Field::Attack => format!("{:.2}", viz.attack()),
        Field::Release => format!("{:.2}", viz.release()),
        Field::Tilt => if viz.tilt_enabled() { "on" } else { "off" }.to_string(),
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
