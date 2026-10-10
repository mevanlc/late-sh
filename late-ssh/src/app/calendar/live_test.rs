use super::live::*;
use crate::app::live::{pick::LiveSource, ui::PICTURE_COLS};
use chrono::{Duration, TimeZone, Utc};
use late_core::models::calendar::{CalendarEvent, EventTiming, STRIP_LEAD};
use uuid::Uuid;

fn event(id: u128, start: chrono::DateTime<Utc>, board: bool, going: i64) -> CalendarEvent {
    CalendarEvent {
        id: Uuid::from_u128(id),
        owner_id: (!board).then_some(Uuid::nil()),
        creator_id: Uuid::nil(),
        creator_name: "mat".into(),
        title: "Movie night".into(),
        description: String::new(),
        timing: EventTiming::Timed {
            start,
            end: Some(start + Duration::hours(2)),
        },
        creator_timezone: "UTC".into(),
        starts_at: start,
        ends_at: start + Duration::hours(2),
        going,
        revision: 1,
    }
}

fn text(spans: &[ratatui::text::Span<'_>]) -> String {
    spans.iter().map(|span| span.content.as_ref()).collect()
}

fn line_text(line: &ratatui::text::Line<'_>) -> String {
    text(&line.spans)
}

/// An event joins the strip an hour before it starts, stamped then, and
/// joins again at its start, stamped then; before the hour and after the
/// end it is not offered.
#[test]
fn an_event_joins_an_hour_before_and_again_at_its_start() {
    let now = Utc.with_ymd_and_hms(2026, 10, 10, 20, 0, 0).unwrap();
    let soon = event(1, now + Duration::minutes(30), true, 2);
    let on = event(2, now - Duration::minutes(10), true, 0);
    let far = event(3, now + Duration::hours(3), true, 0);
    let over = event(4, now - Duration::hours(3), true, 0);
    let upcoming = [soon.clone(), on.clone(), far, over];
    let candidates = candidates(&upcoming, now);
    assert_eq!(candidates.len(), 2);
    assert_eq!(candidates[0].source, LiveSource::BoardEvent(soon.id));
    assert_eq!(candidates[0].updated, soon.starts_at - STRIP_LEAD);
    assert_eq!(candidates[1].source, LiveSource::BoardEvent(on.id));
    assert_eq!(candidates[1].updated, on.starts_at);
    assert!(view(&upcoming, Uuid::from_u128(4), chrono_tz::UTC, now).is_none());
    assert!(view(&upcoming, Uuid::from_u128(3), chrono_tz::UTC, now).is_none());
}

/// What the strip says: how long until it starts, that it is on, how
/// many are in, and who posted it; the viewer's own event says so.
#[test]
fn the_words_say_when_how_many_and_who() {
    let now = Utc.with_ymd_and_hms(2026, 10, 10, 20, 0, 0).unwrap();
    let soon = event(1, now + Duration::minutes(30), true, 2);
    let strip = view(&[soon], Uuid::from_u128(1), chrono_tz::UTC, now).unwrap();
    assert_eq!(when(&strip), "in 30m");
    assert!(!glow(&strip));
    let painted = body(40, &strip);
    assert_eq!(text(&painted.words[1]), "Movie night");
    assert_eq!(text(&painted.words[2]), "in 30m · 2 in");
    assert_eq!(text(&painted.words[3]), "mat posted it");
    assert_eq!(painted.picture.len(), 7);
    for line in &painted.picture {
        assert_eq!(line.width(), usize::from(PICTURE_COLS));
    }
    assert!(line_text(&painted.picture[1]).contains("OCT"));
    assert!(line_text(&painted.picture[5]).contains("20:30"));
    assert_eq!(
        text(&compact_spans(60, &strip)),
        "event mat · Movie night · in 30m"
    );

    let on = event(2, now - Duration::minutes(10), false, 0);
    let strip = view(&[on], Uuid::from_u128(2), chrono_tz::UTC, now).unwrap();
    assert_eq!(when(&strip), "on now");
    assert!(glow(&strip));
    let painted = body(40, &strip);
    assert_eq!(text(&painted.words[2]), "on now");
    assert_eq!(text(&painted.words[3]), "your event");
    assert_eq!(
        text(&compact_spans(60, &strip)),
        "event you · Movie night · on now"
    );
}
