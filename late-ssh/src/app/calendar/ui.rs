//! Theme resolution and hit geometry happen during this frame, never at load.
use super::{
    date_entry,
    state::{Action, CalendarState, Hit, Modal, Pane, ScrollPane, month_start},
};
use crate::app::common::{primitives::hint_line, theme};
use chrono::{DateTime, Datelike, Duration, LocalResult, NaiveDate, TimeZone, Timelike, Utc};
use late_core::models::calendar::{
    CalendarEvent, CalendarSource, CalendarView, EventTiming, Occurrence, event_access,
};
use ratatui::{
    Frame,
    layout::{Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
};
fn base() -> Style {
    Style::default().fg(theme::TEXT()).bg(theme::BG_CANVAS())
}
fn dim() -> Style {
    base().fg(theme::TEXT_DIM())
}
fn muted() -> Style {
    base().fg(theme::TEXT_MUTED())
}
fn bright() -> Style {
    base().fg(theme::TEXT_BRIGHT())
}
fn rule() -> Style {
    base().fg(theme::BORDER_DIM())
}
fn key_style() -> Style {
    base().fg(theme::AMBER_DIM()).add_modifier(Modifier::BOLD)
}
fn separator() -> Span<'static> {
    Span::styled(" · ", base().fg(theme::TEXT_FAINT()))
}
fn accent() -> Style {
    base().fg(theme::AMBER()).add_modifier(Modifier::BOLD)
}
fn border(title: impl Into<Line<'static>>) -> Block<'static> {
    Block::default()
        .title(title)
        .title_style(bright().add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(rule())
        .style(base())
}
fn hit(s: &CalendarState, area: Rect, action: Action) {
    if area.width > 0 && area.height > 0 {
        s.hits.borrow_mut().push(Hit { area, action });
    }
}
fn pane(s: &CalendarState, area: Rect, pane: Pane) {
    s.panes.borrow_mut().push(ScrollPane { area, pane });
}
fn row(frame: &mut Frame, area: Rect, text: impl Into<String>, style: Style) {
    frame.render_widget(Paragraph::new(text.into()).style(style), area);
}
fn styled_row(frame: &mut Frame, area: Rect, line: Line<'_>) {
    frame.render_widget(Paragraph::new(line).style(base()), area);
}
fn patch_line(mut line: Line<'static>, style: Style) -> Line<'static> {
    line.style = line.style.patch(style);
    // Explicit span colors must also yield to terminal-owned selection colors.
    for span in &mut line.spans {
        span.style = span.style.patch(style);
    }
    line
}
fn selected_line(line: Line<'static>, selected: bool) -> Line<'static> {
    if selected {
        patch_line(line, theme::selection_style())
    } else {
        line
    }
}
fn button_line(label: &str) -> Line<'static> {
    let mut line = if let Some((key, description)) = label.split_once(' ')
        && key.chars().count() == 1
    {
        hint_line(&[(key, description)])
    } else if let Some((description, key)) =
        label.strip_suffix(')').and_then(|s| s.rsplit_once(" ("))
    {
        Line::from(vec![
            Span::styled(format!(" {description}"), base()),
            Span::styled(" (", base().fg(theme::TEXT_FAINT())),
            Span::styled(key.to_owned(), key_style()),
            Span::styled(")", base().fg(theme::TEXT_FAINT())),
        ])
    } else {
        Line::from(vec![Span::styled(
            format!(" {label}"),
            if label == "Delete" {
                key_style()
            } else {
                bright().add_modifier(Modifier::BOLD)
            },
        )])
    };
    line.spans.push(Span::raw(" "));
    line
}
#[allow(clippy::too_many_arguments)] // Rendered geometry and action belong together.
fn button(
    frame: &mut Frame,
    s: &CalendarState,
    x: &mut u16,
    y: u16,
    right: u16,
    label: &str,
    action: Action,
    selected: bool,
) {
    let mut line = button_line(label);
    if selected {
        for span in &mut line.spans {
            if span.style.fg == Some(theme::TEXT_DIM()) {
                span.style = bright().add_modifier(Modifier::BOLD);
            }
        }
    }
    let w = line.width() as u16;
    if *x + w > right {
        return;
    }
    let rect = Rect::new(*x, y, w, 1);
    styled_row(frame, rect, selected_line(line, selected));
    hit(s, rect, action);
    *x += w;
}
pub fn source_label(s: &CalendarState) -> String {
    match s.source {
        CalendarSource::Server => "Server".into(),
        CalendarSource::Personal(id) if id == s.viewer => "Personal".into(),
        CalendarSource::Personal(id) => s
            .public
            .iter()
            .find(|p| p.owner_id == id)
            .map(|p| format!("@{}", p.username))
            .unwrap_or_else(|| "Shared calendar".into()),
    }
}
pub fn timing_label(e: &CalendarEvent, tz: chrono_tz::Tz) -> String {
    match e.timing {
        EventTiming::AllDay {
            start,
            end_exclusive,
        } => {
            let last = end_exclusive - Duration::days(1);
            if start == last {
                format!("{start} · all day")
            } else {
                format!("{start} – {last} · all day")
            }
        }
        EventTiming::Timed { start, end } => format!(
            "{}{}",
            local_time_label(start, tz, true),
            end.map(|e| format!(" – {}", local_time_label(e, tz, true)))
                .unwrap_or_default()
        ),
    }
}
fn local_time_label(instant: DateTime<Utc>, tz: chrono_tz::Tz, date: bool) -> String {
    let local = instant.with_timezone(&tz);
    let repeated = matches!(
        tz.from_local_datetime(&local.naive_local()),
        LocalResult::Ambiguous(..)
    );
    let format = match (date, repeated) {
        (true, true) => "%Y-%m-%d %H:%M %Z",
        (true, false) => "%Y-%m-%d %H:%M",
        (false, true) => "%H:%M %Z",
        (false, false) => "%H:%M",
    };
    local.format(format).to_string()
}
fn source_span(e: &CalendarEvent) -> Span<'static> {
    Span::styled(
        if e.owner_id.is_none() {
            "[Server]"
        } else {
            "[Personal]"
        },
        if e.owner_id.is_none() {
            base().fg(theme::AMBER_DIM())
        } else {
            dim()
        },
    )
}
fn timing_line(e: &CalendarEvent, tz: chrono_tz::Tz) -> Line<'static> {
    let mut spans = Vec::new();
    match e.timing {
        EventTiming::AllDay {
            start,
            end_exclusive,
        } => {
            spans.push(Span::styled(start.to_string(), muted()));
            let last = end_exclusive - Duration::days(1);
            if start != last {
                spans.push(Span::styled(" – ", base().fg(theme::TEXT_FAINT())));
                spans.push(Span::styled(last.to_string(), muted()));
            }
            spans.push(separator());
            spans.push(Span::styled("all day", dim()));
        }
        EventTiming::Timed { start, end } => {
            spans.push(Span::styled(local_time_label(start, tz, true), muted()));
            if let Some(end) = end {
                spans.push(Span::styled(" – ", base().fg(theme::TEXT_FAINT())));
                spans.push(Span::styled(local_time_label(end, tz, true), muted()));
            }
        }
    }
    Line::from(spans)
}
fn event_line(e: &CalendarEvent, tz: chrono_tz::Tz) -> Line<'static> {
    let time = match e.timing {
        EventTiming::AllDay { .. } => "all day".into(),
        EventTiming::Timed { start, .. } => local_time_label(start, tz, false),
    };
    let mut spans = Vec::new();
    if e.owner_id.is_none() {
        spans.push(source_span(e));
        spans.push(Span::raw(" "));
    }
    spans.push(Span::styled(time, muted()));
    spans.push(Span::raw(" "));
    spans.push(Span::styled(e.title.clone(), bright()));
    Line::from(spans)
}
fn labeled(label: &str, value: impl Into<String>, style: Style) -> Line<'static> {
    Line::from(vec![
        Span::styled(label.to_owned(), dim()),
        Span::styled(": ", base().fg(theme::TEXT_FAINT())),
        Span::styled(value.into(), style),
    ])
}
pub fn draw(frame: &mut Frame, area: Rect, s: &CalendarState) {
    s.hits.borrow_mut().clear();
    s.panes.borrow_mut().clear();
    s.geometry.set(area);
    if area.width < 12 || area.height < 5 {
        row(frame, area, "Calendars · enlarge terminal", dim());
        return;
    }
    frame.render_widget(Paragraph::new("").style(base()), area);
    let mut x = area.x;
    button(
        frame,
        s,
        &mut x,
        area.y,
        area.right(),
        &format!("s {}", source_label(s)),
        Action::Source,
        true,
    );
    button(
        frame,
        s,
        &mut x,
        area.y,
        area.right(),
        &format!("v {}", s.view.label()),
        Action::View,
        false,
    );
    let timezone_on_first_row = x + Line::from(s.tz.to_string()).width() as u16 + 2 <= area.right();
    if timezone_on_first_row {
        row(
            frame,
            Rect::new(x + 1, area.y, area.right() - x - 1, 1),
            s.tz.to_string(),
            dim(),
        );
    }
    let mut x = area.x;
    for (label, action) in [
        ("[ ‹", Action::Previous),
        ("] ›", Action::Next),
        ("t Today", Action::Today),
        ("g Date", Action::Go),
        ("n New", Action::New),
        ("c Settings", Action::Settings),
    ] {
        button(
            frame,
            s,
            &mut x,
            area.y + 1,
            area.right(),
            label,
            action,
            false,
        );
    }
    let mut period = Vec::new();
    if !timezone_on_first_row {
        period.push(Span::styled(s.tz.to_string(), dim()));
        period.push(separator());
    }
    period.push(Span::styled(
        s.selected.format("%B %Y").to_string(),
        accent(),
    ));
    if s.loading {
        period.push(Span::styled("  Loading…", muted()));
    }
    styled_row(
        frame,
        Rect::new(area.x, area.y + 2, area.width, 1),
        Line::from(period),
    );
    let panel_height = if area.height < 20 || area.width < 50 {
        1
    } else {
        5
    };
    let panel = Rect::new(
        area.x,
        area.bottom() - panel_height,
        area.width,
        panel_height,
    );
    draw_upcoming_panel(frame, panel, s);
    let body = Rect::new(
        area.x,
        area.y + 3,
        area.width,
        area.height.saturating_sub(3 + panel_height),
    );
    match s.view {
        CalendarView::Month => {
            if area.width >= 95 {
                let width = (body.width * 2 / 3).max(56);
                let grid = Rect::new(body.x, body.y, width, body.height);
                let agenda = Rect::new(
                    grid.right() + 1,
                    body.y,
                    body.width.saturating_sub(width + 1),
                    body.height,
                );
                draw_month(frame, grid, s);
                draw_agenda(frame, agenda, s);
            } else {
                draw_month(frame, body, s);
            }
        }
        CalendarView::List => draw_list(frame, body, s, false),
        _ => draw_hours(frame, body, s),
    }
    if let Some(error) = &s.error {
        row(
            frame,
            Rect::new(area.x, area.y + 2, area.width, 1),
            error,
            base().fg(theme::ERROR()),
        );
    }
}
#[allow(clippy::too_many_arguments)]
fn grid_rule(
    frame: &mut Frame,
    s: &CalendarState,
    area: Rect,
    widths: &[u16],
    y: u16,
    left: &str,
    middle: &str,
    right: &str,
) {
    let mut text = left.to_string();
    for (i, w) in widths.iter().enumerate() {
        text.push_str(&"─".repeat(w.saturating_sub(1) as usize));
        text.push_str(if i == 6 { right } else { middle });
    }
    row(frame, Rect::new(area.x, y, area.width, 1), text, rule());
    let _ = s;
}
fn draw_month(frame: &mut Frame, area: Rect, s: &CalendarState) {
    if area.width < 15 || area.height < 9 {
        styled_row(frame, area, hint_line(&[("Enter:", "selected-day agenda")]));
        hit(s, area, Action::Date(s.selected));
        return;
    }
    let available = area.width - 1;
    let mut widths = vec![available / 7; 7];
    for w in widths.iter_mut().take((available % 7) as usize) {
        *w += 1;
    }
    if area.height < 14 {
        let first = s.range().0;
        let mut x = area.x + 1;
        for (col, w) in widths.iter().enumerate() {
            row(
                frame,
                Rect::new(x, area.y, w - 1, 1),
                (first + Duration::days(col as i64))
                    .format("%a")
                    .to_string(),
                dim(),
            );
            x += w;
        }
        grid_rule(frame, s, area, &widths, area.y + 1, "╭", "┬", "╮");
        for week in 0..6 {
            let mut x = area.x;
            for (col, w) in widths.iter().enumerate() {
                let date = first + Duration::days((week * 7 + col) as i64);
                let count = s.day_events(date).len();
                let rect = Rect::new(x + 1, area.y + 2 + week as u16, w - 1, 1);
                let style = if date == s.selected {
                    accent().patch(theme::selection_style())
                } else if date == s.today() {
                    accent().add_modifier(Modifier::UNDERLINED)
                } else if date.month() != s.selected.month() {
                    dim()
                } else {
                    base()
                };
                let label = if count > 0 {
                    format!("{}+{count}", date.day())
                } else {
                    date.day().to_string()
                };
                // Keep counts intact before spending the remaining cells on markers.
                let spare =
                    usize::from(rect.width).saturating_sub(Line::from(label.as_str()).width());
                let marker = match (date == s.today(), date == s.selected, spare) {
                    (true, true, 2..) => "•▸",
                    (true, true, 1) => "◆",
                    (true, false, 1..) => "•",
                    (false, true, 1..) => "▸",
                    _ => "",
                };
                row(frame, Rect::new(x, rect.y, 1, 1), "│", rule());
                let mut line =
                    Line::from(vec![Span::styled(format!("{marker}{}", date.day()), style)]);
                if count > 0 {
                    line.spans.push(Span::styled(
                        format!("+{count}"),
                        muted().add_modifier(Modifier::BOLD),
                    ));
                }
                styled_row(frame, rect, selected_line(line, date == s.selected));
                hit(s, rect, Action::Date(date));
                x += w;
            }
            row(
                frame,
                Rect::new(area.right() - 1, area.y + 2 + week as u16, 1, 1),
                "│",
                rule(),
            );
        }
        grid_rule(frame, s, area, &widths, area.y + 8, "╰", "┴", "╯");
        return;
    }
    let first = s.range().0;
    let today = s.today();
    let mut x = area.x + 1;
    for (col, width) in widths.iter().enumerate() {
        row(
            frame,
            Rect::new(x, area.y, width.saturating_sub(1), 1),
            (first + Duration::days(col as i64))
                .format("%a")
                .to_string(),
            dim(),
        );
        x += width;
    }
    // A short month is still six complete weeks. Cells have at least a date
    // line; crowded short layouts place explicit counts on that line.
    let total = area.height - 2;
    let mut heights = [total / 6; 6];
    for h in heights.iter_mut().take((total % 6) as usize) {
        *h += 1;
    }
    let mut y = area.y + 1;
    grid_rule(frame, s, area, &widths, y, "╭", "┬", "╮");
    for (week, height) in heights.iter().enumerate() {
        let mut x = area.x;
        for (col, width) in widths.iter().enumerate() {
            let date = first + Duration::days((week * 7 + col) as i64);
            let events = s.day_events(date);
            let cell = Rect::new(
                x + 1,
                y + 1,
                width.saturating_sub(1),
                height.saturating_sub(1),
            );
            if cell.height > 0 {
                let style = if date == s.selected {
                    accent().patch(theme::selection_style())
                } else if date == today {
                    accent().add_modifier(Modifier::UNDERLINED)
                } else if date.month() != s.selected.month() {
                    dim()
                } else {
                    base()
                };
                let capacity = cell.height.saturating_sub(1) as usize;
                let mut date_label = Line::styled(
                    format!(
                        "{}{}{}",
                        if date == today { "•" } else { "" },
                        if date == s.selected { "▸" } else { "" },
                        date.day()
                    ),
                    style,
                );
                if capacity == 0 && !events.is_empty() {
                    date_label.spans.push(Span::styled(
                        format!(" +{}", events.len()),
                        muted().add_modifier(Modifier::BOLD),
                    ));
                }
                styled_row(
                    frame,
                    Rect::new(cell.x, cell.y, cell.width, 1),
                    selected_line(date_label, date == s.selected),
                );
                hit(s, cell, Action::Date(date));
                let previews = if events.len() > capacity {
                    capacity.saturating_sub(1)
                } else {
                    capacity
                };
                for (n, e) in events.iter().take(previews).enumerate() {
                    let rect = Rect::new(cell.x, cell.y + 1 + n as u16, cell.width, 1);
                    let mut spans = Vec::new();
                    if s.source.owner().is_some() && e.owner_id.is_none() {
                        spans.push(source_span(e));
                        spans.push(Span::raw(" "));
                    }
                    spans.push(Span::styled(e.title.clone(), bright()));
                    styled_row(frame, rect, Line::from(spans));
                    hit(s, rect, Action::Event(e.id));
                }
                if events.len() > previews && capacity > 0 {
                    let rect = Rect::new(cell.x, cell.y + cell.height - 1, cell.width, 1);
                    row(
                        frame,
                        rect,
                        format!("+{} more", events.len() - previews),
                        dim(),
                    );
                    hit(s, rect, Action::Date(date));
                }
            }
            for line in 1..*height {
                row(frame, Rect::new(x, y + line, 1, 1), "│", rule());
            }
            x += width;
        }
        for line in 1..*height {
            row(
                frame,
                Rect::new(area.right() - 1, y + line, 1, 1),
                "│",
                rule(),
            );
        }
        y += height;
        grid_rule(
            frame,
            s,
            area,
            &widths,
            y,
            if week == 5 { "╰" } else { "├" },
            if week == 5 { "┴" } else { "┼" },
            if week == 5 { "╯" } else { "┤" },
        );
    }
}
fn draw_agenda(frame: &mut Frame, area: Rect, s: &CalendarState) {
    let mut title = vec![
        Span::styled(
            format!(" {}", s.selected),
            bright().add_modifier(Modifier::BOLD),
        ),
        separator(),
    ];
    title.extend(hint_line(&[("Enter", "details")]).spans.into_iter().skip(1));
    title.push(Span::raw(" "));
    let b = border(Line::from(title));
    let inner = b.inner(area);
    frame.render_widget(b, area);
    pane(s, inner, Pane::Agenda);
    let events = s.day_events(s.selected);
    s.max_agenda.set(
        events
            .len()
            .saturating_mul(2)
            .saturating_sub(inner.height as usize),
    );
    if events.is_empty() {
        let mut line = Line::from(vec![Span::styled("No events", dim()), separator()]);
        line.spans
            .extend(hint_line(&[("n", "to add")]).spans.into_iter().skip(1));
        styled_row(frame, inner, line);
        return;
    }
    for (n, e) in events.iter().enumerate() {
        let y = (n * 2) as isize - s.agenda_scroll as isize;
        if y < 0 || y >= inner.height as isize {
            continue;
        }
        let rect = Rect::new(inner.x, inner.y + y as u16, inner.width, 1);
        styled_row(
            frame,
            rect,
            selected_line(event_line(e, s.tz), n == s.event_index),
        );
        hit(s, rect, Action::Event(e.id));
        if y + 1 < inner.height as isize {
            styled_row(
                frame,
                Rect::new(inner.x, rect.y + 1, inner.width, 1),
                timing_line(e, s.tz),
            );
        }
    }
}
/// Greedy interval partitioning: each overlapping segment receives a distinct
/// lane; intervals meeting exactly at an end boundary may reuse a lane.
#[derive(Clone, Debug)]
pub struct Segment {
    pub event: usize,
    pub start: u32,
    pub end: u32,
    pub lane: usize,
    pub before: bool,
    pub after: bool,
    instant_start: DateTime<Utc>,
    instant_end: DateTime<Utc>,
}
pub fn segments(events: &[CalendarEvent], date: NaiveDate, tz: chrono_tz::Tz) -> Vec<Segment> {
    let mut segments = Vec::new();
    for (i, e) in events.iter().enumerate() {
        if let EventTiming::Timed { start, end } = e.timing {
            let end = end.unwrap_or(start + Duration::hours(1));
            let instant_start = start;
            let instant_end = end;
            let start = start.with_timezone(&tz);
            let end = end.with_timezone(&tz);
            if start.date_naive() > date
                || end.date_naive() < date
                || (end.date_naive() == date && end.time() == chrono::NaiveTime::MIN)
            {
                continue;
            }
            let before = start.date_naive() < date;
            let after = end.date_naive() > date;
            let a = if before {
                0
            } else {
                start.hour() * 60 + start.minute()
            };
            let b = if after {
                1440
            } else {
                end.hour() * 60 + end.minute()
            };
            segments.push(Segment {
                event: i,
                start: a,
                end: b.max(a + 1),
                lane: 0,
                before,
                after,
                instant_start,
                instant_end,
            });
        }
    }
    segments.sort_by_key(|s| (s.start, s.end, s.event));
    let mut lanes: Vec<Vec<usize>> = Vec::new();
    for i in 0..segments.len() {
        let s = &segments[i];
        // A repeated DST hour can reverse wall-clock endpoints. Both actual
        // overlaps and collisions on the civil-hour grid need distinct lanes.
        let lane = lanes
            .iter()
            .position(|previous| {
                previous.iter().all(|&j| {
                    let p = &segments[j];
                    p.end <= s.start
                        && (p.instant_end <= s.instant_start || s.instant_end <= p.instant_start)
                })
            })
            .unwrap_or(lanes.len());
        if lane == lanes.len() {
            lanes.push(Vec::new());
        }
        lanes[lane].push(i);
        segments[i].lane = lane;
    }
    segments
}
fn draw_hours(frame: &mut Frame, area: Rect, s: &CalendarState) {
    if area.width < 12 || area.height < 5 {
        styled_row(
            frame,
            area,
            hint_line(&[("Enter:", "agenda"), ("PgUp/PgDn", "scroll hours")]),
        );
        return;
    }
    let (from, to) = s.range();
    let n = (to - from).num_days();
    let timeline_y = area.y + 5;
    let visible = area.height - 5;
    s.hour_rows.set(visible as usize);
    let scroll = s.hour_scroll.min(47) as u32 * 30;
    pane(s, area, Pane::Grid);
    styled_row(
        frame,
        Rect::new(area.x, area.y, area.width, 1),
        hint_line(&[("PgUp/PgDn", "hours"), ("Ctrl+←/→", "columns")]),
    );
    for r in 0..visible {
        let minute = scroll + r as u32 * 30;
        if minute >= 1440 {
            break;
        }
        row(
            frame,
            Rect::new(area.x, timeline_y + r, 5, 1),
            if minute.is_multiple_of(60) {
                format!("{:02}:00", minute / 60)
            } else {
                "    ·".into()
            },
            if minute.is_multiple_of(60) {
                muted()
            } else {
                rule()
            },
        );
    }
    let viewport = Rect::new(area.x + 6, area.y + 1, area.width - 6, area.height - 1);
    let columns: Vec<_> = (0..n)
        .map(|day| {
            let date = from + Duration::days(day);
            let pieces = segments(&s.events, date, s.tz);
            let lanes = pieces.iter().map(|p| p.lane + 1).max().unwrap_or(1);
            (
                date,
                pieces,
                (lanes.saturating_mul(12).min(u16::MAX as usize) as u16).max(20),
            )
        })
        .collect();
    if s.reveal_selected.replace(false) || s.hours_geometry.get() != viewport {
        let mut offset = 0;
        for (date, _, width) in &columns {
            if *date == s.selected {
                let visible_width = (*width as usize).min(viewport.width as usize);
                let current = s.day_scroll.get();
                if offset < current || offset + visible_width > current + viewport.width as usize {
                    s.day_scroll
                        .set(if *width as usize >= viewport.width as usize {
                            offset
                        } else {
                            (offset + visible_width).saturating_sub(viewport.width as usize)
                        });
                }
                break;
            }
            offset += *width as usize;
        }
    }
    s.hours_geometry.set(viewport);
    let selected_event = s.selected_event().map(|event| event.id);
    let mut virtual_x = 0i32;
    for (date, pieces, width) in columns {
        let origin = viewport.x as i32 + virtual_x - s.day_scroll.get() as i32;
        let clip = |x: i32, y: u16, w: u16, h: u16| -> Rect {
            let left = x.max(viewport.x as i32);
            let right = (x + w as i32).min(viewport.right() as i32);
            Rect::new(
                left.max(0) as u16,
                y,
                right.saturating_sub(left).max(0) as u16,
                h,
            )
        };
        let head = clip(origin, area.y + 1, width - 1, 1);
        let style = if date == s.today() {
            accent().add_modifier(Modifier::UNDERLINED)
        } else if date == s.selected {
            accent()
        } else {
            muted()
        };
        styled_row(
            frame,
            head,
            selected_line(
                Line::styled(date.format("%a %b %d").to_string(), style),
                date == s.selected,
            ),
        );
        hit(s, head, Action::Date(date));
        let all: Vec<_> = s
            .day_events(date)
            .into_iter()
            .filter(|e| matches!(e.timing, EventTiming::AllDay { .. }))
            .collect();
        for (i, e) in all.iter().take(2).enumerate() {
            let rect = clip(origin, area.y + 2 + i as u16, width - 1, 1);
            styled_row(
                frame,
                rect,
                selected_line(event_line(e, s.tz), selected_event == Some(e.id)),
            );
            hit(s, rect, Action::Event(e.id));
        }
        if all.len() > 2 {
            let rect = clip(origin, area.y + 4, width - 1, 1);
            row(frame, rect, format!("+{} all-day", all.len() - 2), dim());
            hit(s, rect, Action::Date(date));
        }
        for r in 0..visible {
            let rect = clip(origin, timeline_y + r, width - 1, 1);
            row(
                frame,
                rect,
                if r % 2 == 0 {
                    "─".repeat(width as usize - 1)
                } else {
                    " ".repeat(width as usize - 1)
                },
                rule(),
            );
        }
        for piece in pieces {
            let bottom = scroll + visible as u32 * 30;
            if piece.end <= scroll || piece.start >= bottom {
                continue;
            }
            let y = ((piece.start.max(scroll) - scroll) / 30) as u16;
            let end = ((piece.end.min(bottom) - scroll).div_ceil(30)) as u16;
            let rect = clip(
                origin + piece.lane as i32 * 12,
                timeline_y + y,
                11,
                (end - y).max(1).min(visible - y),
            );
            let e = &s.events[piece.event];
            let mut spans = Vec::new();
            if piece.before || piece.start < scroll {
                spans.push(Span::styled("↑ ", accent()));
            }
            if piece.after || piece.end > bottom {
                spans.push(Span::styled("↓ ", accent()));
            }
            if e.owner_id.is_none() {
                spans.push(source_span(e));
                spans.push(Span::raw(" "));
            }
            spans.push(Span::styled(e.title.clone(), bright()));
            spans.push(separator());
            if let EventTiming::Timed { start, .. } = e.timing {
                spans.push(Span::styled(local_time_label(start, s.tz, false), muted()));
            }
            let selected = selected_event == Some(e.id);
            let fill = if selected {
                theme::selection_style()
            } else if theme::BG_CANVAS() == ratatui::style::Color::Reset {
                Style::default()
            } else {
                Style::default().bg(theme::BG_HIGHLIGHT())
            };
            // Clear the hour rules before painting a card, including its padding.
            frame.render_widget(Clear, rect);
            frame.render_widget(
                Paragraph::new(patch_line(Line::from(spans), fill))
                    .wrap(Wrap { trim: false })
                    .style(base().patch(fill)),
                rect,
            );
            hit(s, rect, Action::Event(e.id));
        }
        virtual_x += width as i32;
    }
    s.max_days
        .set((virtual_x - viewport.width as i32).max(0) as usize);
}
fn list_lines<'a>(
    s: &CalendarState,
    events: &[&'a CalendarEvent],
    upcoming: bool,
) -> Vec<(Line<'static>, Option<&'a CalendarEvent>, usize)> {
    let mut rows = Vec::new();
    let mut last = None;
    for (n, e) in events.iter().enumerate() {
        let date = e.timing.dates(s.tz).0.max(if upcoming {
            NaiveDate::MIN
        } else {
            month_start(s.selected)
        });
        if last != Some(date) {
            rows.push((
                Line::styled(date.format("%A, %B %d, %Y").to_string(), accent()),
                None,
                n,
            ));
            last = Some(date);
        }
        rows.push((event_line(e, s.tz), Some(*e), n));
    }
    rows
}
fn draw_list(frame: &mut Frame, area: Rect, s: &CalendarState, upcoming: bool) {
    let owned = s.upcoming();
    let events = if upcoming {
        owned.iter().collect()
    } else {
        s.ordered_events()
    };
    let rows = list_lines(s, &events, upcoming);
    s.max_scroll
        .set(rows.len().saturating_sub(area.height as usize));
    pane(s, area, if upcoming { Pane::Upcoming } else { Pane::List });
    if rows.is_empty() {
        row(
            frame,
            area,
            if upcoming {
                "No upcoming notices"
            } else {
                "No events this month"
            },
            dim(),
        );
    }
    for (r, (text, event, n)) in rows
        .iter()
        .skip(s.scroll.min(s.max_scroll.get()))
        .take(area.height as usize)
        .enumerate()
    {
        let rect = Rect::new(area.x, area.y + r as u16, area.width, 1);
        styled_row(
            frame,
            rect,
            selected_line(text.clone(), event.is_some() && *n == s.event_index),
        );
        if let Some(e) = event {
            hit(s, rect, Action::Event(e.id));
        }
    }
}
pub fn draw_upcoming_panel(frame: &mut Frame, area: Rect, s: &CalendarState) {
    let notices = s.upcoming();
    let heading = Line::from(vec![
        Span::raw(" "),
        Span::styled("u", key_style()),
        Span::styled(" Upcoming events", bright().add_modifier(Modifier::BOLD)),
        Span::styled(format!(" ({}) ", notices.len()), dim()),
    ]);
    if area.height == 1 {
        let mut heading = heading;
        heading.spans.remove(0);
        styled_row(frame, area, heading);
        hit(s, area, Action::Upcoming);
        return;
    }
    let b = border(heading);
    let inner = b.inner(area);
    frame.render_widget(b, area);
    hit(s, area, Action::Upcoming);
    if notices.is_empty() {
        row(frame, inner, "No upcoming notices", dim());
    }
    for (i, e) in notices
        .iter()
        .take(3)
        .take(inner.height as usize)
        .enumerate()
    {
        let rect = Rect::new(inner.x, inner.y + i as u16, inner.width, 1);
        let mut spans = vec![
            source_span(e),
            Span::raw(" "),
            Span::styled(e.title.clone(), bright()),
            separator(),
        ];
        spans.extend(timing_line(e, s.tz).spans);
        styled_row(frame, rect, Line::from(spans));
        hit(s, rect, Action::Event(e.id));
    }
    if notices.len() > 3 && inner.height > 0 {
        let label = Line::from(vec![
            Span::styled(format!("+{} more", notices.len() - 3), muted()),
            separator(),
            Span::styled("u", key_style()),
        ]);
        let w = (label.width() as u16).min(inner.width);
        let rect = Rect::new(inner.right() - w, area.y, w, 1);
        styled_row(frame, rect, label);
        hit(s, rect, Action::Upcoming);
    }
}
fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let width = w.min(area.width);
    let height = h.min(area.height);
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    )
}
pub fn draw_modal(frame: &mut Frame, area: Rect, s: &CalendarState) {
    let Some(modal) = &s.modal else {
        return;
    };
    s.hits.borrow_mut().clear();
    s.panes.borrow_mut().clear();
    let rect = centered(
        area,
        76,
        match modal {
            Modal::Editor(_) => 33,
            Modal::Details(_) => 24,
            Modal::Settings { .. } => 12,
            Modal::Go(_) => 12,
            Modal::Source(_) => ((s.public.len() + 5).min(24)) as u16,
            _ => 20,
        },
    );
    frame.render_widget(Clear, rect);
    let title = match modal {
        Modal::Editor(_) => " Event editor ",
        Modal::Details(_) => " Event details ",
        Modal::Settings { .. } => " Calendar Settings ",
        Modal::Source(_) => " Calendar source ",
        Modal::View(_) => " Calendar view ",
        Modal::Go(_) => " Go to date ",
        Modal::Delete(_) => " Delete event? ",
        Modal::Upcoming => " Upcoming events ",
        Modal::Agenda => " Selected-day agenda ",
    };
    let block = border(title)
        .title_style(accent())
        .border_style(base().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(rect).inner(Margin::new(1, 0));
    frame.render_widget(block, rect);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    match modal {
        Modal::Source(selected) => {
            let labels: Vec<_> = std::iter::once(Line::styled("Server", bright()))
                .chain(std::iter::once(Line::styled(
                    "My personal calendar",
                    bright(),
                )))
                .chain(s.public.iter().map(|p| {
                    Line::from(vec![
                        Span::styled(format!("@{}", p.username), bright()),
                        separator(),
                        Span::styled("public, read-only", dim()),
                    ])
                }))
                .collect();
            let offset = selected.saturating_sub((inner.height as usize).saturating_sub(1));
            for (i, label) in labels
                .iter()
                .enumerate()
                .skip(offset)
                .take(inner.height as usize)
            {
                let r = Rect::new(inner.x, inner.y + (i - offset) as u16, inner.width, 1);
                styled_row(frame, r, selected_line(label.clone(), i == *selected));
                hit(s, r, Action::Choice(i));
            }
        }
        Modal::View(selected) => {
            for (i, v) in CalendarView::ALL
                .iter()
                .enumerate()
                .take(inner.height as usize)
            {
                let r = Rect::new(inner.x, inner.y + i as u16, inner.width, 1);
                styled_row(
                    frame,
                    r,
                    selected_line(Line::styled(v.label(), bright()), i == *selected),
                );
                hit(s, r, Action::Choice(i));
            }
        }
        Modal::Go(input) => {
            let mut ta = (**input).clone();
            ta.set_style(base());
            ta.set_cursor_line_style(base());
            ta.set_cursor_style(accent().patch(theme::selection_style()));
            frame.render_widget(&ta, Rect::new(inner.x, inner.y, inner.width, 1));
            if inner.height >= 3 {
                let text = input.lines().join("");
                let result = date_entry::parse(&text, s.selected, s.today());
                let preview = if let Some(error) = &s.error {
                    Line::styled(error, base().fg(theme::ERROR()))
                } else if let Ok(date) = result {
                    Line::from(vec![
                        Span::styled("Go to ", dim()),
                        Span::styled(date.to_string(), accent()),
                        separator(),
                        Span::styled(date.format("%A").to_string(), muted()),
                    ])
                } else {
                    Line::styled("Enter a date or calendar offset", dim())
                };
                let lines = vec![
                    preview,
                    Line::from(vec![
                        Span::styled("Offsets from ", dim()),
                        Span::styled(s.selected.to_string(), bright()),
                    ]),
                    Line::from(vec![
                        Span::styled("Today ", dim()),
                        Span::styled(s.today().to_string(), bright()),
                        separator(),
                        Span::styled(s.tz.to_string(), dim()),
                    ]),
                    Line::from(vec![
                        Span::styled("Oct 2, 2026", muted()),
                        separator(),
                        Span::styled("2 Oct", muted()),
                        separator(),
                        Span::styled("2026/10/2", muted()),
                    ]),
                    Line::from(vec![
                        Span::styled("2 months ago", muted()),
                        separator(),
                        Span::styled("in 3 weeks", muted()),
                        separator(),
                        Span::styled("+2w", muted()),
                    ]),
                ];
                frame.render_widget(
                    Paragraph::new(lines)
                        .wrap(Wrap { trim: false })
                        .style(base()),
                    Rect::new(inner.x, inner.y + 1, inner.width, inner.height - 2),
                );
                let mut x = inner.x;
                button(
                    frame,
                    s,
                    &mut x,
                    inner.bottom() - 1,
                    inner.right(),
                    "Go (Enter)",
                    Action::Save,
                    false,
                );
                button(
                    frame,
                    s,
                    &mut x,
                    inner.bottom() - 1,
                    inner.right(),
                    "Cancel (Esc)",
                    Action::Cancel,
                    false,
                );
            }
        }
        Modal::Settings { draft, focus } => {
            let labels = [
                labeled(
                    "Week starts",
                    if draft.week_start == 0 {
                        "Monday"
                    } else {
                        "Sunday"
                    },
                    bright(),
                ),
                labeled("Default view", draft.default_view.label(), bright()),
                labeled(
                    "Server overlay",
                    if draft.server_overlay {
                        "Enabled"
                    } else {
                        "Disabled"
                    },
                    bright(),
                ),
                labeled(
                    "Personal calendar",
                    if draft.public {
                        "Public to signed-in users"
                    } else {
                        "Private"
                    },
                    bright(),
                ),
                Line::styled("Save", bright().add_modifier(Modifier::BOLD)),
                Line::styled("Cancel", dim()),
            ];
            for (i, label) in labels.iter().enumerate().take(inner.height as usize) {
                let r = Rect::new(inner.x, inner.y + i as u16, inner.width, 1);
                styled_row(frame, r, selected_line(label.clone(), i == *focus));
                hit(
                    s,
                    r,
                    if i == 4 {
                        Action::Save
                    } else if i == 5 {
                        Action::Cancel
                    } else {
                        Action::ToggleField(i)
                    },
                );
            }
            if inner.height > 7 {
                let line = s.error.as_ref().map_or_else(
                    || {
                        hint_line(&[
                            ("Tab", "focus"),
                            ("Enter/Space", "change"),
                            ("Ctrl+S", "save"),
                        ])
                    },
                    |error| Line::styled(error.clone(), base().fg(theme::ERROR())),
                );
                styled_row(frame, Rect::new(inner.x, inner.y + 7, inner.width, 1), line);
            }
        }
        Modal::Editor(e) => draw_editor(frame, inner, s, e),
        Modal::Delete(e) => {
            styled_row(
                frame,
                inner,
                Line::from(vec![
                    Span::styled("Delete “", dim()),
                    Span::styled(e.title.clone(), bright().add_modifier(Modifier::BOLD)),
                    Span::styled("”?", dim()),
                ]),
            );
            let mut x = inner.x;
            button(
                frame,
                s,
                &mut x,
                inner.y + 2,
                inner.right(),
                "y Delete",
                Action::Save,
                false,
            );
            button(
                frame,
                s,
                &mut x,
                inner.y + 2,
                inner.right(),
                "n Cancel",
                Action::Cancel,
                false,
            );
            if let Some(error) = &s.error {
                row(
                    frame,
                    Rect::new(inner.x, inner.y + 4, inner.width, 1),
                    error,
                    base().fg(theme::ERROR()),
                );
            }
        }
        Modal::Details(e) => {
            let access = event_access(e, s.viewer, s.role);
            let mut lines = vec![
                Line::styled(&e.title, bright().add_modifier(Modifier::BOLD)),
                timing_line(e, s.tz),
                Line::from(vec![
                    source_span(e),
                    separator(),
                    Span::styled(s.tz.to_string(), dim()),
                    separator(),
                    Span::styled(format!("revision {}", e.revision), dim()),
                ]),
                Line::from(""),
            ];
            lines.extend(e.description.lines().map(|s| Line::from(s.to_owned())));
            if access.notifications {
                lines.push(labeled(
                    "Notifications",
                    e.notice_lead_seconds
                        .map(|n| {
                            format!(
                                "{} before",
                                humantime::format_duration(std::time::Duration::from_secs(
                                    n as u64
                                ))
                            )
                        })
                        .unwrap_or_else(|| "disabled".into()),
                    if e.notice_lead_seconds.is_some() {
                        bright()
                    } else {
                        dim()
                    },
                ));
            }
            let content = Rect::new(
                inner.x,
                inner.y,
                inner.width,
                inner.height.saturating_sub(2),
            );
            pane(s, content, Pane::List);
            let p = Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .style(base());
            s.max_scroll.set(
                p.line_count(content.width)
                    .saturating_sub(content.height as usize),
            );
            frame.render_widget(
                p.scroll((s.scroll.min(s.max_scroll.get()) as u16, 0)),
                content,
            );
            let mut x = inner.x;
            if access.edit {
                button(
                    frame,
                    s,
                    &mut x,
                    inner.bottom() - 1,
                    inner.right(),
                    "e Edit",
                    Action::Edit,
                    false,
                );
                button(
                    frame,
                    s,
                    &mut x,
                    inner.bottom() - 1,
                    inner.right(),
                    "Delete",
                    Action::Delete,
                    false,
                );
            }
            button(
                frame,
                s,
                &mut x,
                inner.bottom() - 1,
                inner.right(),
                "Close",
                Action::Cancel,
                false,
            );
        }
        Modal::Upcoming => draw_list(frame, inner, s, true),
        Modal::Agenda => draw_agenda(frame, inner, s),
    }
    if s.pending {
        row(
            frame,
            Rect::new(inner.x, inner.bottom() - 1, inner.width, 1),
            "Saving…",
            accent(),
        );
    }
}
fn draw_editor(frame: &mut Frame, inner: Rect, s: &CalendarState, e: &super::state::Editor) {
    if inner.height < 4 {
        return;
    }
    if e.discard_prompt {
        row(frame, inner, "Discard unsaved changes?", accent());
        let mut x = inner.x;
        button(
            frame,
            s,
            &mut x,
            inner.y + 2,
            inner.right(),
            "d Discard",
            Action::Discard,
            false,
        );
        button(
            frame,
            s,
            &mut x,
            inner.y + 2,
            inner.right(),
            "k Keep editing",
            Action::Keep,
            false,
        );
        return;
    }
    let rows = [
        ("Title", Some(0), 1u16),
        ("Description", Some(1), 3),
        ("Start date", Some(2), 1),
        ("Start time", Some(3), 1),
        ("End date (inclusive for all-day)", Some(4), 1),
        ("End time", Some(5), 1),
        ("All day", None, 1),
        ("Notifications", None, 1),
        ("Lead time (e.g. 1 day, 1h 30m)", Some(6), 1),
        ("Moderator delegation", None, 1),
        ("Repeated start time", None, 1),
        ("Repeated end time", None, 1),
    ];
    // Keep focused controls visible on compact terminals. The complete editor
    // remains reachable via Tab, Shift+Tab, and explicit Save/Cancel.
    let start = if inner.height < 29 {
        e.focus.min(11).saturating_sub(2)
    } else {
        0
    };
    let mut y = inner.y;
    for (i, (label, field, height)) in rows.iter().enumerate().skip(start) {
        let h = *height;
        if y + h + 1 > inner.bottom().saturating_sub(3) {
            break;
        }
        let r = Rect::new(inner.x, y, inner.width, h + 1);
        hit(
            s,
            r,
            if i == 12 {
                Action::Save
            } else if i == 13 {
                Action::Cancel
            } else {
                Action::Field(i)
            },
        );
        let label_style = if e.focus == i { accent() } else { muted() };
        let label = match label.split_once(" (") {
            Some((name, note)) => Line::from(vec![
                Span::styled(name.to_owned(), label_style),
                Span::styled(format!(" ({note}"), dim()),
            ]),
            None => Line::styled((*label).to_owned(), label_style),
        };
        styled_row(frame, Rect::new(r.x, r.y, r.width, 1), label);
        let value = Rect::new(r.x + 1, r.y + 1, r.width.saturating_sub(1), h);
        if let Some(n) = field {
            let mut ta = e.fields[*n].clone();
            let field_style = if i == 0 { bright() } else { base() };
            ta.set_style(field_style);
            ta.set_cursor_line_style(field_style);
            ta.set_cursor_style(if e.focus == i {
                accent().patch(theme::selection_style())
            } else {
                field_style
            });
            frame.render_widget(&ta, value);
        } else {
            let text = match i {
                6 => if e.all_day { "[x]" } else { "[ ]" }.to_string(),
                7 => {
                    if e.access.notifications {
                        format!("[{}]", if e.notifications { 'x' } else { ' ' })
                    } else {
                        "Admin controls notifications".into()
                    }
                }
                9 => {
                    if e.access.delegate {
                        format!("[{}]", if e.delegated { 'x' } else { ' ' })
                    } else {
                        if e.delegated {
                            "Mod-editable"
                        } else {
                            "Not delegated"
                        }
                        .into()
                    }
                }
                10 => match e.occurrence {
                    None => "Choose if local time repeats",
                    Some(Occurrence::Earlier) => "Earlier occurrence",
                    Some(Occurrence::Later) => "Later occurrence",
                }
                .into(),
                11 => match e.end_occurrence {
                    None => "Choose if local time repeats",
                    Some(Occurrence::Earlier) => "Earlier occurrence",
                    Some(Occurrence::Later) => "Later occurrence",
                }
                .into(),
                _ => String::new(),
            };
            let style = match i {
                6 if e.all_day => accent(),
                7 if e.access.notifications && e.notifications => accent(),
                9 if e.access.delegate && e.delegated => accent(),
                10 if e.occurrence.is_some() => bright(),
                11 if e.end_occurrence.is_some() => bright(),
                _ => dim(),
            };
            row(frame, value, text, style);
            hit(
                s,
                value,
                if matches!(i, 6 | 7 | 9 | 10 | 11) {
                    Action::ToggleField(i)
                } else if i == 12 {
                    Action::Save
                } else {
                    Action::Cancel
                },
            );
        }
        y += h + 1;
    }
    let mut footer = vec![Span::styled(s.tz.to_string(), dim()), separator()];
    let mut controls = hint_line(&[("Tab", "fields"), ("Space", "toggles")]);
    controls.spans.remove(0);
    footer.extend(controls.spans);
    styled_row(
        frame,
        Rect::new(inner.x, inner.bottom() - 3, inner.width, 1),
        Line::from(footer),
    );
    if let Some(error) = &e.error {
        row(
            frame,
            Rect::new(inner.x, inner.bottom() - 2, inner.width, 1),
            error,
            base().fg(theme::ERROR()),
        );
        if e.existing.is_some() {
            hit(
                s,
                Rect::new(inner.x, inner.bottom() - 2, inner.width, 1),
                Action::Reload,
            );
        }
    } else {
        let hint = match e.focus {
            2 | 4 => Some(("Dates", "Oct 2 or +2w", "offsets from today")),
            8 => Some(("Lead", "1 day, 24h or 1h 30m", "nonnegative")),
            _ => None,
        };
        styled_row(
            frame,
            Rect::new(inner.x, inner.bottom() - 2, inner.width, 1),
            hint.map_or_else(Line::default, |(label, example, note)| {
                let mut line = labeled(label, example, muted());
                line.spans.push(separator());
                line.spans.push(Span::styled(note, dim()));
                line
            }),
        );
    }
    let mut x = inner.x;
    button(
        frame,
        s,
        &mut x,
        inner.bottom() - 1,
        inner.right(),
        "Save",
        Action::Save,
        e.focus == 12,
    );
    button(
        frame,
        s,
        &mut x,
        inner.bottom() - 1,
        inner.right(),
        "Cancel",
        Action::Cancel,
        e.focus == 13,
    );
}
