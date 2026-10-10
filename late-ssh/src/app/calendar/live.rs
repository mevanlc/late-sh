//! A board event on the live strip (`app/live/`) and the Live panel: an hour
//! before it starts the room sees what it is, who posted it and how many are
//! in; when it starts it goes up again, lit. A drawn calendar page sits in
//! the picture column; the words beside it are the title, how long until
//! it starts or that it is on, the count, and who posted it. The viewer's
//! own events ride the same surfaces for the viewer alone.

use chrono::{DateTime, Datelike, Utc};
use chrono_tz::Tz;
use late_core::models::calendar::{CalendarEvent, EventTiming, STRIP_LEAD};
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use uuid::Uuid;

use crate::app::{
    common::theme,
    live::{
        pick::{LiveCandidate, LiveSource},
        ui::{HintPart, PICTURE_COLS, PICTURE_ROWS, StripBody, key_hint_spans, truncate_chars},
    },
    pot::state::lead_time,
};

/// An event as the live strip paints it.
#[derive(Clone, Debug)]
pub struct EventStripView {
    pub event: CalendarEvent,
    pub tz: Tz,
    pub now: DateTime<Utc>,
}

/// Every upcoming event inside [`STRIP_LEAD`] of its start or already on.
/// Stamped an hour before its start, and again at the start, so it joins the
/// queue twice: once as a heads-up, once as the thing happening. Both stamps
/// are the row's own instant, shared by every replica.
pub(crate) fn candidates(upcoming: &[CalendarEvent], now: DateTime<Utc>) -> Vec<LiveCandidate> {
    upcoming
        .iter()
        .filter(|event| event.on_strip(now))
        .map(|event| LiveCandidate {
            source: LiveSource::BoardEvent(event.id),
            updated: if event.started(now) {
                event.starts_at
            } else {
                event.starts_at - STRIP_LEAD
            },
            aimed_at: None,
        })
        .collect()
}

/// One event as the strip paints it. `None` once it is over or gone.
pub(crate) fn view(
    upcoming: &[CalendarEvent],
    id: Uuid,
    tz: Tz,
    now: DateTime<Utc>,
) -> Option<EventStripView> {
    upcoming
        .iter()
        .find(|event| event.id == id && event.on_strip(now))
        .map(|event| EventStripView {
            event: event.clone(),
            tz,
            now,
        })
}

pub(crate) fn body(budget: usize, strip: &EventStripView) -> StripBody {
    StripBody {
        picture: page_lines(strip),
        words: word_rows(budget, strip),
        hint: key_hint_spans(
            budget,
            &[
                HintPart::Key("o"),
                HintPart::Text(" or click for the board"),
            ],
        ),
        glow: glow(strip),
    }
}

/// Lit once the event is on.
pub(crate) fn glow(strip: &EventStripView) -> bool {
    strip.event.started(strip.now)
}

/// `event mat · Movie night · in 58m`, after the rule label.
pub(crate) fn compact_spans(rest: u16, strip: &EventStripView) -> Vec<Span<'static>> {
    let rest = usize::from(rest);
    let lead = format!("event {} · ", poster(strip));
    let lead = truncate_chars(&lead, rest);
    let left = rest.saturating_sub(lead.chars().count());
    let tail = format!(" · {}", when(strip));
    let title_room = left.saturating_sub(tail.chars().count());
    vec![
        Span::styled(lead, Style::default().fg(theme::TEXT_DIM())),
        Span::styled(
            truncate_chars(&strip.event.title, title_room),
            Style::default().fg(theme::TEXT()),
        ),
        Span::styled(
            truncate_chars(&tail, left.saturating_sub(title_room)),
            Style::default().fg(theme::TEXT_DIM()),
        ),
    ]
}

/// `in 58m`, `in 1h`, or `on now`.
pub(crate) fn when(strip: &EventStripView) -> String {
    when_at(&strip.event, strip.now)
}

pub(crate) fn when_at(event: &CalendarEvent, now: DateTime<Utc>) -> String {
    if event.started(now) {
        "on now".to_string()
    } else {
        format!(
            "in {}",
            lead_time((event.starts_at - now).num_seconds().max(60))
        )
    }
}

fn poster(strip: &EventStripView) -> String {
    if strip.event.is_board() {
        strip.event.creator_name.clone()
    } else {
        "you".to_string()
    }
}

/// A calendar page: the month on top, the day large, the weekday and the
/// start time under it.
fn page_lines(strip: &EventStripView) -> Vec<Line<'static>> {
    let inner = usize::from(PICTURE_COLS) - 2;
    let frame = Style::default().fg(theme::BORDER_DIM());
    let edge = |left: &str, right: &str| {
        Line::from(Span::styled(
            format!("{left}{}{right}", "─".repeat(inner)),
            frame,
        ))
    };
    let centered = |text: String, style: Style| {
        let text = truncate_chars(&text, inner);
        let lead = (inner - text.chars().count()) / 2;
        Line::from(vec![
            Span::styled("│".to_string(), frame),
            Span::raw(" ".repeat(lead)),
            Span::styled(text.clone(), style),
            Span::raw(" ".repeat(inner - lead - text.chars().count())),
            Span::styled("│".to_string(), frame),
        ])
    };
    let start = strip.event.starts_at.with_timezone(&strip.tz);
    let time = match strip.event.timing {
        EventTiming::AllDay { .. } => "all day".to_string(),
        EventTiming::Timed { .. } => start.format("%H:%M %Z").to_string(),
    };
    vec![
        edge("╭", "╮"),
        centered(
            start.format("%b").to_string().to_uppercase(),
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ),
        centered(String::new(), frame),
        centered(
            start.day().to_string(),
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ),
        centered(
            start.format("%A").to_string(),
            Style::default().fg(theme::TEXT_DIM()),
        ),
        centered(time, Style::default().fg(theme::TEXT())),
        edge("╰", "╯"),
    ]
}

/// The words beside the page, one entry per picture row: the title, when
/// and how many, who posted it.
fn word_rows(budget: usize, strip: &EventStripView) -> Vec<Vec<Span<'static>>> {
    let mut rows: Vec<Vec<Span<'static>>> = (0..PICTURE_ROWS).map(|_| Vec::new()).collect();
    if budget == 0 {
        return rows;
    }
    rows[1] = vec![Span::styled(
        truncate_chars(&strip.event.title, budget),
        Style::default()
            .fg(theme::TEXT())
            .add_modifier(Modifier::BOLD),
    )];
    let going = match (strip.event.is_board(), strip.event.going) {
        (false, _) => String::new(),
        (true, 0) => " · nobody's in yet".to_string(),
        (true, n) => format!(" · {n} in"),
    };
    rows[2] = vec![
        Span::styled(
            when(strip),
            if glow(strip) {
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::TEXT())
            },
        ),
        Span::styled(
            truncate_chars(&going, budget.saturating_sub(when(strip).chars().count())),
            Style::default().fg(theme::TEXT_DIM()),
        ),
    ];
    rows[3] = poster_spans(budget, strip);
    rows
}

/// `mat posted it`, the poster in amber; `your event` on a personal one.
fn poster_spans(budget: usize, strip: &EventStripView) -> Vec<Span<'static>> {
    if !strip.event.is_board() {
        return vec![Span::styled(
            truncate_chars("your event", budget),
            Style::default().fg(theme::TEXT_DIM()),
        )];
    }
    let name = truncate_chars(&strip.event.creator_name, budget);
    let left = budget.saturating_sub(name.chars().count());
    vec![
        Span::styled(
            name,
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            truncate_chars(" posted it", left),
            Style::default().fg(theme::TEXT()),
        ),
    ]
}
