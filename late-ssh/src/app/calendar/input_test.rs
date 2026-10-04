use super::*;
use crate::app::calendar::{state::Hit, svc::CalendarService};
use chrono::{NaiveDate, TimeZone, Utc};
use late_core::{
    db::{Db, DbConfig},
    models::calendar::{CalendarEvent, EventTiming},
};
use ratatui::layout::Rect;
use uuid::Uuid;

fn state() -> CalendarState {
    let mut state = CalendarState::new(
        CalendarService::new(Db::new(&DbConfig::default()).unwrap()),
        Uuid::nil(),
    );
    state.loading = false;
    state.source = CalendarSource::Personal(state.viewer);
    state.selected = NaiveDate::from_ymd_opt(2026, 10, 2).unwrap();
    state
}

fn event() -> CalendarEvent {
    let start = Utc.with_ymd_and_hms(2026, 10, 2, 9, 0, 0).unwrap();
    CalendarEvent {
        id: Uuid::from_u128(1),
        owner_id: Some(Uuid::nil()),
        creator_id: Uuid::nil(),
        creation_tier: CreationTier::User,
        mod_editable: false,
        title: "Selected appointment".into(),
        description: String::new(),
        timing: EventTiming::Timed {
            start,
            end: Some(start + Duration::hours(1)),
        },
        creator_timezone: "UTC".into(),
        notice_lead_seconds: None,
        notice_start: start,
        notice_end: start + Duration::hours(1),
        revision: 1,
    }
}

fn click(x: u16, y: u16, button: MouseButton) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down,
        button: Some(button),
        x,
        y,
        modifiers: Default::default(),
    }
}

#[tokio::test]
async fn calendar_empty_agenda_new_and_cancel_return_to_agenda() {
    let mut s = state();
    s.push_modal(Modal::Agenda);
    handle_modal(&mut s, &ParsedInput::Char('n'));
    assert!(matches!(s.modal, Some(Modal::Editor(_))));
    escape(&mut s);
    assert!(matches!(s.modal, Some(Modal::Agenda)));
    escape(&mut s);
    assert!(s.modal.is_none());
}

#[tokio::test]
async fn calendar_blank_click_breaks_event_double_click_sequence() {
    let mut s = state();
    let event = event();
    s.events.push(event.clone());
    s.hits.borrow_mut().push(Hit {
        area: Rect::new(2, 2, 20, 1),
        action: Action::Event(event.id),
    });
    assert!(handle_mouse(&mut s, &click(3, 3, MouseButton::Left)));
    assert_eq!(s.selection, Selection::Event(event.id));
    assert!(s.last_click.borrow().is_some());
    assert!(!handle_mouse(&mut s, &click(40, 15, MouseButton::Left)));
    assert!(s.last_click.borrow().is_none());
    let generation = s.open_generation;
    assert!(handle_mouse(&mut s, &click(3, 3, MouseButton::Left)));
    assert_eq!(
        s.open_generation,
        generation + 1,
        "selection cancels old opens but must not start one"
    );
    assert!(s.modal.is_none());
}

#[tokio::test]
async fn calendar_context_menu_dismissal_does_not_activate_underlying_new_button() {
    let mut s = state();
    s.open_context_menu(ClickTarget::Date(s.selected), (12, 8));
    s.hits.borrow_mut().push(Hit {
        area: Rect::new(2, 2, 10, 1),
        action: Action::New,
    });
    handle_menu(&mut s, &ParsedInput::Mouse(click(3, 3, MouseButton::Left)));
    assert!(s.context_menu.is_none());
    assert!(s.modal.is_none());
}

#[tokio::test]
async fn calendar_context_menu_escape_keeps_details_open() {
    let mut s = state();
    let event = event();
    s.events.push(event.clone());
    s.select_event(event.id, None, false);
    s.push_modal(Modal::Details(event.clone()));
    s.open_context_menu(ClickTarget::Event(event.id), (12, 8));
    handle_menu(&mut s, &ParsedInput::Byte(0x1b));
    assert!(s.context_menu.is_none());
    assert!(matches!(s.modal, Some(Modal::Details(_))));
    assert_eq!(s.modal_parents.len(), 1);
}

#[tokio::test]
async fn calendar_modal_and_menu_letters_accept_terminal_character_events() {
    let mut s = state();
    let event = event();
    s.events.push(event.clone());
    s.push_modal(Modal::Agenda);
    handle_modal(&mut s, &ParsedInput::Char('j'));
    assert_eq!(s.selection, Selection::Event(event.id));
    handle_modal(&mut s, &ParsedInput::Char('e'));
    assert!(matches!(s.modal, Some(Modal::Editor(_))));
    escape(&mut s);
    assert!(matches!(s.modal, Some(Modal::Agenda)));
    s.open_context_menu(ClickTarget::Event(event.id), (12, 8));
    handle_menu(&mut s, &ParsedInput::Char('j'));
    assert_eq!(s.context_menu.as_ref().unwrap().selected, 1);
    handle_menu(&mut s, &ParsedInput::Byte(b'\r'));
    assert!(s.context_menu.is_none());
    assert!(matches!(s.modal, Some(Modal::Editor(_))));
}

#[tokio::test]
async fn calendar_source_arrows_wrap_and_clear_the_previous_calendar() {
    let mut s = state();
    let shared = Uuid::from_u128(2);
    s.public.push(late_core::models::calendar::PublicCalendar {
        owner_id: shared,
        username: "shared".into(),
    });
    s.source = CalendarSource::Server;
    let date = s.selected;
    s.events.push(event());
    s.selection = Selection::Event(s.events[0].id);
    s.agenda_scroll = 5;

    act(&mut s, Action::CycleSource(-1));
    assert_eq!(s.source, CalendarSource::Personal(shared));
    assert!(s.events.is_empty());
    assert_eq!(s.selection, Selection::Date);
    assert_eq!(s.agenda_scroll, 0);
    assert_eq!(s.selected, date);
    assert!(s.modal.is_none());

    act(&mut s, Action::CycleSource(1));
    assert_eq!(s.source, CalendarSource::Server);
    act(&mut s, Action::CycleSource(1));
    assert_eq!(s.source, CalendarSource::Personal(s.viewer));
    act(&mut s, Action::Source);
    assert!(matches!(s.modal, Some(Modal::Source(1))));
}

#[tokio::test]
async fn calendar_view_arrows_wrap_and_preserve_the_selected_date() {
    let mut s = state();
    let date = s.selected;
    act(&mut s, Action::CycleView(-1));
    assert_eq!(s.view, CalendarView::List);
    act(&mut s, Action::CycleView(1));
    assert_eq!(s.view, CalendarView::Month);
    act(&mut s, Action::CycleView(1));
    assert_eq!(s.view, CalendarView::Week);
    assert!(matches!(s.selection, Selection::Slot(_)));
    assert_eq!(s.selected, date);
    assert!(s.modal.is_none());
    act(&mut s, Action::View);
    assert!(matches!(s.modal, Some(Modal::View(1))));
}
