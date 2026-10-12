use super::input::launch;
use super::state::Mode;
use crate::app::common::primitives::Screen;
use crate::test_helpers::{make_app, wait_for_app};

/// A disabled client is a banner on the hub, never a menu whose actions do
/// nothing; an enabled one opens the edition menu.
#[tokio::test]
async fn the_games_card_respects_the_enabled_gate() {
    let db = late_core::db::Db::new(&late_core::db::DbConfig {
        port: 1,
        ..Default::default()
    })
    .unwrap();
    let mut app = make_app(db, uuid::Uuid::from_u128(1), "zork-gate");
    app.set_screen(Screen::Games);

    launch(&mut app);
    assert_eq!(app.screen, Screen::Games);
    assert!(app.zork_state.is_none());
    assert_eq!(
        app.banner.as_ref().map(|banner| banner.message.as_str()),
        Some("Zork Trilogy is currently unavailable.")
    );

    app.zork_enabled = true;
    launch(&mut app);
    assert_eq!(app.screen, Screen::Zork);
    assert_eq!(
        app.zork_state.as_ref().map(|state| state.mode()),
        Some(Mode::Editions)
    );
}

/// A state with no live game does not outlive the visit, like every other
/// door: leaving the menus drops it, so the next visit starts fresh instead
/// of repeating the last visit's closing message, and a detached game that
/// ends off-screen is reaped so the hub card and the backtick cycle stop
/// advertising it.
#[tokio::test]
async fn a_state_without_a_live_game_does_not_outlive_the_visit() {
    let db = late_core::db::Db::new(&late_core::db::DbConfig {
        port: 1,
        ..Default::default()
    })
    .unwrap();
    let mut app = make_app(db, uuid::Uuid::from_u128(1), "zork-leave");
    app.zork_enabled = true;
    app.set_screen(Screen::Games);

    launch(&mut app);
    assert_eq!(app.screen, Screen::Zork);
    assert!(app.zork_state.is_some());
    app.set_screen(Screen::Games);
    assert!(
        app.zork_state.is_none(),
        "idle menus must not survive leaving"
    );

    launch(&mut app);
    app.zork_state
        .as_mut()
        .expect("zork state")
        .force_running_for_test();
    app.handle_input(b"`");
    assert_ne!(app.screen, Screen::Zork);
    assert!(
        app.zork_state
            .as_ref()
            .is_some_and(|state| state.is_running()),
        "a detached game stays alive"
    );
    // The fabricated proxy points at a dead address: the game ends off-screen.
    wait_for_app(&mut app, "ended game reaped", |app| {
        app.zork_state.is_none()
    })
    .await;
}
