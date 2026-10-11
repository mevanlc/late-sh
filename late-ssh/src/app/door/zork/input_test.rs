use super::input::launch;
use super::state::Mode;
use crate::app::common::primitives::Screen;
use crate::test_helpers::make_app;

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
