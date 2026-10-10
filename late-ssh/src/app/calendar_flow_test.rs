use crate::{
    app::{
        calendar::{
            input::act,
            navigation::Selection,
            state::{Action, Modal},
        },
        common::primitives::Screen,
        live::pick::LiveSource,
        state::App,
    },
    test_helpers::{make_app, new_test_db, render_plain, wait_for_app},
};
use chrono::{Duration, Utc};
use late_core::models::calendar::{CalendarSource, CalendarStore, EventDraft, EventTiming};
use late_core::test_utils::create_test_user;
use ratatui::layout::Rect;
use uuid::Uuid;

fn event_hit(app: &mut App, id: Uuid) -> Rect {
    render_plain(app);
    app.calendar
        .hits
        .borrow()
        .iter()
        .find(|hit| matches!(hit.action, Action::Event(event) | Action::EventAt(event, _) if event == id))
        .unwrap()
        .area
}

fn mouse_click(app: &mut App, rect: Rect, button: u8) {
    app.handle_input(
        format!(
            "\x1b[<{button};{};{}M\x1b[<{button};{};{}m",
            rect.x + 1,
            rect.y + 1,
            rect.x + 1,
            rect.y + 1,
        )
        .as_bytes(),
    );
}

/// The whole arc of a post from the keyboard: `7`, `n`, a title with the
/// time in it, save, say you are in, and the board shows the count.
#[tokio::test]
async fn posting_to_the_board_and_saying_youre_in() {
    let db = new_test_db().await;
    let user = create_test_user(&db.db, "calendar_poster").await;
    let mut app = make_app(db.db.clone(), user.id, "calendar-post-flow");
    app.show_splash = false;
    app.handle_input(b"7");
    assert_eq!(app.screen, Screen::Calendars);
    wait_for_app(&mut app, "calendar load", |a| !a.calendar.loading).await;
    let rendered = render_plain(&mut app);
    assert!(rendered.contains("Board shown"), "{rendered}");
    assert!(!rendered.contains("Upcoming events"));

    app.handle_input(b"nMovie night tomorrow at 9pm");
    // Account Settings opens over the editor and hands back to it untouched.
    app.handle_input(b"\x0f");
    assert!(app.show_settings);
    app.handle_input(b"\x1b");
    wait_for_app(&mut app, "account settings escape", |a| !a.show_settings).await;
    app.handle_input(b"\t");
    let Some(Modal::Editor(e)) = &app.calendar.modal else {
        panic!("no editor");
    };
    assert_eq!(e.text(0), "Movie night");
    assert_eq!(e.text(3), "21:00");
    assert_eq!(e.source, CalendarSource::Board);
    app.handle_input(b"\x13");
    wait_for_app(&mut app, "board post saved", |a| !a.calendar.pending).await;
    let Some(Modal::Details(saved)) = &app.calendar.modal else {
        panic!("details after save");
    };
    let id = saved.id;
    assert!(saved.is_board());
    assert_eq!(saved.creator_name, "calendar_poster");
    let rendered = render_plain(&mut app);
    assert!(rendered.contains("nobody's in yet"), "{rendered}");

    app.handle_input(b"i");
    wait_for_app(&mut app, "rsvp lands", |a| {
        !a.calendar.pending && a.calendar.going(id)
    })
    .await;
    let rendered = render_plain(&mut app);
    assert!(rendered.contains("you're in"), "{rendered}");
    assert!(rendered.contains("I'm out"));
    app.handle_input(b"i");
    wait_for_app(&mut app, "rsvp taken back", |a| {
        !a.calendar.pending && !a.calendar.going(id)
    })
    .await;

    act(&mut app.calendar, Action::Cancel);
    assert!(app.calendar.modal.is_none());
    wait_for_app(&mut app, "post in the grid", |a| {
        !a.calendar.loading && a.calendar.events.iter().any(|e| e.id == id)
    })
    .await;
    // A right click on it offers the poster's full menu.
    let hit = event_hit(&mut app, id);
    mouse_click(&mut app, hit, 2);
    let menu = app.calendar.context_menu.as_ref().expect("context menu");
    assert_eq!(
        menu.items.len(),
        4,
        "open, in, edit, delete: {:?}",
        menu.items
    );
    app.handle_input(b"\x1b");
    wait_for_app(&mut app, "menu closes", |a| {
        a.calendar.context_menu.is_none()
    })
    .await;
}

/// A moderator sees a stranger's post with Delete and without Edit, and the
/// delete returns to the agenda with the selection moved on.
#[tokio::test]
async fn staff_take_a_post_down_from_its_details() {
    let db = new_test_db().await;
    let poster = create_test_user(&db.db, "calendar_stranger").await;
    let staff = create_test_user(&db.db, "calendar_mod").await;
    db.db
        .get()
        .await
        .unwrap()
        .execute(
            "UPDATE users SET is_moderator=true WHERE id=$1",
            &[&staff.id],
        )
        .await
        .unwrap();
    let mut app = make_app(db.db.clone(), staff.id, "calendar-staff-flow");
    app.show_splash = false;
    app.handle_input(b"7");
    wait_for_app(&mut app, "calendar load", |a| !a.calendar.loading).await;
    let date = app.calendar.selected;
    let post = CalendarStore::new(db.db.clone())
        .save(
            poster.id,
            CalendarSource::Board,
            None,
            &EventDraft {
                title: "Loud party".into(),
                description: String::new(),
                timing: EventTiming::AllDay {
                    start: date,
                    end_exclusive: date.succ_opt().unwrap(),
                },
            },
        )
        .await
        .unwrap()
        .event;
    app.calendar.refresh();
    wait_for_app(&mut app, "post loaded", |a| {
        !a.calendar.loading && a.calendar.events.iter().any(|e| e.id == post.id) && a.calendar.staff
    })
    .await;
    app.calendar.select_date(date);
    app.handle_input(b"\r");
    assert!(matches!(app.calendar.modal, Some(Modal::Agenda)));
    app.handle_input(b"j\r");
    wait_for_app(
        &mut app,
        "details",
        |a| matches!(&a.calendar.modal, Some(Modal::Details(e)) if e.id == post.id),
    )
    .await;
    app.handle_input(b"e");
    assert!(
        matches!(&app.calendar.modal, Some(Modal::Details(_))),
        "staff do not edit a stranger's words"
    );
    assert_eq!(
        app.calendar.error.as_deref(),
        Some("This event is not yours to change")
    );
    app.handle_input(b"\x1b[3~");
    assert!(matches!(app.calendar.modal, Some(Modal::Delete(_))));
    let rendered = render_plain(&mut app);
    assert!(
        rendered.contains("Posted by calendar_stranger"),
        "{rendered}"
    );
    app.handle_input(b"y");
    wait_for_app(&mut app, "post deleted", |a| !a.calendar.pending).await;
    assert!(matches!(app.calendar.modal, Some(Modal::Agenda)));
    assert_ne!(app.calendar.selection, Selection::Event(post.id));
    assert_eq!(app.calendar.selected, date);
}

/// An event an hour out reaches the live strip and the Live panel, and
/// opening it from there lands on the board with its details up.
#[tokio::test]
async fn an_event_about_to_start_opens_from_the_live_strip() {
    let db = new_test_db().await;
    let user = create_test_user(&db.db, "calendar_live").await;
    let mut app = make_app(db.db.clone(), user.id, "calendar-live-flow");
    app.show_splash = false;
    // Fifty-eight minutes out: the strip stamp (an hour before) is two
    // minutes old, inside the five the strip keeps something up.
    let start = Utc::now() + Duration::minutes(58);
    let event = CalendarStore::new(db.db.clone())
        .save(
            user.id,
            CalendarSource::Personal,
            None,
            &EventDraft {
                title: "Standup".into(),
                description: String::new(),
                timing: EventTiming::Timed {
                    start,
                    end: Some(start + Duration::hours(1)),
                },
            },
        )
        .await
        .unwrap()
        .event;
    app.handle_input(b"7");
    wait_for_app(&mut app, "own upcoming loaded", |a| {
        !a.calendar.loading
            && a.calendar
                .personal_upcoming
                .iter()
                .any(|e| e.id == event.id)
    })
    .await;
    app.handle_input(b"1");
    assert_eq!(app.screen, Screen::Dashboard);
    wait_for_app(&mut app, "strip shows the event", |a| {
        a.live.showing() == Some(LiveSource::BoardEvent(event.id))
    })
    .await;
    // The test database has no #lounge, so no card carries the strip; the
    // Live panel on the sidebar lists it as the viewer's own.
    let rendered = render_plain(&mut app);
    assert!(
        rendered.contains("yours") && rendered.contains("in 5"),
        "{rendered}"
    );
    assert!(crate::app::live::input::open_from_key(&mut app));
    assert_eq!(app.screen, Screen::Calendars);
    assert!(matches!(&app.calendar.modal, Some(Modal::Details(e)) if e.id == event.id));
    let rendered = render_plain(&mut app);
    assert!(rendered.contains("Your event"), "{rendered}");
}
