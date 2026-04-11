//! App-level dashboard input integration tests against a real ephemeral DB.

mod helpers;

use helpers::{make_app, make_app_with_paired_client, new_test_db, wait_for_render_contains};
use late_core::test_utils::create_test_user;
use late_ssh::session::PairControlMessage;

async fn make_app_harness() -> (late_core::test_utils::TestDb, late_ssh::app::state::App) {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "todo-it").await;
    let app = make_app(test_db.db.clone(), user.id, "todo-flow-it");
    (test_db, app)
}

#[tokio::test]
async fn enter_on_dashboard_shows_url_copied_banner() {
    let (_test_db, mut app) = make_app_harness().await;

    app.handle_input(b"\n");
    wait_for_render_contains(&mut app, "CLI install command copied!").await;
}

#[tokio::test]
async fn r_refresh_on_dashboard_keeps_dashboard_visible() {
    let (_test_db, mut app) = make_app_harness().await;

    wait_for_render_contains(&mut app, " Dashboard ").await;
    app.handle_input(b"r");
    wait_for_render_contains(&mut app, " Dashboard ").await;
}

#[tokio::test]
async fn m_on_dashboard_sends_toggle_to_paired_client() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "paired-browser-it").await;
    let (mut app, mut rx) =
        make_app_with_paired_client(test_db.db.clone(), user.id, "paired-browser-flow-it");

    app.handle_input(b"m");

    assert_eq!(rx.try_recv().unwrap(), PairControlMessage::ToggleMute);
    wait_for_render_contains(&mut app, "Sent mute toggle to paired client").await;
}

#[tokio::test]
async fn plus_and_minus_send_volume_controls_to_paired_client() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "paired-volume-it").await;
    let (mut app, mut rx) =
        make_app_with_paired_client(test_db.db.clone(), user.id, "paired-volume-flow-it");

    app.handle_input(b"+");
    assert_eq!(rx.try_recv().unwrap(), PairControlMessage::VolumeUp);
    wait_for_render_contains(&mut app, "Sent volume up to paired client").await;

    app.handle_input(b"-");
    assert_eq!(rx.try_recv().unwrap(), PairControlMessage::VolumeDown);
    wait_for_render_contains(&mut app, "Sent volume down to paired client").await;
}
