use crate::{
    app::{
        calendar::{
            input::act,
            state::{Action, Modal},
        },
        common::primitives::Screen,
    },
    test_helpers::{make_app, new_test_db, render_plain, wait_for_app},
};
use late_core::models::calendar::{
    CalendarPreferences, CalendarSource, CalendarStore, CalendarView,
};
use late_core::test_utils::create_test_user;
#[tokio::test]
async fn calendar_go_date_human_input_preview_validation_and_mouse_submit() {
    let db = new_test_db().await;
    let user = create_test_user(&db.db, "calendar_human_dates").await;
    let mut app = make_app(db.db.clone(), user.id, "calendar-human-dates");
    app.show_splash = false;
    app.handle_input(b"7");
    wait_for_app(&mut app, "calendar load", |a| !a.calendar.loading).await;
    app.calendar.selected = "2028-08-31".parse().unwrap();
    app.handle_input(b"g2 months ago");
    assert!(render_plain(&mut app).contains("Go to 2028-06-30"));
    app.handle_input(b"\r");
    assert!(app.calendar.modal.is_none());
    assert_eq!(app.calendar.selected.to_string(), "2028-06-30");
    app.handle_input(b"g02/03/2026\r");
    assert!(app.calendar.error.as_deref().unwrap().contains("Ambiguous"));
    let Some(Modal::Go(input)) = &app.calendar.modal else {
        panic!()
    };
    assert_eq!(input.lines(), &["02/03/2026"]);
    assert_eq!(app.calendar.selected.to_string(), "2028-06-30");
    app.handle_input(b"\x15Oct 2nd, 2028");
    assert!(app.calendar.error.is_none());
    app.resize(48, 16).unwrap();
    let rendered = render_plain(&mut app);
    assert!(rendered.contains("Go to 2028-10-02"), "{rendered}");
    let go = app
        .calendar
        .hits
        .borrow()
        .iter()
        .find(|h| h.action == Action::Save)
        .unwrap()
        .area;
    app.handle_input(format!("\x1b[<0;{};{}M", go.x + 1, go.y + 1).as_bytes());
    assert!(app.calendar.modal.is_none());
    assert_eq!(app.calendar.selected.to_string(), "2028-10-02");
    app.handle_input(b"g+2w\r");
    assert_eq!(app.calendar.selected.to_string(), "2028-10-16");
    app.handle_input(b"gtoday\r");
    assert_eq!(app.calendar.selected, app.calendar.today());
    app.handle_input(b"g-2 weeks\x1b");
    wait_for_app(&mut app, "go-to-date escape", |a| {
        a.calendar.modal.is_none()
    })
    .await;
    assert_eq!(app.calendar.selected, app.calendar.today());

    app.handle_input(b"s\x1b[B\rnTrip\t\t\x152 Oct 2028\t");
    let Some(Modal::Editor(e)) = &app.calendar.modal else {
        panic!()
    };
    assert_eq!(e.text(2), "2028-10-02");
    assert!(e.ever_assigned);
    app.handle_input(b"\x13");
    wait_for_app(&mut app, "human calendar date save", |a| {
        !a.calendar.pending
    })
    .await;
    let Some(Modal::Details(saved)) = &app.calendar.modal else {
        panic!()
    };
    assert_eq!(
        saved.timing.dates(app.calendar.tz).0.to_string(),
        "2028-10-02"
    );
}

#[tokio::test]
async fn calendar_app_navigation_modal_priority_and_editor_text() {
    let db = new_test_db().await;
    let user = create_test_user(&db.db, "calendar_ui").await;
    let mut app = make_app(db.db.clone(), user.id, "calendar-ui-flow");
    app.show_splash = false;
    app.handle_input(b"7");
    assert_eq!(app.screen, Screen::Calendars);
    wait_for_app(&mut app, "calendar load", |a| !a.calendar.loading).await;
    assert!(render_plain(&mut app).contains("Upcoming events"));
    app.handle_input(b"s");
    assert!(matches!(app.calendar.modal, Some(Modal::Source(_))));
    app.handle_input(b"\x1b[B\r");
    assert_eq!(app.calendar.source, CalendarSource::Personal(user.id));
    app.handle_input(b"n");
    assert!(matches!(app.calendar.modal, Some(Modal::Editor(_))));
    app.handle_input(b"Unicode \xe6\x97\xa5 2028-02-29");
    app.handle_input(b"\x0f");
    assert!(app.show_settings);
    app.handle_input(b"7sv");
    let Some(Modal::Editor(e)) = &app.calendar.modal else {
        panic!()
    };
    assert_eq!(e.text(0), "Unicode 日 2028-02-29");
    app.handle_input(b"\x1b");
    wait_for_app(&mut app, "account settings escape", |a| !a.show_settings).await;
    app.handle_input(b"\t");
    let Some(Modal::Editor(e)) = &app.calendar.modal else {
        panic!()
    };
    assert_eq!(e.text(0), "Unicode 日");
    assert_eq!(e.text(2), "2028-02-29");
    app.handle_input(b"7svt");
    let Some(Modal::Editor(e)) = &app.calendar.modal else {
        panic!()
    };
    assert_eq!(e.text(1), "7svt");
    assert_eq!(app.screen, Screen::Calendars);
    act(&mut app.calendar, Action::Cancel);
    let Some(Modal::Editor(e)) = &app.calendar.modal else {
        panic!()
    };
    assert!(e.discard_prompt);
    act(&mut app.calendar, Action::Keep);
    app.handle_input(b"\x13");
    wait_for_app(&mut app, "calendar save", |a| !a.calendar.pending).await;
    assert!(matches!(app.calendar.modal, Some(Modal::Details(_))));
    act(&mut app.calendar, Action::Cancel);
    app.handle_input(b"c");
    assert!(matches!(app.calendar.modal, Some(Modal::Settings { .. })));
    act(&mut app.calendar, Action::Cancel);
    app.handle_input(b"\x0f");
    assert!(app.show_settings);
    app.show_settings = false;
    app.handle_input(b"\t");
    assert_eq!(app.screen, Screen::Clubhouse);
    app.handle_input(b"\x1b[Z");
    assert_eq!(app.screen, Screen::Calendars);
    app.calendar.view = CalendarView::Week;
    render_plain(&mut app);
    assert!(!app.calendar.hits.borrow().is_empty());
    app.resize(48, 16).unwrap();
    assert!(app.calendar.hits.borrow().is_empty());
    render_plain(&mut app);
    let grid = app
        .calendar
        .panes
        .borrow()
        .iter()
        .find(|p| p.pane == crate::app::calendar::state::Pane::Grid)
        .unwrap()
        .area;
    let before = app.calendar.hour_scroll;
    app.handle_input(format!("\x1b[<65;{};{}M", grid.x + 1, grid.y + 1).as_bytes());
    assert_eq!(app.calendar.hour_scroll, before + 3);
    app.handle_input(b"n");
    app.handle_input(b"Mouse Save 2028-03-01");
    render_plain(&mut app);
    let save = app
        .calendar
        .hits
        .borrow()
        .iter()
        .find(|h| h.action == Action::Save)
        .unwrap()
        .area;
    app.handle_input(format!("\x1b[<0;{};{}M", save.x + 1, save.y + 1).as_bytes());
    wait_for_app(&mut app, "mouse calendar save", |a| !a.calendar.pending).await;
    let Some(Modal::Details(saved)) = &app.calendar.modal else {
        panic!()
    };
    assert_eq!(saved.title, "Mouse Save");
    assert_eq!(
        saved.timing.dates(app.calendar.tz).0.to_string(),
        "2028-03-01"
    );
    app.handle_input(b"\x1b[3~");
    assert!(matches!(app.calendar.modal, Some(Modal::Delete(_))));
    app.handle_input(b"y");
    wait_for_app(&mut app, "calendar confirmed delete", |a| {
        !a.calendar.pending
    })
    .await;
    assert!(app.calendar.modal.is_none());
}

#[tokio::test]
async fn calendar_public_profile_link_opens_read_only_source_and_clears_on_resize() {
    let db = new_test_db().await;
    let owner = create_test_user(&db.db, "calendar_profile_owner").await;
    let viewer = create_test_user(&db.db, "calendar_profile_viewer").await;
    CalendarStore::new(db.db.clone())
        .save_preferences(
            owner.id,
            &CalendarPreferences {
                public: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let mut app = make_app(db.db.clone(), viewer.id, "calendar-profile-link");
    app.show_splash = false;
    wait_for_app(&mut app, "public calendar sources", |a| !a.calendar.loading).await;
    app.set_screen(Screen::Profiles);
    app.profile_modal_state.open(owner.id, &owner.username);
    app.show_profile_modal = true;
    assert!(render_plain(&mut app).contains("c Open calendar"));
    assert!(app.profile_modal_state.calendar_link.get().width > 0);
    app.resize(80, 24).unwrap();
    assert_eq!(app.profile_modal_state.calendar_link.get().width, 0);
    app.handle_input(b"c");
    assert_eq!(app.screen, Screen::Calendars);
    assert_eq!(app.calendar.source, CalendarSource::Personal(owner.id));
    assert!(!app.show_profile_modal);
    app.handle_input(b"n");
    assert!(!matches!(app.calendar.modal, Some(Modal::Editor(_))));
    assert_eq!(
        app.calendar.error.as_deref(),
        Some("This calendar is read-only")
    );
}
