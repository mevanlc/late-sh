use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols,
    text::{Line, Span},
    widgets::{
        Axis, Bar, BarChart, BarGroup, Block, Borders, Chart, Dataset, GraphType, Paragraph, Wrap,
    },
};

use crate::app::chat::state::{
    ControlCenterStatusSummary, DetailValue, RoomListRow, UserDetailRow, UserListRow,
};
use crate::app::common::theme;

use super::state::{BanPromptField, Tab};

const CC_STATUS_BANNED: Color = Color::Rgb(220, 68, 68);
const CC_STATUS_ADMIN: Color = Color::Rgb(245, 176, 65);
const CC_STATUS_MOD: Color = Color::Rgb(80, 190, 220);
const CC_STATUS_REGULAR: Color = Color::Rgb(176, 184, 192);

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
    pub music_vibe: &'a str,
    pub status_summary: &'a ControlCenterStatusSummary,
    pub user_list_lines: &'a [String],
    pub user_detail_lines: &'a [String],
    pub user_list_rows: &'a [UserListRow],
    pub user_detail_rows: &'a [UserDetailRow],
    pub selected_user_name: Option<&'a str>,
    pub user_filter: &'a str,
    pub user_filter_focused: bool,
    pub room_list_lines: &'a [String],
    pub room_list_rows: &'a [RoomListRow],
    pub room_detail_lines: &'a [String],
    pub room_filter: &'a str,
    pub room_filter_focused: bool,
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
        Tab::Users => "j/k or ↑/↓ move · ^F filter · h/l or ←/→ switch tabs",
        Tab::Rooms if view.room_filter_focused => {
            "Type to filter · ↓ enter list · Esc clear · ←/→ switch tabs"
        }
        Tab::Rooms => "j/k or ↑/↓ move · ^F filter · h/l or ←/→ switch tabs",
        Tab::Staff if view.is_admin => "j/k or ↑/↓ move · g grant admin · m give/remove mod",
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
        Tab::Rooms => draw_rooms_panel(frame, inner, view),
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
    let left_panels = Layout::vertical([
        Constraint::Length(16),
        Constraint::Length(6),
        Constraint::Fill(1),
    ])
    .split(columns[0]);

    draw_summary_card(frame, left_panels[0], "Pulse", &pulse_lines(view));
    draw_summary_card(
        frame,
        left_panels[1],
        "Moderation Radar",
        &moderation_radar_lines(view.status_summary),
    );
    draw_summary_card(
        frame,
        left_panels[2],
        "Staff Coverage",
        &staff_coverage_lines(view.status_summary),
    );
    draw_graphs_panel(frame, columns[1]);
}

fn pulse_lines(view: &ControlCenterView<'_>) -> Vec<Line<'static>> {
    let summary = view.status_summary;
    vec![
        metric_line("Users online", view.online_count.to_string()),
        metric_line("Live sessions", view.live_session_count.to_string()),
        metric_line("# Accounts", summary.account_count.to_string()),
        metric_line("Active topic rooms", recent_rooms_value(summary)),
        metric_line("New users today", summary.new_users_today.to_string()),
        metric_line(
            "New users this week",
            summary.new_users_this_week.to_string(),
        ),
        metric_line("Unique users today", summary.unique_users_today.to_string()),
        metric_line(
            "Unique users this week",
            summary.unique_users_this_week.to_string(),
        ),
        metric_line(
            "Users idle / active",
            format!("{} / {}", summary.idle_users, summary.active_users),
        ),
        metric_line(
            "Newest room",
            summary
                .newest_room
                .clone()
                .unwrap_or_else(|| "none".to_string()),
        ),
        metric_line("Music vibe", view.music_vibe.to_string()),
        metric_line("# Public Rooms", summary.public_rooms.to_string()),
        metric_line("# Private Rooms", summary.private_rooms.to_string()),
    ]
}

fn moderation_radar_lines(summary: &ControlCenterStatusSummary) -> Vec<Line<'static>> {
    vec![
        metric_line(
            "# Active server bans",
            summary.active_server_bans.to_string(),
        ),
        metric_line("# Active room bans", summary.active_room_bans.to_string()),
        metric_line(
            "Most recent ban",
            summary
                .most_recent_ban
                .clone()
                .unwrap_or_else(|| "none".to_string()),
        ),
        metric_line(
            "Most recent kick",
            summary
                .most_recent_kick
                .clone()
                .unwrap_or_else(|| "none".to_string()),
        ),
    ]
}

fn staff_coverage_lines(summary: &ControlCenterStatusSummary) -> Vec<Line<'static>> {
    vec![
        metric_line("Staff online now", summary.staff_online_now.to_string()),
        metric_line(
            "Staff recently active",
            summary.staff_recently_active.to_string(),
        ),
        metric_line(
            "Last staff actor / action",
            summary
                .last_staff_action
                .clone()
                .unwrap_or_else(|| "none".to_string()),
        ),
    ]
}

fn recent_rooms_value(summary: &ControlCenterStatusSummary) -> String {
    if summary.recent_room_labels.is_empty() {
        return format!("({}) none", summary.recent_room_count);
    }
    let mut value = format!(
        "({}) {}",
        summary.recent_room_count,
        summary.recent_room_labels.join(", ")
    );
    if summary.recent_room_count > summary.recent_room_labels.len() {
        value.push_str(", ...");
    }
    value
}

fn metric_line(label: &str, value: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{label:<23} "),
            Style::default().fg(theme::TEXT_DIM()),
        ),
        Span::styled(
            value,
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ),
    ])
}

fn draw_graphs_panel(frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .title(" Graphs ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Fill(1),
        Constraint::Fill(1),
        Constraint::Fill(1),
    ])
    .split(inner);
    let top_charts = Layout::horizontal([Constraint::Fill(1), Constraint::Fill(1)]).split(rows[0]);

    let weekday_bars: Vec<Bar<'_>> = [
        ("Mon", 18),
        ("Tue", 24),
        ("Wed", 21),
        ("Thu", 29),
        ("Fri", 33),
        ("Sat", 27),
        ("Sun", 20),
    ]
    .into_iter()
    .map(|(label, value)| {
        Bar::default()
            .value(value)
            .label(label)
            .style(Style::default().fg(theme::AMBER()))
            .value_style(Style::default().fg(theme::BG_CANVAS()).bg(theme::AMBER()))
    })
    .collect();
    frame.render_widget(
        bar_chart(
            &weekday_bars,
            " Average users by day of week (TODO) ",
            3,
            36,
        ),
        top_charts[0],
    );

    let time_of_day_bars: Vec<Bar<'_>> = [
        ("00", 7),
        ("04", 6),
        ("08", 18),
        ("12", 26),
        ("16", 33),
        ("20", 20),
    ]
    .into_iter()
    .map(|(label, value)| {
        Bar::default()
            .value(value)
            .label(label)
            .style(Style::default().fg(theme::AMBER()))
            .value_style(Style::default().fg(theme::BG_CANVAS()).bg(theme::AMBER()))
    })
    .collect();
    frame.render_widget(
        bar_chart(
            &time_of_day_bars,
            " Average users by time of day (TODO) ",
            3,
            36,
        ),
        top_charts[1],
    );

    let recent_chats = hourly_points(&[
        6.0, 3.0, 2.0, 4.0, 5.0, 8.0, 10.0, 13.0, 18.0, 21.0, 19.0, 24.0, 30.0, 26.0, 23.0, 27.0,
        35.0, 31.0, 28.0, 20.0, 17.0, 13.0, 9.0, 7.0,
    ]);
    frame.render_widget(
        line_chart(
            &recent_chats,
            " Recent chats, 1h buckets, 24h ago <-> now (TODO) ",
            [0.0, 23.0],
            [0.0, 40.0],
            &["-24h", "-12h", "now"],
        ),
        rows[1],
    );

    let recent_waterings = hourly_points(&[
        1.0, 0.0, 0.0, 1.0, 0.0, 2.0, 1.0, 3.0, 2.0, 4.0, 3.0, 5.0, 7.0, 5.0, 4.0, 6.0, 8.0, 7.0,
        5.0, 4.0, 4.0, 2.0, 2.0, 1.0,
    ]);
    frame.render_widget(
        line_chart(
            &recent_waterings,
            " Recent tree waterings, 1h buckets, 24h ago <-> now (TODO) ",
            [0.0, 23.0],
            [0.0, 10.0],
            &["-24h", "-12h", "now"],
        ),
        rows[2],
    );

    let recent_artboard = hourly_points(&[
        3.0, 2.0, 1.0, 1.0, 2.0, 4.0, 6.0, 5.0, 9.0, 12.0, 10.0, 14.0, 18.0, 15.0, 16.0, 19.0,
        23.0, 21.0, 20.0, 17.0, 12.0, 9.0, 7.0, 5.0,
    ]);
    frame.render_widget(
        line_chart(
            &recent_artboard,
            " Recent Artboard actions, 1h buckets, 24h ago <-> now (TODO) ",
            [0.0, 23.0],
            [0.0, 26.0],
            &["-24h", "-12h", "now"],
        ),
        rows[3],
    );
}

fn hourly_points(values: &[f64; 24]) -> Vec<(f64, f64)> {
    values
        .iter()
        .enumerate()
        .map(|(hour, value)| (hour as f64, *value))
        .collect()
}

fn line_chart<'a>(
    data: &'a [(f64, f64)],
    title: &'a str,
    x_bounds: [f64; 2],
    y_bounds: [f64; 2],
    x_labels: &[&'a str],
) -> Chart<'a> {
    let dataset = Dataset::default()
        .marker(symbols::Marker::Braille)
        .graph_type(GraphType::Line)
        .style(Style::default().fg(theme::AMBER()))
        .data(data);

    Chart::new(vec![dataset])
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme::BORDER_DIM())),
        )
        .x_axis(Axis::default().bounds(x_bounds).labels(x_labels.to_vec()))
        .y_axis(Axis::default().bounds(y_bounds).labels(["0", "mid", "max"]))
}

fn bar_chart<'a>(bars: &'a [Bar<'a>], title: &'a str, bar_width: u16, max: u64) -> BarChart<'a> {
    BarChart::default()
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme::BORDER_DIM())),
        )
        .bar_width(bar_width)
        .bar_gap(1)
        .max(max)
        .data(BarGroup::default().bars(bars))
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
            Constraint::Fill(1),
            Constraint::Fill(1),
            Constraint::Fill(1),
        ])
        .split(area)
    };

    draw_user_directory_card(
        frame,
        columns[0],
        view.user_list_lines,
        view.user_list_rows,
        view.user_filter,
        view.user_filter_focused,
    );

    let detail_title = view
        .selected_user_name
        .map(|name| format!("User: @{name}"))
        .unwrap_or_else(|| "User Detail".to_string());
    if view.user_detail_rows.is_empty() {
        draw_panel_card(
            frame,
            columns[1],
            &detail_title,
            view.user_detail_lines,
            false,
        );
    } else {
        draw_user_detail_card(frame, columns[1], &detail_title, view.user_detail_rows);
    }

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

const USER_COL_WIDTH: usize = 22;

fn draw_user_directory_card(
    frame: &mut Frame,
    area: Rect,
    user_list_lines: &[String],
    user_list_rows: &[UserListRow],
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
        Constraint::Length(1), // filter (top)
        Constraint::Length(1), // divider
        Constraint::Length(1), // header: "banned" flag line
        Constraint::Length(1), // header: column labels
        Constraint::Length(1), // divider
        Constraint::Fill(1),   // list body
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
    frame.render_widget(Paragraph::new(user_list_header_line_2()), layout[2]);
    frame.render_widget(Paragraph::new(user_list_header_line_1()), layout[3]);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "─".repeat(layout[4].width as usize),
            Style::default().fg(theme::BORDER()),
        ))),
        layout[4],
    );

    let body_lines: Vec<Line<'_>> = if user_list_rows.is_empty() {
        user_list_lines
            .iter()
            .map(|line| {
                Line::from(Span::styled(
                    line.as_str(),
                    Style::default().fg(theme::TEXT_FAINT()),
                ))
            })
            .collect()
    } else {
        user_list_rows.iter().map(user_list_row_line).collect()
    };
    // No wrap: preserve leading whitespace (reserved arrow column) and clip long names.
    frame.render_widget(Paragraph::new(body_lines), layout[5]);
}

fn user_list_header_line_1() -> Line<'static> {
    // Layout (no · separator): prefix(2) + user(USER_COL_WIDTH) + count(1) + role(2) + ban(1)
    // 'a' in admin/mod lands at position 2+USER_COL_WIDTH+1+1 = role indicator position.
    // Fill gap so "#sessions " ends at the count column and 'a' lines up with the role letter.
    // fill = role_indicator_pos - prefix(2) - "user"(4) - "#sessions "(10)
    //      = (2 + USER_COL_WIDTH + 2) - 16 = USER_COL_WIDTH - 12
    // Spaces must be styled so ratatui emits them (default-style spaces are no-ops in diff rendering).
    let fill = " ".repeat(USER_COL_WIDTH - 12);
    Line::from(vec![
        Span::styled("  ".to_string(), Style::default().fg(theme::TEXT_FAINT())),
        Span::styled("user".to_string(), Style::default().fg(theme::TEXT_FAINT())),
        Span::styled(fill, Style::default().fg(theme::TEXT_FAINT())),
        Span::styled(
            "#sessions ".to_string(),
            Style::default().fg(theme::TEXT_FAINT()),
        ),
        Span::styled("a".to_string(), cc_admin_style()),
        Span::styled(
            "dmin/".to_string(),
            Style::default().fg(theme::TEXT_FAINT()),
        ),
        Span::styled("m".to_string(), cc_mod_style()),
        Span::styled("od".to_string(), Style::default().fg(theme::TEXT_FAINT())),
    ])
}

fn user_list_header_line_2() -> Line<'static> {
    // Rendered above the column-labels line. 'b' aligns with ban column:
    // prefix(2) + USER_COL_WIDTH + count(1) + role(2) = ban position.
    // Styled so ratatui emits the spaces (default-style spaces are no-ops).
    let pad = " ".repeat(2 + USER_COL_WIDTH + 1 + 2);
    Line::from(vec![
        Span::styled(pad, Style::default().fg(theme::TEXT_FAINT())),
        Span::styled("b".to_string(), cc_banned_style()),
        Span::styled(
            "anned".to_string(),
            Style::default().fg(theme::TEXT_FAINT()),
        ),
    ])
}

fn user_list_row_line(row: &UserListRow) -> Line<'static> {
    // Columns 0-1: fixed 2-char prefix — arrow occupies col 0, space occupies col 1.
    // Paragraph renders without trimming so the "  " for non-selected is preserved,
    // keeping @username pinned at col 2 regardless of selection state.
    let prefix_span = if row.selected {
        Span::styled("> ".to_string(), Style::default().fg(theme::AMBER_GLOW()))
    } else {
        Span::styled("  ".to_string(), Style::default().fg(theme::TEXT_FAINT()))
    };

    let at_name = format!("@{}", row.username);
    let padded = format!("{:<width$}", at_name, width = USER_COL_WIDTH);
    let username_style = cc_user_status_style(row);
    let username_span = Span::styled(padded, username_style);

    let count_style = if row.session_count > 0 {
        Style::default().fg(theme::AMBER())
    } else {
        Style::default().fg(theme::TEXT_FAINT())
    };
    let count_span = Span::styled(row.session_count.to_string(), count_style);

    // Role span is 2 chars (space + indicator) so the ban column stays fixed regardless of
    // staff status. Spaces must be styled — default-style spaces are no-ops in differential rendering.
    let role_span = if row.is_admin {
        Span::styled(" a".to_string(), cc_admin_style())
    } else if row.is_moderator {
        Span::styled(" m".to_string(), cc_mod_style())
    } else {
        Span::styled("  ".to_string(), cc_regular_style())
    };

    let ban_span = if row.banned {
        Span::styled("b".to_string(), cc_banned_style())
    } else {
        Span::raw("")
    };

    Line::from(vec![
        prefix_span,
        username_span,
        count_span,
        role_span,
        ban_span,
    ])
}

fn cc_user_status_style(row: &UserListRow) -> Style {
    let style = if row.banned {
        cc_banned_style()
    } else if row.is_admin {
        cc_admin_style()
    } else if row.is_moderator {
        cc_mod_style()
    } else {
        cc_regular_style()
    };
    if row.selected {
        style.bg(theme::BG_HIGHLIGHT()).add_modifier(Modifier::BOLD)
    } else {
        style
    }
}

fn cc_banned_style() -> Style {
    Style::default().fg(CC_STATUS_BANNED)
}

fn cc_admin_style() -> Style {
    Style::default().fg(CC_STATUS_ADMIN)
}

fn cc_mod_style() -> Style {
    Style::default().fg(CC_STATUS_MOD)
}

fn cc_regular_style() -> Style {
    Style::default().fg(CC_STATUS_REGULAR)
}

fn draw_user_detail_card(frame: &mut Frame, area: Rect, title: &str, rows: &[UserDetailRow]) {
    let block = Block::default()
        .title(format!(" {title} "))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines: Vec<Line<'_>> = rows.iter().map(user_detail_row_line).collect();
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
}

fn user_detail_row_line(row: &UserDetailRow) -> Line<'static> {
    // Pad to 19 so labels up to 18 chars always have at least one space before value.
    let label_span = Span::styled(
        format!("{:<19}", row.label),
        Style::default().fg(theme::TEXT_DIM()),
    );
    let value_span = match &row.value {
        DetailValue::Placeholder => Span::styled(
            "\u{2014}".to_string(),
            Style::default().fg(theme::TEXT_FAINT()),
        ),
        DetailValue::Text(value) => {
            Span::styled(value.clone(), Style::default().fg(theme::TEXT_BRIGHT()))
        }
        DetailValue::Count(0) => {
            Span::styled("0".to_string(), Style::default().fg(theme::TEXT_FAINT()))
        }
        DetailValue::Count(n) => Span::styled(n.to_string(), Style::default().fg(theme::AMBER())),
        DetailValue::BanActive(note) => Span::styled(format!("Yes  ({note})"), cc_banned_style()),
        DetailValue::BanNone => Span::styled("No".to_string(), cc_regular_style()),
    };
    Line::from(vec![label_span, value_span])
}

fn user_filter_line(value: &str, focused: bool) -> Line<'static> {
    filter_line("filter ^F", "@username", value, focused)
}

fn room_filter_line(value: &str, focused: bool) -> Line<'static> {
    filter_line("filter ^F", "#room / kind", value, focused)
}

fn filter_line(label: &str, placeholder: &str, value: &str, focused: bool) -> Line<'static> {
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
        Span::styled(label.to_string(), label_style),
        Span::raw(" "),
        Span::styled("> ".to_string(), caret_style),
    ];
    if value.is_empty() {
        spans.push(Span::styled(
            placeholder.to_string(),
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
        "s  Sanction history (TODO)".to_string(),
        "c  Clear profile (TODO)".to_string(),
        "a  View audit trail".to_string(),
        "!  Warn user (TODO)".to_string(),
        "k  Kick (server) (TODO)".to_string(),
        "r  Recent chats (TODO)".to_string(),
        "b  Ban\u{2026}".to_string(),
        "u  Unban".to_string(),
        ">  Open DM (TODO)".to_string(),
        "p  View profile (TODO)".to_string(),
    ];
    if is_admin {
        lines.push("m  Give/remove mod".to_string());
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
            let active = implemented.contains(&key);
            let key_style = if active {
                Style::default().fg(theme::AMBER())
            } else {
                Style::default().fg(theme::TEXT_FAINT())
            };
            let label_style = if active {
                Style::default().fg(theme::TEXT())
            } else {
                Style::default().fg(theme::TEXT_FAINT())
            };
            let (key_part, label_part) = line.split_at(line.find("  ").unwrap_or(line.len()));
            Line::from(vec![
                Span::styled(key_part.to_string(), key_style),
                Span::styled(label_part.to_string(), label_style),
            ])
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

fn draw_rooms_panel(frame: &mut Frame, area: Rect, view: &ControlCenterView<'_>) {
    let columns = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Fill(1),
        Constraint::Fill(1),
    ])
    .split(area);

    draw_room_directory_card(
        frame,
        columns[0],
        view.room_list_lines,
        view.room_list_rows,
        view.room_filter,
        view.room_filter_focused,
    );
    draw_room_detail_card(frame, columns[1], view.room_detail_lines);
    if let Some(prompt) = view.room_prompt.as_ref() {
        draw_room_prompt_card(frame, columns[2], prompt);
    } else {
        draw_room_actions_panel(frame, columns[2]);
    }
}

const ROOM_COL_WIDTH: usize = 20;

fn draw_room_directory_card(
    frame: &mut Frame,
    area: Rect,
    room_list_lines: &[String],
    room_list_rows: &[RoomListRow],
    room_filter: &str,
    room_filter_focused: bool,
) {
    let border_style = if room_filter_focused {
        Style::default().fg(theme::BORDER_ACTIVE())
    } else {
        Style::default().fg(theme::BORDER())
    };
    let block = Block::default()
        .title(" Room Directory ")
        .borders(Borders::ALL)
        .border_style(border_style);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let layout = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .split(inner);

    frame.render_widget(
        Paragraph::new(room_filter_line(room_filter, room_filter_focused)),
        layout[0],
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "─".repeat(layout[1].width as usize),
            Style::default().fg(theme::BORDER()),
        ))),
        layout[1],
    );
    frame.render_widget(Paragraph::new(room_list_header_line()), layout[2]);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "─".repeat(layout[3].width as usize),
            Style::default().fg(theme::BORDER()),
        ))),
        layout[3],
    );

    let body_lines: Vec<Line<'_>> = if room_list_rows.is_empty() {
        room_list_lines
            .iter()
            .map(|line| {
                Line::from(Span::styled(
                    line.as_str(),
                    Style::default().fg(theme::TEXT_FAINT()),
                ))
            })
            .collect()
    } else {
        room_list_rows.iter().map(room_list_row_line).collect()
    };
    frame.render_widget(Paragraph::new(body_lines), layout[4]);
}

fn room_list_header_line() -> Line<'static> {
    Line::from(vec![
        Span::styled("  ".to_string(), Style::default().fg(theme::TEXT_FAINT())),
        Span::styled(
            format!("{:<width$}", "room", width = ROOM_COL_WIDTH),
            Style::default().fg(theme::TEXT_FAINT()),
        ),
        Span::styled("mem ".to_string(), Style::default().fg(theme::TEXT_FAINT())),
        Span::styled("vis ".to_string(), Style::default().fg(theme::TEXT_FAINT())),
        Span::styled(
            "kind ".to_string(),
            Style::default().fg(theme::TEXT_FAINT()),
        ),
        Span::styled(
            "flags".to_string(),
            Style::default().fg(theme::TEXT_FAINT()),
        ),
    ])
}

fn room_list_row_line(row: &RoomListRow) -> Line<'static> {
    let prefix_span = if row.selected {
        Span::styled("> ".to_string(), Style::default().fg(theme::AMBER_GLOW()))
    } else {
        Span::styled("  ".to_string(), Style::default().fg(theme::TEXT_FAINT()))
    };
    let room_style = if row.selected {
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .bg(theme::BG_HIGHLIGHT())
            .add_modifier(Modifier::BOLD)
    } else if row.active_ban_count > 0 {
        cc_banned_style()
    } else {
        Style::default().fg(theme::TEXT())
    };
    let label = truncate_to_width(&row.label, ROOM_COL_WIDTH);
    let label_span = Span::styled(
        format!("{label:<width$}", width = ROOM_COL_WIDTH),
        room_style,
    );
    let member_style = if row.member_count > 0 {
        Style::default().fg(theme::AMBER())
    } else {
        Style::default().fg(theme::TEXT_FAINT())
    };
    let visibility_style = match row.visibility.as_str() {
        "public" => Style::default().fg(theme::SUCCESS()),
        "private" => Style::default().fg(theme::AMBER()),
        "dm" => Style::default().fg(theme::TEXT_DIM()),
        _ => Style::default().fg(theme::TEXT()),
    };
    let flags = room_flags(row);
    Line::from(vec![
        prefix_span,
        label_span,
        Span::styled(format!("{:<4}", row.member_count), member_style),
        Span::styled(
            short_visibility(&row.visibility).to_string(),
            visibility_style,
        ),
        Span::styled(" ".to_string(), Style::default().fg(theme::TEXT_FAINT())),
        Span::styled(
            format!("{:<5}", short_kind(&row.kind)),
            Style::default().fg(theme::TEXT_DIM()),
        ),
        Span::styled(flags, Style::default().fg(theme::TEXT_FAINT())),
    ])
}

fn truncate_to_width(value: &str, width: usize) -> String {
    let mut out: String = value.chars().take(width).collect();
    if value.chars().count() > width && width > 0 {
        out.pop();
        out.push('…');
    }
    out
}

fn short_visibility(visibility: &str) -> &str {
    match visibility {
        "public" => "pub ",
        "private" => "priv",
        "dm" => "dm  ",
        _ => "oth ",
    }
}

fn short_kind(kind: &str) -> &str {
    match kind {
        "general" => "gen",
        "language" => "lang",
        "private" => "priv",
        "game" => "game",
        "dm" => "dm",
        _ => "room",
    }
}

fn room_flags(row: &RoomListRow) -> String {
    let mut flags = String::new();
    if row.permanent {
        flags.push('p');
    }
    if row.auto_join {
        flags.push('a');
    }
    if row.active_ban_count > 0 {
        flags.push('b');
    }
    if flags.is_empty() {
        "-".to_string()
    } else {
        flags
    }
}

fn draw_room_detail_card(frame: &mut Frame, area: Rect, room_detail_lines: &[String]) {
    let block = Block::default()
        .title(" Selected Room ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines: Vec<Line<'_>> = room_detail_lines
        .iter()
        .map(|line| room_detail_line(line))
        .collect();
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
}

fn room_detail_line(line: &str) -> Line<'static> {
    if line.is_empty() {
        return Line::from(Span::raw(""));
    }
    if line.starts_with('#') {
        return Line::from(Span::styled(
            line.to_string(),
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ));
    }
    if let Some((label, value)) = line.split_once(':') {
        let value = value.trim();
        let value_style = match label {
            "visibility" if value == "public" => Style::default().fg(theme::SUCCESS()),
            "visibility" if value == "private" => Style::default().fg(theme::AMBER()),
            "active room bans" if value != "none" => cc_banned_style(),
            "permanent" | "auto-join" if value == "yes" => Style::default().fg(theme::AMBER()),
            _ => Style::default().fg(theme::TEXT_BRIGHT()),
        };
        return Line::from(vec![
            Span::styled(
                format!("{:<18} ", label),
                Style::default().fg(theme::TEXT_DIM()),
            ),
            Span::styled(value.to_string(), value_style),
        ]);
    }
    Line::from(Span::styled(
        line.to_string(),
        Style::default().fg(theme::TEXT()),
    ))
}

fn draw_room_actions_panel(frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .title(" Room Actions ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines: Vec<Line<'_>> = room_actions_lines()
        .into_iter()
        .map(|line| {
            let (key_part, label_part) = line.split_at(line.find("  ").unwrap_or(line.len()));
            Line::from(vec![
                Span::styled(key_part.to_string(), Style::default().fg(theme::AMBER())),
                Span::styled(label_part.to_string(), Style::default().fg(theme::TEXT())),
            ])
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), inner);
}

fn room_actions_lines() -> Vec<String> {
    vec![
        "x  Kick member…".to_string(),
        "b  Ban member…".to_string(),
        "u  Unban member…".to_string(),
        "r  Rename room…".to_string(),
        "p  Make public".to_string(),
        "v  Make private".to_string(),
        "d  Delete room".to_string(),
    ]
}

fn draw_room_prompt_card(frame: &mut Frame, area: Rect, prompt: &RoomPromptView<'_>) {
    let block = Block::default()
        .title(format!(" {} ", prompt.panel_title))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

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
    let lines = vec![
        Line::from(Span::styled(
            value_label,
            Style::default().fg(theme::TEXT_DIM()),
        )),
        Line::from(Span::raw("")),
        Line::from(vec![
            Span::styled("> ".to_string(), Style::default().fg(theme::AMBER_GLOW())),
            Span::styled(
                format!("{}{}", value_prefix, prompt.value),
                Style::default().fg(theme::TEXT_BRIGHT()),
            ),
        ]),
        Line::from(Span::raw("")),
        Line::from(Span::styled(
            "Enter confirms",
            Style::default().fg(theme::TEXT_FAINT()),
        )),
        Line::from(Span::styled(
            "Esc cancels",
            Style::default().fg(theme::TEXT_FAINT()),
        )),
    ];
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
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
