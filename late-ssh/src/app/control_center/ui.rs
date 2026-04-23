use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::app::common::theme;

use super::state::{Focus, Tab};

pub struct ControlCenterView<'a> {
    pub selected_tab: Tab,
    pub focus: Focus,
    pub username: &'a str,
    pub is_admin: bool,
    pub is_moderator: bool,
    pub online_count: usize,
    pub live_session_count: usize,
    pub user_list_lines: &'a [String],
    pub user_detail_lines: &'a [String],
    pub room_list_lines: &'a [String],
    pub room_detail_lines: &'a [String],
    pub room_prompt_panel_title: Option<&'a str>,
    pub room_prompt_title: Option<&'a str>,
    pub room_prompt_value: Option<&'a str>,
}

pub fn draw_control_center(frame: &mut Frame, area: Rect, view: &ControlCenterView<'_>) {
    let layout = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(5),
        Constraint::Fill(1),
    ])
    .split(area);

    draw_tab_row(frame, layout[0], view);
    draw_summary(frame, layout[1], view);
    draw_active_panel(frame, layout[2], view);
}

fn draw_tab_row(frame: &mut Frame, area: Rect, view: &ControlCenterView<'_>) {
    let selected = view.selected_tab;
    let tabs = [Tab::Users, Tab::Rooms];
    let mut spans = vec![Span::styled(
        " Staff Control Center ",
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD),
    )];
    spans.push(Span::styled("· ", Style::default().fg(theme::BORDER_DIM())));
    spans.push(Span::styled(
        "0 hidden entry",
        Style::default().fg(theme::TEXT_DIM()),
    ));
    spans.push(Span::raw("   "));

    for tab in tabs {
        let style = if tab == selected && view.focus == Focus::Tabs {
            Style::default()
                .fg(theme::AMBER_GLOW())
                .bg(theme::BG_HIGHLIGHT())
                .add_modifier(Modifier::BOLD)
        } else if tab == selected {
            Style::default()
                .fg(theme::AMBER())
                .bg(theme::BG_HIGHLIGHT())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT_DIM())
        };
        spans.push(Span::styled(format!(" {} ", tab.label()), style));
        spans.push(Span::raw(" "));
    }

    let help_text = match (view.focus, selected) {
        (_, Tab::Rooms) if view.room_prompt_title.is_some() => {
            "type @user · Enter confirm · Esc cancel"
        }
        (Focus::Tabs, _) => "Tab focus pane · h/l or ←/→ switch tabs",
        (Focus::ActivePane, Tab::Users) if view.is_admin => {
            "Tab focus tabs · j/k or ↑/↓ move · x disconnect"
        }
        (Focus::ActivePane, Tab::Users) => "Tab focus tabs · j/k or ↑/↓ move",
        (Focus::ActivePane, Tab::Rooms) => {
            "Tab focus tabs · j/k or ↑/↓ move · x kick · b ban · u unban · r rename · p public · v private · d delete"
        }
    };
    spans.push(Span::styled(
        help_text,
        Style::default().fg(theme::TEXT_FAINT()),
    ));

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_summary(frame: &mut Frame, area: Rect, view: &ControlCenterView<'_>) {
    let columns = Layout::horizontal([Constraint::Fill(1), Constraint::Fill(1)]).split(area);
    draw_summary_card(
        frame,
        columns[0],
        "Access",
        &[
            Line::from(vec![
                Span::styled("@", Style::default().fg(theme::TEXT_DIM())),
                Span::styled(
                    view.username,
                    Style::default()
                        .fg(theme::TEXT_BRIGHT())
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(Span::styled(
                if view.is_admin {
                    "Administrator"
                } else if view.is_moderator {
                    "Moderator"
                } else {
                    "Unavailable"
                },
                Style::default().fg(theme::AMBER()),
            )),
        ],
    );
    draw_summary_card(
        frame,
        columns[1],
        "Runtime",
        &[
            Line::from(vec![
                Span::styled("Users online: ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled(
                    view.online_count.to_string(),
                    Style::default()
                        .fg(theme::TEXT_BRIGHT())
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled("Live sessions: ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled(
                    view.live_session_count.to_string(),
                    Style::default()
                        .fg(theme::TEXT_BRIGHT())
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
        ],
    );
}

fn draw_summary_card(frame: &mut Frame, area: Rect, title: &str, lines: &[Line<'_>]) {
    let block = Block::default()
        .title(format!(" {title} "))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER()));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(lines.to_vec()).wrap(Wrap { trim: true }),
        inner,
    );
}

fn draw_active_panel(frame: &mut Frame, area: Rect, view: &ControlCenterView<'_>) {
    let title = format!(" {} ", view.selected_tab.label());
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if view.focus == Focus::ActivePane {
            theme::BORDER_ACTIVE()
        } else {
            theme::BORDER()
        }));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    match view.selected_tab {
        Tab::Users => draw_user_panel(frame, inner, view.user_list_lines, view.user_detail_lines),
        Tab::Rooms => draw_rooms_panel(
            frame,
            inner,
            view.room_list_lines,
            view.room_detail_lines,
            view.room_prompt_panel_title,
            view.room_prompt_title,
            view.room_prompt_value,
        ),
    }
}

fn draw_user_panel(
    frame: &mut Frame,
    area: Rect,
    user_list_lines: &[String],
    user_detail_lines: &[String],
) {
    let columns =
        Layout::horizontal([Constraint::Percentage(42), Constraint::Percentage(58)]).split(area);
    draw_panel_card(frame, columns[0], "User Directory", user_list_lines);
    draw_panel_card(frame, columns[1], "Selected User", user_detail_lines);
}

fn draw_rooms_panel(
    frame: &mut Frame,
    area: Rect,
    room_list_lines: &[String],
    room_detail_lines: &[String],
    room_prompt_panel_title: Option<&str>,
    room_prompt_title: Option<&str>,
    room_prompt_value: Option<&str>,
) {
    let columns = if room_prompt_panel_title.is_some() {
        Layout::horizontal([
            Constraint::Percentage(36),
            Constraint::Percentage(42),
            Constraint::Percentage(22),
        ])
        .split(area)
    } else {
        Layout::horizontal([Constraint::Percentage(48), Constraint::Percentage(52)]).split(area)
    };
    draw_panel_card(frame, columns[0], "Room Directory", room_list_lines);
    draw_panel_card(frame, columns[1], "Selected Room", room_detail_lines);
    if let (Some(panel_title), Some(title)) = (room_prompt_panel_title, room_prompt_title) {
        let value_prefix = if panel_title == "Admin Action" {
            "#"
        } else {
            "@"
        };
        let value_label = if panel_title == "Admin Action" {
            format!("{} room", title)
        } else {
            format!("{} target", title)
        };
        let prompt_lines = vec![
            value_label,
            String::new(),
            format!("{}{}", value_prefix, room_prompt_value.unwrap_or("")),
            String::new(),
            "Enter confirms".to_string(),
            "Esc cancels".to_string(),
        ];
        draw_panel_card(frame, columns[2], panel_title, &prompt_lines);
    }
}

fn draw_panel_card(frame: &mut Frame, area: Rect, title: &str, lines: &[String]) {
    let block = Block::default()
        .title(format!(" {title} "))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let text_lines: Vec<Line<'_>> = lines
        .iter()
        .map(|line| {
            Line::from(Span::styled(
                line.as_str(),
                Style::default().fg(theme::TEXT()),
            ))
        })
        .collect();
    frame.render_widget(Paragraph::new(text_lines).wrap(Wrap { trim: true }), inner);
}
