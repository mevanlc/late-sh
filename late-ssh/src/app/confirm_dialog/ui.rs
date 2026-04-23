use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::app::common::theme;

use super::state::ConfirmDialogState;

const MODAL_WIDTH: u16 = 68;
const MODAL_HEIGHT: u16 = 12;

pub fn draw(frame: &mut Frame, area: Rect, state: &ConfirmDialogState) {
    let popup = centered_rect(MODAL_WIDTH, MODAL_HEIGHT, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(format!(" {} ", state.title))
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
        Constraint::Length(2),
        Constraint::Length(2),
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(inner);

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            state.prompt.as_str(),
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        )))
        .centered(),
        layout[1],
    );

    frame.render_widget(
        Paragraph::new(state.detail.as_str())
            .wrap(Wrap { trim: true })
            .centered(),
        layout[2],
    );

    let field_block = Block::default()
        .title(" Confirm ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if state.is_confirm_enabled() {
            theme::BORDER_ACTIVE()
        } else {
            theme::BORDER()
        }));
    let field_inner = field_block.inner(layout[3]);
    frame.render_widget(field_block, layout[3]);
    let expected = state.required_text.as_deref().unwrap_or("confirm");
    let field_line = Line::from(vec![
        Span::styled("> ", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(
            state.input_value.as_str(),
            Style::default().fg(theme::TEXT_BRIGHT()),
        ),
        Span::styled(
            if state.input_value.is_empty() {
                expected
            } else {
                ""
            },
            Style::default().fg(theme::TEXT_FAINT()),
        ),
    ]);
    frame.render_widget(Paragraph::new(field_line), field_inner);

    let footer_cols = Layout::horizontal([
        Constraint::Length(2),
        Constraint::Fill(1),
        Constraint::Fill(1),
        Constraint::Length(2),
    ])
    .split(layout[4]);

    let confirm = Line::from(vec![
        Span::styled("Enter", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(
            format!(" {}", state.confirm_label),
            Style::default().fg(if state.is_confirm_enabled() {
                theme::ERROR()
            } else {
                theme::TEXT_FAINT()
            }),
        ),
    ]);
    frame.render_widget(Paragraph::new(confirm), footer_cols[1]);

    let cancel = Line::from(vec![
        Span::styled("Esc", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(
            format!(" {}", state.cancel_label),
            Style::default().fg(theme::TEXT_DIM()),
        ),
    ]);
    frame.render_widget(Paragraph::new(cancel).right_aligned(), footer_cols[2]);
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
