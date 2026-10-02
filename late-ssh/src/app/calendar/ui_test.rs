use super::{state::*, svc::CalendarService, ui::*};
use chrono::{Datelike, Duration, TimeZone, Utc};
use late_core::{
    db::{Db, DbConfig},
    models::calendar::*,
};
use ratatui::{Terminal, backend::TestBackend};
use uuid::Uuid;
fn event(start: u32, end: u32) -> CalendarEvent {
    let start = Utc.with_ymd_and_hms(2026, 10, 2, start, 0, 0).unwrap();
    let end = Utc.with_ymd_and_hms(2026, 10, 2, end, 0, 0).unwrap();
    CalendarEvent {
        id: Uuid::now_v7(),
        owner_id: None,
        creator_id: Uuid::nil(),
        creation_tier: CreationTier::Admin,
        mod_editable: false,
        title: "日本語 café 🗓 Calendar".into(),
        description: String::new(),
        timing: EventTiming::Timed {
            start,
            end: Some(end),
        },
        creator_timezone: "UTC".into(),
        notice_lead_seconds: None,
        notice_start: start,
        notice_end: end,
        revision: 1,
    }
}
#[test]
fn calendar_overlaps_use_separate_lanes_and_continue_overnight() {
    let events = vec![event(9, 12), event(10, 11), event(11, 13)];
    let date = "2026-10-02".parse().unwrap();
    let pieces = segments(&events, date, chrono_tz::UTC);
    assert_eq!(
        pieces.iter().map(|p| p.lane).collect::<Vec<_>>(),
        vec![0, 1, 1]
    );
    let mut overnight = event(23, 23);
    if let EventTiming::Timed { end, .. } = &mut overnight.timing {
        *end = Some(Utc.with_ymd_and_hms(2026, 10, 3, 2, 0, 0).unwrap());
    }
    let pieces = segments(&[overnight.clone()], date, chrono_tz::UTC);
    assert!(pieces[0].after);
    assert_eq!(pieces[0].end, 1440);
    let pieces = segments(&[overnight], date + Duration::days(1), chrono_tz::UTC);
    assert!(pieces[0].before);
    assert_eq!(pieces[0].start, 0);
}

#[tokio::test]
async fn calendar_go_date_preview_and_controls_fit_compact_terminals() {
    let mut s = CalendarState::new(
        CalendarService::new(Db::new(&DbConfig::default()).unwrap()),
        Uuid::nil(),
    );
    s.selected = "2028-01-31".parse().unwrap();
    s.modal = Some(Modal::Go(Box::new(ratatui_textarea::TextArea::from(vec![
        "+1 month".to_string(),
    ]))));
    for (w, h) in [(140, 45), (80, 24), (48, 16)] {
        s.invalidate_geometry();
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal
            .draw(|frame| draw_modal(frame, frame.area(), &s))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let text: String = buffer.content().iter().map(|c| c.symbol()).collect();
        assert!(text.contains("Go to 2028-02-29"), "{w}x{h}: {text}");
        assert!(text.contains("Offsets from 2028-01-31"), "{text}");
        for action in [Action::Save, Action::Cancel] {
            let hit = s
                .hits
                .borrow()
                .iter()
                .find(|hit| hit.action == action)
                .unwrap()
                .area;
            assert_eq!(
                hit.intersection(ratatui::layout::Rect::new(0, 0, w, h)),
                hit
            );
        }
    }
}

#[test]
fn calendar_repeated_dst_hour_keeps_actual_overlaps_in_separate_lanes() {
    let tz = chrono_tz::America::New_York;
    let date = "2026-11-01".parse::<chrono::NaiveDate>().unwrap();
    let make = |start: &str, end: &str, occurrence| {
        let mut e = event(9, 12);
        e.timing = EventTiming::Timed {
            start: local_instant(start.parse().unwrap(), tz, Some(occurrence)).unwrap(),
            end: Some(local_instant(end.parse().unwrap(), tz, Some(Occurrence::Later)).unwrap()),
        };
        e
    };
    let cross_fold = make(
        "2026-11-01T01:45:00",
        "2026-11-01T01:15:00",
        Occurrence::Earlier,
    );
    let later = make(
        "2026-11-01T01:00:00",
        "2026-11-01T01:20:00",
        Occurrence::Later,
    );
    let label = timing_label(&cross_fold, tz);
    assert!(
        label.contains("01:45 EDT") && label.contains("01:15 EST"),
        "{label}"
    );
    let pieces = segments(&[cross_fold, later], date, tz);
    assert_ne!(pieces[0].lane, pieces[1].lane);
}
#[tokio::test]
async fn calendar_all_views_render_clipped_geometry_and_unicode() {
    let mut s = CalendarState::new(
        CalendarService::new(Db::new(&DbConfig::default()).unwrap()),
        Uuid::nil(),
    );
    s.selected = "2026-10-02".parse().unwrap();
    s.events = (0..8).map(|_| event(9, 12)).collect();
    for (w, h) in [(140, 45), (80, 24), (48, 16), (22, 9)] {
        for view in CalendarView::ALL {
            s.view = view;
            s.reset_scroll();
            let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
            terminal
                .draw(|frame| draw(frame, frame.area(), &s))
                .unwrap();
            let area = ratatui::layout::Rect::new(0, 0, w, h);
            for hit in s.hits.borrow().iter() {
                assert_eq!(hit.area.intersection(area), hit.area, "{view:?} {hit:?}");
            }
            assert!(!s.hits.borrow().is_empty());
        }
    }
}

#[tokio::test]
async fn calendar_compact_month_keeps_crowded_day_count_intact() {
    let mut s = CalendarState::new(
        CalendarService::new(Db::new(&DbConfig::default()).unwrap()),
        Uuid::nil(),
    );
    s.selected = s.today();
    s.events = (0..21)
        .map(|_| {
            let mut event = event(9, 12);
            event.timing = EventTiming::AllDay {
                start: s.selected,
                end_exclusive: s.selected + Duration::days(1),
            };
            event
        })
        .collect();
    let mut terminal = Terminal::new(TestBackend::new(48, 16)).unwrap();
    terminal
        .draw(|frame| draw(frame, frame.area(), &s))
        .unwrap();
    let cell = s
        .hits
        .borrow()
        .iter()
        .find(|h| h.action == Action::Date(s.selected))
        .unwrap()
        .area;
    let buffer = terminal.backend().buffer();
    let text: String = (cell.x..cell.right())
        .map(|x| buffer[(x, cell.y)].symbol())
        .collect();
    assert!(text.contains(&format!("{}+21", s.selected.day())), "{text}");
}

#[tokio::test]
async fn calendar_editor_controls_remain_visible_and_clipped_after_resize() {
    let mut s = CalendarState::new(
        CalendarService::new(Db::new(&DbConfig::default()).unwrap()),
        Uuid::nil(),
    );
    let editor = Editor::from_event(
        &event(9, 12),
        Uuid::nil(),
        CreationTier::Admin,
        chrono_tz::UTC,
    );
    for (w, h) in [(140, 45), (80, 24), (48, 16), (22, 9)] {
        for focus in 0..14 {
            let mut editor = editor.clone();
            editor.focus = focus;
            s.modal = Some(Modal::Editor(Box::new(editor)));
            let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
            terminal
                .draw(|frame| draw_modal(frame, frame.area(), &s))
                .unwrap();
            let area = ratatui::layout::Rect::new(0, 0, w, h);
            for hit in s.hits.borrow().iter() {
                assert_eq!(hit.area.intersection(area), hit.area);
            }
            assert!(s.hits.borrow().iter().any(|h| h.action == Action::Save));
            assert!(s.hits.borrow().iter().any(|h| h.action == Action::Cancel));
        }
    }
}

#[tokio::test]
async fn calendar_narrow_week_reveals_selected_day_and_every_overlap_lane() {
    let mut s = CalendarState::new(
        CalendarService::new(Db::new(&DbConfig::default()).unwrap()),
        Uuid::nil(),
    );
    s.view = CalendarView::Week;
    s.selected = "2026-10-02".parse().unwrap();
    s.events = (0..8).map(|_| event(9, 12)).collect();
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal
        .draw(|frame| draw(frame, frame.area(), &s))
        .unwrap();
    assert!(
        s.hits
            .borrow()
            .iter()
            .any(|h| h.action == Action::Date(s.selected))
    );
    assert!(s.day_scroll.get() > 0);
    let mut ids: std::collections::HashSet<_> = s
        .hits
        .borrow()
        .iter()
        .filter_map(|h| {
            if let Action::Event(id) = h.action {
                Some(id)
            } else {
                None
            }
        })
        .collect();
    s.day_scroll.set(s.max_days.get());
    terminal
        .draw(|frame| draw(frame, frame.area(), &s))
        .unwrap();
    ids.extend(s.hits.borrow().iter().filter_map(|h| {
        if let Action::Event(id) = h.action {
            Some(id)
        } else {
            None
        }
    }));
    assert!(s.events.iter().all(|e| ids.contains(&e.id)));
    let mut compact = Terminal::new(TestBackend::new(48, 16)).unwrap();
    compact.draw(|frame| draw(frame, frame.area(), &s)).unwrap();
    assert!(
        s.hits
            .borrow()
            .iter()
            .any(|h| h.action == Action::Date(s.selected))
    );
}
