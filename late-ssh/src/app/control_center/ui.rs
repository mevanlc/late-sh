use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::app::common::theme;

use super::state::{BanPromptField, Tab};

pub struct RoomPromptView<'a> {
    pub panel_title: &'a str,
    pub title: &'a str,
    pub value: &'a str,
}

pub struct BanPromptView<'a> {
    pub username: &'a str,
    pub reason: &'a str,
    pub duration: &'a str,
    pub focus: BanPromptField,
}

pub struct ControlCenterView<'a> {
    pub selected_tab: Tab,
    pub username: &'a str,
    pub is_admin: bool,
    pub is_moderator: bool,
    pub online_count: usize,
    pub live_session_count: usize,
    pub user_list_lines: &'a [String],
    pub user_detail_lines: &'a [String],
    pub selected_user_name: Option<&'a str>,
    pub user_filter: &'a str,
    pub user_filter_focused: bool,
    pub room_list_lines: &'a [String],
    pub room_detail_lines: &'a [String],
    pub staff_list_lines: &'a [String],
    pub staff_detail_lines: &'a [String],
    pub audit_list_lines: &'a [String],
    pub audit_detail_lines: &'a [String],
    pub audit_filter: &'a str,
    pub audit_filter_focused: bool,
    pub room_prompt: Option<RoomPromptView<'a>>,
    pub ban_prompt: Option<BanPromptView<'a>>,
}

pub fn draw_control_center(frame: &mut Frame, area: Rect, view: &ControlCenterView<'_>) {
    let layout = Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(2),
    ])
    .split(area);
    draw_active_panel(frame, layout[1], view);
    draw_hint_row(frame, layout[2], view);
}

fn build_tab_title(view: &ControlCenterView<'_>) -> Line<'static> {
    let tabs: &[(Tab, &str, &str)] = &[
        (Tab::Status, "1", "Status"),
        (Tab::Users, "2", "Users"),
        (Tab::Rooms, "3", "Rooms"),
        (Tab::Staff, "4", "Staff"),
        (Tab::Log, "5", "Log"),
    ];
    let sep_style = Style::default().fg(theme::BORDER_DIM());
    let mut spans: Vec<Span<'static>> = vec![Span::raw(" ")];
    for (i, (tab, num, label)) in tabs.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" · ", sep_style));
        }
        let is_selected = *tab == view.selected_tab;
        let num_style = if is_selected {
            Style::default().fg(theme::AMBER())
        } else {
            Style::default().fg(theme::TEXT_FAINT())
        };
        spans.push(Span::styled(num.to_string(), num_style));
        if is_selected {
            let label_style = Style::default()
                .fg(theme::AMBER())
                .bg(theme::BG_HIGHLIGHT())
                .add_modifier(Modifier::BOLD);
            spans.push(Span::styled(" [ ", sep_style));
            spans.push(Span::styled(label.to_string(), label_style));
            spans.push(Span::styled(" ]", sep_style));
        } else {
            spans.push(Span::styled(
                format!(" {label}"),
                Style::default().fg(theme::TEXT_DIM()),
            ));
        }
    }
    spans.push(Span::raw(" "));
    Line::from(spans)
}

fn hint_text(view: &ControlCenterView<'_>) -> &'static str {
    match view.selected_tab {
        _ if view.room_prompt.is_some() => "type @user · Enter confirm · Esc cancel",
        _ if view.ban_prompt.is_some() => "Tab switch field · Enter confirm · Esc cancel",
        Tab::Status => "h/l or ←/→ switch tabs · 1-5 jump to section",
        Tab::Users if view.user_filter_focused => {
            "Type to filter · ↓ enter list · Esc clear · ←/→ switch tabs"
        }
        Tab::Users if view.is_admin => {
            "j/k or ↑/↓ move · ^F filter · x disconnect · b ban · u unban · m grant mod"
        }
        Tab::Users => "j/k or ↑/↓ move · ^F filter · x disconnect · b ban · u unban",
        Tab::Rooms => {
            "j/k or ↑/↓ move · x kick · b ban · u unban · r rename · p public · v private · d delete"
        }
        Tab::Staff if view.is_admin => "j/k or ↑/↓ move · g grant admin · r revoke mod",
        Tab::Staff => "j/k or ↑/↓ move",
        Tab::Log if view.audit_filter_focused => {
            "Type to filter · ↓ enter list · Esc clear · ^R reset · ←/→ switch tabs"
        }
        Tab::Log => "j/k or ↑/↓ move · ^F filter",
    }
}

fn draw_hint_row(frame: &mut Frame, area: Rect, view: &ControlCenterView<'_>) {
    frame.render_widget(
        Paragraph::new(hint_text(view))
            .style(Style::default().fg(theme::TEXT_FAINT()))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn draw_active_panel(frame: &mut Frame, area: Rect, view: &ControlCenterView<'_>) {
    let block = Block::default()
        .title(build_tab_title(view))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    match view.selected_tab {
        Tab::Status => draw_status_panel(frame, inner, view),
        Tab::Users => draw_user_panel(frame, inner, view),
        Tab::Rooms => draw_rooms_panel(
            frame,
            inner,
            view.room_list_lines,
            view.room_detail_lines,
            view.room_prompt.as_ref(),
        ),
        Tab::Staff => {
            draw_staff_panel(frame, inner, view.staff_list_lines, view.staff_detail_lines)
        }
        Tab::Log => draw_audit_panel(
            frame,
            inner,
            view.audit_list_lines,
            view.audit_detail_lines,
            view.audit_filter,
            view.audit_filter_focused,
        ),
    }
}

fn draw_status_panel(frame: &mut Frame, area: Rect, view: &ControlCenterView<'_>) {
    let columns = Layout::horizontal([Constraint::Fill(1), Constraint::Fill(1)]).split(area);
    draw_summary_card(
        frame,
        columns[0],
        "Access",
        &[
            Line::from(vec![
                Span::styled("@", Style::default().fg(theme::TEXT_DIM())),
                Span::styled(
                    view.username.to_string(),
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
                    "User"
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

fn draw_audit_panel(
    frame: &mut Frame,
    area: Rect,
    audit_list_lines: &[String],
    audit_detail_lines: &[String],
    audit_filter: &str,
    audit_filter_focused: bool,
) {
    let columns =
        Layout::horizontal([Constraint::Percentage(60), Constraint::Percentage(40)]).split(area);
    draw_audit_entries_card(
        frame,
        columns[0],
        audit_list_lines,
        audit_filter,
        audit_filter_focused,
    );
    draw_panel_card(frame, columns[1], "Entry detail", audit_detail_lines, false);
}

fn draw_audit_entries_card(
    frame: &mut Frame,
    area: Rect,
    audit_list_lines: &[String],
    audit_filter: &str,
    audit_filter_focused: bool,
) {
    let border_style = if audit_filter_focused {
        Style::default().fg(theme::BORDER_ACTIVE())
    } else {
        Style::default().fg(theme::BORDER())
    };
    let block = Block::default()
        .title(" Entries ")
        .borders(Borders::ALL)
        .border_style(border_style);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let layout = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .split(inner);

    frame.render_widget(
        Paragraph::new(audit_filter_line(audit_filter, audit_filter_focused)),
        layout[0],
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "─".repeat(layout[1].width as usize),
            Style::default().fg(theme::BORDER()),
        ))),
        layout[1],
    );
    let body_lines: Vec<Line<'_>> = audit_list_lines
        .iter()
        .map(|line| {
            Line::from(Span::styled(
                line.as_str(),
                Style::default().fg(theme::TEXT()),
            ))
        })
        .collect();
    frame.render_widget(
        Paragraph::new(body_lines).wrap(Wrap { trim: true }),
        layout[2],
    );
}

fn audit_filter_line(value: &str, focused: bool) -> Line<'static> {
    let label_style = if focused {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };
    let caret_style = if focused {
        Style::default().fg(theme::AMBER_GLOW())
    } else {
        Style::default().fg(theme::AMBER_DIM())
    };
    let value_style = if focused {
        Style::default().fg(theme::TEXT_BRIGHT())
    } else {
        Style::default().fg(theme::TEXT())
    };
    let mut spans = vec![
        Span::styled("filter ^F".to_string(), label_style),
        Span::raw(" "),
        Span::styled("> ".to_string(), caret_style),
    ];
    if value.is_empty() {
        spans.push(Span::styled(
            "actor:@alice target:@troll action:ban since:2026-04-20".to_string(),
            Style::default().fg(theme::TEXT_FAINT()),
        ));
    } else {
        spans.push(Span::styled(value.to_string(), value_style));
        if focused {
            spans.push(Span::styled(
                "_".to_string(),
                Style::default().fg(theme::AMBER_GLOW()),
            ));
        }
    }
    Line::from(spans)
}

fn draw_staff_panel(
    frame: &mut Frame,
    area: Rect,
    staff_list_lines: &[String],
    staff_detail_lines: &[String],
) {
    let columns =
        Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)]).split(area);
    draw_panel_card(frame, columns[0], "Staff", staff_list_lines, false);
    draw_panel_card(
        frame,
        columns[1],
        "Selected Staffer",
        staff_detail_lines,
        false,
    );
}

fn draw_user_panel(frame: &mut Frame, area: Rect, view: &ControlCenterView<'_>) {
    let ban_prompt = view.ban_prompt.as_ref();
    let columns = if ban_prompt.is_some() {
        Layout::horizontal([
            Constraint::Percentage(25),
            Constraint::Percentage(33),
            Constraint::Percentage(22),
            Constraint::Percentage(20),
        ])
        .split(area)
    } else {
        Layout::horizontal([
            Constraint::Percentage(28),
            Constraint::Percentage(42),
            Constraint::Percentage(30),
        ])
        .split(area)
    };

    draw_user_directory_card(
        frame,
        columns[0],
        view.user_list_lines,
        view.user_filter,
        view.user_filter_focused,
    );

    let detail_title = view
        .selected_user_name
        .map(|name| format!(" @{name} "))
        .unwrap_or_else(|| " User Detail ".to_string());
    draw_panel_card(
        frame,
        columns[1],
        &detail_title,
        view.user_detail_lines,
        false,
    );

    if let Some(prompt) = ban_prompt {
        draw_panel_card(
            frame,
            columns[2],
            "Actions",
            &actions_lines(view.is_admin),
            false,
        );
        draw_ban_prompt_card(frame, columns[3], prompt);
    } else {
        draw_actions_panel(frame, columns[2], view.is_admin);
    }
}

fn draw_user_directory_card(
    frame: &mut Frame,
    area: Rect,
    user_list_lines: &[String],
    user_filter: &str,
    user_filter_focused: bool,
) {
    let border_style = if user_filter_focused {
        Style::default().fg(theme::BORDER_ACTIVE())
    } else {
        Style::default().fg(theme::BORDER())
    };
    let block = Block::default()
        .title(" User Directory ")
        .borders(Borders::ALL)
        .border_style(border_style);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let layout = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .split(inner);

    frame.render_widget(
        Paragraph::new(user_filter_line(user_filter, user_filter_focused)),
        layout[0],
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "─".repeat(layout[1].width as usize),
            Style::default().fg(theme::BORDER()),
        ))),
        layout[1],
    );
    let body_lines: Vec<Line<'_>> = user_list_lines
        .iter()
        .map(|line| {
            Line::from(Span::styled(
                line.as_str(),
                Style::default().fg(theme::TEXT()),
            ))
        })
        .collect();
    frame.render_widget(
        Paragraph::new(body_lines).wrap(Wrap { trim: true }),
        layout[2],
    );
}

fn user_filter_line(value: &str, focused: bool) -> Line<'static> {
    let label_style = if focused {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };
    let caret_style = if focused {
        Style::default().fg(theme::AMBER_GLOW())
    } else {
        Style::default().fg(theme::AMBER_DIM())
    };
    let value_style = if focused {
        Style::default().fg(theme::TEXT_BRIGHT())
    } else {
        Style::default().fg(theme::TEXT())
    };
    let mut spans = vec![
        Span::styled("filter ^F".to_string(), label_style),
        Span::raw(" "),
        Span::styled("> ".to_string(), caret_style),
    ];
    if value.is_empty() {
        spans.push(Span::styled(
            "@username".to_string(),
            Style::default().fg(theme::TEXT_FAINT()),
        ));
    } else {
        spans.push(Span::styled(value.to_string(), value_style));
        if focused {
            spans.push(Span::styled(
                "_".to_string(),
                Style::default().fg(theme::AMBER_GLOW()),
            ));
        }
    }
    Line::from(spans)
}

fn actions_lines(is_admin: bool) -> Vec<String> {
    let mut lines = vec![
        "s  Sanction history".to_string(),
        "c  Clear profile".to_string(),
        "a  View audit trail".to_string(),
        "!  Warn user".to_string(),
        "k  Kick (server)".to_string(),
        "r  Recent chats".to_string(),
        "b  Ban\u{2026}".to_string(),
        "u  Unban".to_string(),
        ">  Open DM".to_string(),
        "p  View profile".to_string(),
    ];
    if is_admin {
        lines.push("m  Grant mod".to_string());
    }
    lines
}

fn draw_actions_panel(frame: &mut Frame, area: Rect, is_admin: bool) {
    let block = Block::default()
        .title(" Actions ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let implemented = ["b", "u", "m"];
    let lines: Vec<Line<'_>> = actions_lines(is_admin)
        .into_iter()
        .map(|line| {
            let key = line.split_whitespace().next().unwrap_or("");
            let style = if implemented.contains(&key) {
                Style::default().fg(theme::TEXT())
            } else {
                Style::default().fg(theme::TEXT_FAINT())
            };
            Line::from(Span::styled(line, style))
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_ban_prompt_card(frame: &mut Frame, area: Rect, prompt: &BanPromptView<'_>) {
    let block = Block::default()
        .title(" Ban User ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let reason_focused = matches!(prompt.focus, BanPromptField::Reason);
    let duration_focused = matches!(prompt.focus, BanPromptField::Duration);

    let lines = vec![
        Line::from(Span::styled(
            format!("target: @{}", prompt.username),
            Style::default().fg(theme::TEXT_BRIGHT()),
        )),
        Line::from(Span::raw("")),
        ban_field_header("reason (required)", reason_focused),
        ban_field_line(prompt.reason, reason_focused, "what happened?"),
        Line::from(Span::raw("")),
        ban_field_header("duration (blank = permanent)", duration_focused),
        ban_field_line(prompt.duration, duration_focused, "e.g. 1h, 24h, 7d"),
        Line::from(Span::raw("")),
        Line::from(Span::styled(
            "Tab switch · Enter confirm · Esc cancel",
            Style::default().fg(theme::TEXT_FAINT()),
        )),
    ];
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn ban_field_header(label: &str, focused: bool) -> Line<'static> {
    let style = if focused {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };
    Line::from(Span::styled(label.to_string(), style))
}

fn ban_field_line(value: &str, focused: bool, placeholder: &str) -> Line<'static> {
    let caret_style = if focused {
        Style::default().fg(theme::AMBER_GLOW())
    } else {
        Style::default().fg(theme::AMBER_DIM())
    };
    let value_style = if focused {
        Style::default().fg(theme::TEXT_BRIGHT())
    } else {
        Style::default().fg(theme::TEXT())
    };
    if value.is_empty() {
        Line::from(vec![
            Span::styled("> ".to_string(), caret_style),
            Span::styled(
                placeholder.to_string(),
                Style::default().fg(theme::TEXT_FAINT()),
            ),
        ])
    } else {
        Line::from(vec![
            Span::styled("> ".to_string(), caret_style),
            Span::styled(value.to_string(), value_style),
        ])
    }
}

fn draw_rooms_panel(
    frame: &mut Frame,
    area: Rect,
    room_list_lines: &[String],
    room_detail_lines: &[String],
    room_prompt: Option<&RoomPromptView<'_>>,
) {
    let columns = if room_prompt.is_some() {
        Layout::horizontal([
            Constraint::Percentage(36),
            Constraint::Percentage(42),
            Constraint::Percentage(22),
        ])
        .split(area)
    } else {
        Layout::horizontal([Constraint::Percentage(48), Constraint::Percentage(52)]).split(area)
    };
    draw_panel_card(frame, columns[0], "Room Directory", room_list_lines, false);
    draw_panel_card(frame, columns[1], "Selected Room", room_detail_lines, false);
    if let Some(prompt) = room_prompt {
        let value_prefix = if prompt.panel_title == "Admin Action" {
            "#"
        } else {
            "@"
        };
        let value_label = if prompt.panel_title == "Admin Action" {
            format!("{} room", prompt.title)
        } else {
            format!("{} target", prompt.title)
        };
        let prompt_lines = vec![
            value_label,
            String::new(),
            format!("{}{}", value_prefix, prompt.value),
            String::new(),
            "Enter confirms".to_string(),
            "Esc cancels".to_string(),
        ];
        draw_panel_card(frame, columns[2], prompt.panel_title, &prompt_lines, false);
    }
}

fn draw_panel_card(frame: &mut Frame, area: Rect, title: &str, lines: &[String], active: bool) {
    let block = Block::default()
        .title(format!(" {title} "))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if active {
            theme::BORDER_ACTIVE()
        } else {
            theme::BORDER()
        }));
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
