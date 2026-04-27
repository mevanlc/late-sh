//! App input integration tests against a real ephemeral DB.

mod helpers;

use helpers::{
    assert_render_not_contains_for, chat_compose_app, make_app, make_app_with_chat_service,
    make_app_with_permissions, make_app_with_runtime_permissions, new_test_db, render_plain,
    wait_for_render_contains, wait_until,
};
use late_core::models::{
    chat_message::{ChatMessage, ChatMessageParams},
    chat_message_reaction::ChatMessageReaction,
    chat_room::ChatRoom,
    chat_room_member::ChatRoomMember,
    server_ban::ServerBan,
    user::User,
};
use late_core::test_utils::create_test_user;
use late_ssh::authz::Permissions;
use late_ssh::session::{SessionMessage, SessionRegistration, SessionRegistry};
use rstest::rstest;
use tokio::time::Duration;
use uuid::Uuid;

#[tokio::test]
async fn dashboard_chat_compose_blocks_quit_shortcut() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "popup-it").await;
    let client = test_db.db.get().await.expect("db client");
    let general = ChatRoom::ensure_general(&client)
        .await
        .expect("ensure general room");
    ChatRoomMember::join(&client, general.id, user.id)
        .await
        .expect("join general room");
    let mut app = make_app(test_db.db.clone(), user.id, "popup-flow-it");

    // Hop through the chat screen first so the async room snapshot has
    // definitely landed: `> general` only renders once `drain_snapshot`
    // populates `general_room_id`, which the dashboard `i` handler needs.
    app.handle_input(b"2");
    wait_for_render_contains(&mut app, "> general").await;
    app.handle_input(b"1");
    wait_for_render_contains(&mut app, " Dashboard ").await;

    app.handle_input(b"i");
    wait_for_render_contains(
        &mut app,
        "Compose (Enter send, Alt+S stay, Alt+Enter newline, Esc cancel)",
    )
    .await;

    app.handle_input(b"q$$$");
    wait_for_render_contains(&mut app, "$$$").await;
    wait_for_render_contains(&mut app, " Dashboard ").await;
}

#[tokio::test]
async fn q_opens_quit_confirm_and_escape_dismisses_it() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "quit-confirm-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "quit-confirm-flow-it");

    app.handle_input(b"q");
    wait_for_render_contains(&mut app, " Quit? ").await;
    wait_for_render_contains(&mut app, "Clicked by mistake, right?").await;
    wait_for_render_contains(&mut app, "bye, I'll be back").await;
    wait_for_render_contains(&mut app, "yeah, my bad, stay").await;

    app.handle_input(b"\x1b");
    tokio::time::sleep(Duration::from_millis(60)).await;
    let frame = render_plain(&mut app);
    assert!(
        !frame.contains("Clicked by mistake, right?"),
        "expected quit confirm to dismiss after Esc; frame={frame:?}"
    );
}

#[tokio::test]
async fn ctrl_c_does_not_quit_the_app() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "ctrl-c-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "ctrl-c-flow-it");

    app.handle_input(b"\x03");
    tokio::time::sleep(Duration::from_millis(60)).await;

    assert!(
        app.is_running(),
        "expected Ctrl+C to no longer quit the app"
    );
    let frame = render_plain(&mut app);
    assert!(
        frame.contains(" Dashboard "),
        "expected app to remain on the dashboard after Ctrl+C; frame={frame:?}"
    );
    assert!(
        !frame.contains(" Quit? "),
        "expected Ctrl+C to stay inert rather than opening quit confirm; frame={frame:?}"
    );
}

#[tokio::test]
async fn screen_number_keys_switch_between_dashboard_chat_games_rooms_and_artboard() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "screen-it").await;
    let client = test_db.db.get().await.expect("db client");
    let general = ChatRoom::ensure_general(&client)
        .await
        .expect("ensure general room");
    ChatRoomMember::join(&client, general.id, user.id)
        .await
        .expect("join general room");
    let mut app = make_app(test_db.db.clone(), user.id, "screen-flow-it");

    app.handle_input(b"2");
    wait_for_render_contains(&mut app, " Chat ").await;

    app.handle_input(b"3");
    wait_for_render_contains(&mut app, " The Arcade ").await;

    app.handle_input(b"4");
    wait_for_render_contains(&mut app, " Rooms ").await;

    app.handle_input(b"5");
    wait_for_render_contains(&mut app, "Mode       view").await;

    app.handle_input(b"1");
    wait_for_render_contains(&mut app, " Dashboard ").await;
}

#[tokio::test]
async fn shift_tab_cycles_screens_backwards() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "screen-backtab-it").await;
    let client = test_db.db.get().await.expect("db client");
    let general = ChatRoom::ensure_general(&client)
        .await
        .expect("ensure general room");
    ChatRoomMember::join(&client, general.id, user.id)
        .await
        .expect("join general room");
    let mut app = make_app(test_db.db.clone(), user.id, "screen-backtab-flow-it");

    app.handle_input(b"\x1b[Z");
    wait_for_render_contains(&mut app, "Mode       view").await;

    app.handle_input(b"\x1b[Z");
    wait_for_render_contains(&mut app, " Rooms ").await;

    app.handle_input(b"\x1b[Z");
    wait_for_render_contains(&mut app, " The Arcade ").await;

    app.handle_input(b"\x1b[Z");
    wait_for_render_contains(&mut app, " Chat ").await;

    app.handle_input(b"\x1b[Z");
    wait_for_render_contains(&mut app, " Dashboard ").await;
}

#[tokio::test]
async fn zero_does_not_open_control_center_for_regular_users() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "screen-zero-regular").await;
    let mut app = make_app(test_db.db.clone(), user.id, "screen-zero-regular-flow");

    let frame = render_plain(&mut app);
    assert!(
        frame.contains(" 1 2 3 4 "),
        "expected standard switcher entries for regular user; frame={frame:?}"
    );
    assert!(
        !frame.contains(" 0 1 2 3 4 "),
        "expected screen 0 switcher entry to stay hidden from regular user; frame={frame:?}"
    );

    app.handle_input(b"0");
    tokio::time::sleep(Duration::from_millis(60)).await;

    let frame = render_plain(&mut app);
    assert!(
        frame.contains(" Dashboard "),
        "expected regular user to remain on dashboard; frame={frame:?}"
    );
    assert!(
        !frame.contains("Staff Control Center"),
        "expected control center to stay hidden from regular users; frame={frame:?}"
    );
}

#[tokio::test]
async fn staff_user_can_open_control_center_and_switch_tabs() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "screen-zero-staff").await;
    let client = test_db.db.get().await.expect("db client");
    ChatRoom::ensure_general(&client)
        .await
        .expect("ensure general room");
    ChatRoom::get_or_create_public_room(&client, "ops")
        .await
        .expect("create ops room");
    client
        .execute(
            "UPDATE users SET is_moderator = true WHERE id = $1",
            &[&user.id],
        )
        .await
        .expect("promote moderator");
    let mut app = make_app_with_permissions(
        test_db.db.clone(),
        user.id,
        "screen-zero-staff-flow",
        Permissions::new(false, true),
    );

    let frame = render_plain(&mut app);
    assert!(
        frame.contains(" 0 1 2 3 4 "),
        "expected staff switcher to include screen 0 entry; frame={frame:?}"
    );

    app.handle_input(b"0");
    wait_for_render_contains(&mut app, "Staff Control Center").await;
    wait_for_render_contains(&mut app, "Tab focus tabs · j/k or ↑/↓ move").await;
    wait_for_render_contains(&mut app, "> @screen-zero-staff [mod]").await;

    app.handle_input(b"l");
    wait_for_render_contains(&mut app, " Room Directory ").await;
    wait_for_render_contains(&mut app, "#general").await;
    wait_for_render_contains(&mut app, "#ops").await;
    wait_for_render_contains(&mut app, " Selected Room ").await;

    app.handle_input(b"\x1b[B");
    wait_for_render_contains(&mut app, "> #ops").await;

    app.handle_input(b"\t");
    wait_for_render_contains(&mut app, "Tab focus rooms · h/l or ←/→ switch tabs").await;
    wait_for_render_contains(&mut app, "Staff Control Center").await;

    app.handle_input(b"\x1b[D");
    wait_for_render_contains(&mut app, " User Directory ").await;
    wait_for_render_contains(&mut app, "Selected User").await;

    app.handle_input(b"\t");
    wait_for_render_contains(&mut app, "Tab focus tabs · j/k or ↑/↓ move").await;
}

#[tokio::test]
async fn admin_can_grant_moderator_from_users_tab() {
    let test_db = new_test_db().await;
    let actor = create_test_user(&test_db.db, "cc-grant-mod-actor").await;
    let target = create_test_user(&test_db.db, "cc-grant-mod-target").await;
    let client = test_db.db.get().await.expect("db client");
    client
        .execute(
            "UPDATE users SET is_admin = true WHERE id = $1",
            &[&actor.id],
        )
        .await
        .expect("promote admin");

    let mut app = make_app_with_permissions(
        test_db.db.clone(),
        actor.id,
        "cc-grant-mod-flow",
        Permissions::new(true, false),
    );

    app.handle_input(b"0");
    wait_for_render_contains(&mut app, "Staff Control Center").await;
    wait_for_render_contains(&mut app, "@cc-grant-mod-target").await;

    app.handle_input(b"j");
    wait_for_render_contains(&mut app, "> @cc-grant-mod-target").await;

    app.handle_input(b"m");
    wait_for_render_contains(&mut app, " Grant Moderator ").await;
    wait_for_render_contains(
        &mut app,
        "Type @cc-grant-mod-target to confirm grant moderator",
    )
    .await;

    app.handle_input(b"@cc-grant-mod-target\r");
    wait_for_render_contains(&mut app, "Granting moderator to @cc-grant-mod-target...").await;
    wait_for_render_contains(&mut app, "Granted moderator to @cc-grant-mod-target").await;

    wait_until(
        || async {
            let row = client
                .query_one(
                    "SELECT is_moderator FROM users WHERE id = $1",
                    &[&target.id],
                )
                .await
                .expect("lookup target user");
            row.get::<_, bool>(0)
        },
        "target user is_moderator to flip true",
    )
    .await;

    let audit_row = client
        .query_opt(
            "SELECT action FROM moderation_audit_log
             WHERE actor_user_id = $1 AND target_id = $2
             ORDER BY created DESC LIMIT 1",
            &[&actor.id, &target.id],
        )
        .await
        .expect("audit lookup");
    let audit_row = audit_row.expect("audit row should exist");
    assert_eq!(audit_row.get::<_, String>(0), "grant_moderator");
}

#[tokio::test]
async fn audit_tab_lists_recent_actions_and_shows_detail() {
    let test_db = new_test_db().await;
    let actor = create_test_user(&test_db.db, "cc-audit-actor").await;
    let _target = create_test_user(&test_db.db, "cc-audit-target").await;
    let client = test_db.db.get().await.expect("db client");
    client
        .execute(
            "UPDATE users SET is_admin = true WHERE id = $1",
            &[&actor.id],
        )
        .await
        .expect("promote actor admin");

    let mut app = make_app_with_permissions(
        test_db.db.clone(),
        actor.id,
        "cc-audit-flow",
        Permissions::new(true, false),
    );

    app.handle_input(b"0");
    wait_for_render_contains(&mut app, "Staff Control Center").await;
    wait_for_render_contains(&mut app, "@cc-audit-target").await;

    app.handle_input(b"j");
    wait_for_render_contains(&mut app, "> @cc-audit-target").await;
    app.handle_input(b"m");
    wait_for_render_contains(&mut app, " Grant Moderator ").await;
    app.handle_input(b"@cc-audit-target\r");
    wait_for_render_contains(&mut app, "Granted moderator to @cc-audit-target").await;

    app.handle_input(b"l");
    app.handle_input(b"l");
    app.handle_input(b"l");
    wait_for_render_contains(&mut app, " Entries ").await;
    wait_for_render_contains(&mut app, " Entry detail ").await;
    wait_for_render_contains(&mut app, "grant_moderator").await;
    wait_for_render_contains(&mut app, "@cc-audit-target by @cc-audit-actor").await;
    wait_for_render_contains(&mut app, "action      : grant_moderator").await;
    wait_for_render_contains(&mut app, "actor       : @cc-audit-actor").await;
    wait_for_render_contains(&mut app, "target      : @cc-audit-target").await;
    wait_for_render_contains(&mut app, "target_username: cc-audit-target").await;
}

#[tokio::test]
async fn admin_can_grant_admin_from_staff_tab() {
    let test_db = new_test_db().await;
    let actor = create_test_user(&test_db.db, "cc-grant-admin-actor").await;
    let target = create_test_user(&test_db.db, "cc-grant-admin-target").await;
    let client = test_db.db.get().await.expect("db client");
    client
        .execute(
            "UPDATE users SET is_admin = true WHERE id = $1",
            &[&actor.id],
        )
        .await
        .expect("promote actor admin");
    client
        .execute(
            "UPDATE users SET is_moderator = true WHERE id = $1",
            &[&target.id],
        )
        .await
        .expect("promote target moderator");

    let mut app = make_app_with_permissions(
        test_db.db.clone(),
        actor.id,
        "cc-grant-admin-flow",
        Permissions::new(true, false),
    );

    app.handle_input(b"0");
    wait_for_render_contains(&mut app, "Staff Control Center").await;

    app.handle_input(b"l");
    app.handle_input(b"l");
    wait_for_render_contains(&mut app, " Selected Staffer ").await;
    wait_for_render_contains(&mut app, "@cc-grant-admin-target").await;

    app.handle_input(b"j");
    wait_for_render_contains(&mut app, "> @cc-grant-admin-target m").await;

    app.handle_input(b"g");
    wait_for_render_contains(&mut app, " Grant Admin ").await;
    wait_for_render_contains(
        &mut app,
        "Type @cc-grant-admin-target to confirm grant admin",
    )
    .await;

    app.handle_input(b"@cc-grant-admin-target\r");
    wait_for_render_contains(&mut app, "Granting admin to @cc-grant-admin-target...").await;
    wait_for_render_contains(&mut app, "Granted admin to @cc-grant-admin-target").await;

    wait_until(
        || async {
            let row = client
                .query_one(
                    "SELECT is_admin, is_moderator FROM users WHERE id = $1",
                    &[&target.id],
                )
                .await
                .expect("lookup target");
            let is_admin: bool = row.get(0);
            let is_moderator: bool = row.get(1);
            is_admin && !is_moderator
        },
        "target user becomes admin (and not moderator)",
    )
    .await;

    let audit_action = client
        .query_one(
            "SELECT action FROM moderation_audit_log
             WHERE actor_user_id = $1 AND target_id = $2
             ORDER BY created DESC LIMIT 1",
            &[&actor.id, &target.id],
        )
        .await
        .expect("audit row");
    assert_eq!(audit_action.get::<_, String>(0), "grant_admin");
}

#[tokio::test]
async fn admin_can_revoke_moderator_from_staff_tab() {
    let test_db = new_test_db().await;
    let actor = create_test_user(&test_db.db, "cc-revoke-mod-actor").await;
    let target = create_test_user(&test_db.db, "cc-revoke-mod-target").await;
    let client = test_db.db.get().await.expect("db client");
    client
        .execute(
            "UPDATE users SET is_admin = true WHERE id = $1",
            &[&actor.id],
        )
        .await
        .expect("promote actor admin");
    client
        .execute(
            "UPDATE users SET is_moderator = true WHERE id = $1",
            &[&target.id],
        )
        .await
        .expect("promote target moderator");

    let mut app = make_app_with_permissions(
        test_db.db.clone(),
        actor.id,
        "cc-revoke-mod-flow",
        Permissions::new(true, false),
    );

    app.handle_input(b"0");
    wait_for_render_contains(&mut app, "Staff Control Center").await;

    app.handle_input(b"l");
    app.handle_input(b"l");
    wait_for_render_contains(&mut app, " Selected Staffer ").await;
    wait_for_render_contains(&mut app, "@cc-revoke-mod-target").await;

    app.handle_input(b"j");
    wait_for_render_contains(&mut app, "> @cc-revoke-mod-target m").await;

    app.handle_input(b"r");
    wait_for_render_contains(&mut app, " Revoke Moderator ").await;
    wait_for_render_contains(
        &mut app,
        "Type @cc-revoke-mod-target to confirm revoke moderator",
    )
    .await;

    app.handle_input(b"@cc-revoke-mod-target\r");
    wait_for_render_contains(&mut app, "Revoking moderator from @cc-revoke-mod-target...").await;
    wait_for_render_contains(&mut app, "Revoked moderator from @cc-revoke-mod-target").await;

    wait_until(
        || async {
            let row = client
                .query_one(
                    "SELECT is_admin, is_moderator FROM users WHERE id = $1",
                    &[&target.id],
                )
                .await
                .expect("lookup target");
            let is_admin: bool = row.get(0);
            let is_moderator: bool = row.get(1);
            !is_admin && !is_moderator
        },
        "target user becomes regular",
    )
    .await;

    let audit_action = client
        .query_one(
            "SELECT action FROM moderation_audit_log
             WHERE actor_user_id = $1 AND target_id = $2
             ORDER BY created DESC LIMIT 1",
            &[&actor.id, &target.id],
        )
        .await
        .expect("audit row");
    assert_eq!(audit_action.get::<_, String>(0), "revoke_moderator");
}

#[tokio::test]
async fn staff_tab_lists_moderators_and_admins() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "cc-staff-tab-mod").await;
    let admin = create_test_user(&test_db.db, "cc-staff-tab-admin").await;
    let regular = create_test_user(&test_db.db, "cc-staff-tab-regular").await;
    let client = test_db.db.get().await.expect("db client");
    ChatRoom::ensure_general(&client)
        .await
        .expect("ensure general room");
    client
        .execute(
            "UPDATE users SET is_moderator = true WHERE id = $1",
            &[&user.id],
        )
        .await
        .expect("promote moderator");
    client
        .execute(
            "UPDATE users SET is_admin = true WHERE id = $1",
            &[&admin.id],
        )
        .await
        .expect("promote admin");
    let mut app = make_app_with_permissions(
        test_db.db.clone(),
        user.id,
        "cc-staff-tab-flow",
        Permissions::new(false, true),
    );

    app.handle_input(b"0");
    wait_for_render_contains(&mut app, "Staff Control Center").await;

    app.handle_input(b"l");
    app.handle_input(b"l");
    wait_for_render_contains(&mut app, " Staff ").await;
    wait_for_render_contains(&mut app, "Tab focus tabs · j/k or ↑/↓ move").await;
    wait_for_render_contains(&mut app, "@cc-staff-tab-mod").await;
    wait_for_render_contains(&mut app, "@cc-staff-tab-admin").await;
    wait_for_render_contains(&mut app, " Selected Staffer ").await;

    let frame = render_plain(&mut app);
    assert!(
        !frame.contains("@cc-staff-tab-regular"),
        "Staff tab should exclude non-staff users; frame={frame:?}"
    );

    app.handle_input(b"\x1b[B");
    wait_for_render_contains(&mut app, "> @cc-staff-tab-mod").await;

    let _ = (admin, regular);
}

#[tokio::test]
async fn moderator_can_kick_user_from_control_center_room() {
    let test_db = new_test_db().await;
    let moderator = create_test_user(&test_db.db, "cc-room-mod").await;
    let target = create_test_user(&test_db.db, "cc-room-target").await;
    let client = test_db.db.get().await.expect("db client");
    let general = ChatRoom::ensure_general(&client)
        .await
        .expect("ensure general room");
    let room = ChatRoom::create_private_room(&client, "cc-side")
        .await
        .expect("create room");
    ChatRoomMember::join(&client, general.id, moderator.id)
        .await
        .expect("join moderator to general");
    ChatRoomMember::join(&client, room.id, moderator.id)
        .await
        .expect("join moderator to room");
    ChatRoomMember::join(&client, room.id, target.id)
        .await
        .expect("join target to room");
    client
        .execute(
            "UPDATE users SET is_moderator = true WHERE id = $1",
            &[&moderator.id],
        )
        .await
        .expect("promote moderator");

    let mut app = make_app_with_permissions(
        test_db.db.clone(),
        moderator.id,
        "cc-room-mod-flow",
        Permissions::new(false, true),
    );

    app.handle_input(b"0");
    wait_for_render_contains(&mut app, "Staff Control Center").await;
    app.handle_input(b"l");
    wait_for_render_contains(&mut app, " Room Directory ").await;
    wait_for_render_contains(&mut app, "> #general").await;

    app.handle_input(b"j");
    wait_for_render_contains(&mut app, "> #cc-side").await;

    app.handle_input(b"x");
    wait_for_render_contains(&mut app, " Moderate User ").await;
    wait_for_render_contains(&mut app, "Kick target").await;

    app.handle_input(b"@cc-room-target\r");
    wait_for_render_contains(&mut app, "Kicking @cc-room-target in #cc-side...").await;
    wait_for_render_contains(&mut app, "Kicked @cc-room-target in #cc-side").await;

    wait_until(
        || async {
            !ChatRoomMember::is_member(&client, room.id, target.id)
                .await
                .expect("load target membership")
        },
        "target to be removed from control center selected room",
    )
    .await;
}

#[tokio::test]
async fn admin_can_rename_room_from_control_center() {
    let test_db = new_test_db().await;
    let admin = create_test_user(&test_db.db, "cc-room-admin").await;
    let client = test_db.db.get().await.expect("db client");
    let general = ChatRoom::ensure_general(&client)
        .await
        .expect("ensure general room");
    let room = ChatRoom::create_private_room(&client, "cc-admin-side")
        .await
        .expect("create room");
    ChatRoomMember::join(&client, general.id, admin.id)
        .await
        .expect("join admin to general");
    ChatRoomMember::join(&client, room.id, admin.id)
        .await
        .expect("join admin to room");
    client
        .execute(
            "UPDATE users SET is_admin = true WHERE id = $1",
            &[&admin.id],
        )
        .await
        .expect("promote admin");

    let mut app = make_app_with_permissions(
        test_db.db.clone(),
        admin.id,
        "cc-room-admin-flow",
        Permissions::new(true, false),
    );

    app.handle_input(b"0");
    wait_for_render_contains(&mut app, "Staff Control Center").await;
    app.handle_input(b"l");
    wait_for_render_contains(&mut app, " Room Directory ").await;
    wait_for_render_contains(&mut app, "> #general").await;

    app.handle_input(b"j");
    wait_for_render_contains(&mut app, "> #cc-admin-side").await;

    app.handle_input(b"r");
    wait_for_render_contains(&mut app, " Admin Action ").await;
    wait_for_render_contains(&mut app, "Rename room").await;

    app.handle_input(b"#cc-renamed\r");
    wait_for_render_contains(&mut app, "Renaming #cc-admin-side to #cc-renamed...").await;
    wait_for_render_contains(&mut app, "Renamed #cc-admin-side to #cc-renamed").await;

    wait_until(
        || async {
            ChatRoom::get(&client, room.id)
                .await
                .expect("reload room")
                .and_then(|room| room.slug)
                .as_deref()
                == Some("cc-renamed")
        },
        "control center selected room to be renamed",
    )
    .await;
}

#[tokio::test]
async fn admin_must_type_room_name_before_deleting_from_control_center() {
    let test_db = new_test_db().await;
    let admin = create_test_user(&test_db.db, "cc-room-delete-admin").await;
    let client = test_db.db.get().await.expect("db client");
    let general = ChatRoom::ensure_general(&client)
        .await
        .expect("ensure general room");
    let room = ChatRoom::create_private_room(&client, "cc-delete-side")
        .await
        .expect("create room");
    ChatRoomMember::join(&client, general.id, admin.id)
        .await
        .expect("join admin to general");
    ChatRoomMember::join(&client, room.id, admin.id)
        .await
        .expect("join admin to room");
    client
        .execute(
            "UPDATE users SET is_admin = true WHERE id = $1",
            &[&admin.id],
        )
        .await
        .expect("promote admin");

    let mut app = make_app_with_permissions(
        test_db.db.clone(),
        admin.id,
        "cc-room-delete-admin-flow",
        Permissions::new(true, false),
    );

    app.handle_input(b"0");
    wait_for_render_contains(&mut app, "Staff Control Center").await;
    app.handle_input(b"l");
    wait_for_render_contains(&mut app, " Room Directory ").await;
    wait_for_render_contains(&mut app, "> #general").await;

    app.handle_input(b"j");
    wait_for_render_contains(&mut app, "> #cc-delete-side").await;

    app.handle_input(b"d");
    wait_for_render_contains(&mut app, " Delete Room ").await;
    wait_for_render_contains(&mut app, "Type #cc-delete-side to confirm delete").await;

    app.handle_input(b"#wrong\r");
    assert_render_not_contains_for(
        &mut app,
        "Deleting #cc-delete-side...",
        Duration::from_millis(200),
    )
    .await;
    wait_for_render_contains(&mut app, "Type #cc-delete-side to confirm delete").await;

    app.handle_input(b"\x1b");
    assert_render_not_contains_for(&mut app, " Delete Room ", Duration::from_millis(200)).await;

    app.handle_input(b"d");
    wait_for_render_contains(&mut app, "Type #cc-delete-side to confirm delete").await;
    app.handle_input(b"#cc-delete-side\r");
    wait_for_render_contains(&mut app, "Deleting #cc-delete-side...").await;
    wait_for_render_contains(&mut app, "Deleted #cc-delete-side").await;

    wait_until(
        || async {
            ChatRoom::get(&client, room.id)
                .await
                .expect("reload room")
                .is_none()
        },
        "control center selected room to be deleted",
    )
    .await;
}

#[tokio::test]
async fn admin_can_disconnect_selected_user_from_control_center() {
    let test_db = new_test_db().await;
    let admin = create_test_user(&test_db.db, "cc-user-admin").await;
    let target = create_test_user(&test_db.db, "cc-user-target").await;
    let client = test_db.db.get().await.expect("db client");
    client
        .execute(
            "UPDATE users SET is_admin = true WHERE id = $1",
            &[&admin.id],
        )
        .await
        .expect("promote admin");

    let session_registry = SessionRegistry::new();
    let (target_tx, mut target_rx) = tokio::sync::mpsc::channel(4);
    session_registry.register(SessionRegistration {
        session_id: Uuid::now_v7(),
        token: "cc-user-target-token".to_string(),
        user_id: target.id,
        username: target.username.clone(),
        tx: target_tx,
    });

    let mut app = make_app_with_runtime_permissions(
        test_db.db.clone(),
        admin.id,
        "cc-user-admin-flow",
        Permissions::new(true, false),
        Some(session_registry.clone()),
    );

    app.handle_input(b"0");
    wait_for_render_contains(&mut app, "Staff Control Center").await;
    wait_for_render_contains(&mut app, " User Directory ").await;
    wait_for_render_contains(&mut app, "> @cc-user-admin [admin]").await;
    wait_for_render_contains(&mut app, "@cc-user-target · online now · 1 live session").await;

    app.handle_input(b"j");
    wait_for_render_contains(&mut app, "> @cc-user-target · online now · 1 live session").await;
    wait_for_render_contains(&mut app, " Selected User ").await;
    wait_for_render_contains(&mut app, "Live Session Detail").await;

    app.handle_input(b"x");
    wait_for_render_contains(&mut app, " Disconnect User ").await;
    wait_for_render_contains(&mut app, "Type @cc-user-target to confirm disconnect").await;

    app.handle_input(b"@wrong\r");
    assert_render_not_contains_for(
        &mut app,
        "Disconnecting @cc-user-target...",
        Duration::from_millis(200),
    )
    .await;
    wait_for_render_contains(&mut app, "Type @cc-user-target to confirm disconnect").await;

    app.handle_input(b"\x1b");
    assert_render_not_contains_for(&mut app, " Disconnect User ", Duration::from_millis(200)).await;

    app.handle_input(b"x");
    wait_for_render_contains(&mut app, "Type @cc-user-target to confirm disconnect").await;
    app.handle_input(b"@cc-user-target\r");
    wait_for_render_contains(&mut app, "Disconnecting @cc-user-target...").await;
    wait_for_render_contains(&mut app, "Disconnected @cc-user-target (1 live session)").await;

    let disconnect = tokio::time::timeout(Duration::from_secs(1), target_rx.recv())
        .await
        .expect("disconnect message to arrive")
        .expect("disconnect message");
    match disconnect {
        SessionMessage::Disconnect { reason } => {
            assert_eq!(reason, "You were disconnected by an admin");
        }
        other => panic!("expected disconnect message, got {other:?}"),
    }
}

#[tokio::test]
async fn admin_can_disconnect_selected_live_session_from_control_center() {
    let test_db = new_test_db().await;
    let admin = create_test_user(&test_db.db, "cc-user-session-admin").await;
    let target = create_test_user(&test_db.db, "cc-user-session-target").await;
    let client = test_db.db.get().await.expect("db client");
    client
        .execute(
            "UPDATE users SET is_admin = true WHERE id = $1",
            &[&admin.id],
        )
        .await
        .expect("promote admin");

    let session_registry = SessionRegistry::new();
    let (target_tx_a, mut target_rx_a) = tokio::sync::mpsc::channel(4);
    let (target_tx_b, mut target_rx_b) = tokio::sync::mpsc::channel(4);
    let session_id_a = Uuid::now_v7();
    let session_id_b = Uuid::now_v7();
    session_registry.register(SessionRegistration {
        session_id: session_id_a,
        token: "cc-user-session-target-token-a".to_string(),
        user_id: target.id,
        username: target.username.clone(),
        tx: target_tx_a,
    });
    session_registry.register(SessionRegistration {
        session_id: session_id_b,
        token: "cc-user-session-target-token-b".to_string(),
        user_id: target.id,
        username: target.username.clone(),
        tx: target_tx_b,
    });

    let mut app = make_app_with_runtime_permissions(
        test_db.db.clone(),
        admin.id,
        "cc-user-session-admin-flow",
        Permissions::new(true, false),
        Some(session_registry.clone()),
    );

    let short_session_a: String = session_id_a.to_string().chars().take(8).collect();
    let short_session_b: String = session_id_b.to_string().chars().take(8).collect();

    app.handle_input(b"0");
    wait_for_render_contains(&mut app, "Staff Control Center").await;
    wait_for_render_contains(
        &mut app,
        "@cc-user-session-target · online now · 2 live sessions",
    )
    .await;

    app.handle_input(b"j");
    wait_for_render_contains(
        &mut app,
        "> @cc-user-session-target · online now · 2 live sessions",
    )
    .await;
    wait_for_render_contains(&mut app, "Live Session Detail").await;

    app.handle_input(b"\t");
    wait_for_render_contains(&mut app, "Tab focus tabs").await;
    wait_for_render_contains(&mut app, &format!("> session {}", short_session_a)).await;
    app.handle_input(b"j");
    wait_for_render_contains(&mut app, &format!("> session {}", short_session_b)).await;

    app.handle_input(b"x");
    wait_for_render_contains(&mut app, " Disconnect Session ").await;
    wait_for_render_contains(
        &mut app,
        &format!("Type {} to confirm disconnect", short_session_b),
    )
    .await;

    app.handle_input(b"wrong\r");
    assert_render_not_contains_for(
        &mut app,
        &format!(
            "Disconnecting session {} for @cc-user-session-target...",
            short_session_b
        ),
        Duration::from_millis(200),
    )
    .await;

    app.handle_input(b"\x17");
    app.handle_input(format!("{short_session_b}\r").as_bytes());
    wait_for_render_contains(
        &mut app,
        &format!(
            "Disconnecting session {} for @cc-user-session-target...",
            short_session_b
        ),
    )
    .await;
    wait_for_render_contains(
        &mut app,
        &format!(
            "Disconnected session {} for @cc-user-session-target",
            short_session_b
        ),
    )
    .await;

    let disconnect_b = tokio::time::timeout(Duration::from_secs(1), target_rx_b.recv())
        .await
        .expect("disconnect session message to arrive")
        .expect("disconnect session message");
    match disconnect_b {
        SessionMessage::Disconnect { reason } => {
            assert_eq!(reason, "You were disconnected by an admin");
        }
        other => panic!("expected disconnect message, got {other:?}"),
    }

    assert!(
        tokio::time::timeout(Duration::from_millis(200), target_rx_a.recv())
            .await
            .is_err(),
        "expected first live session to remain connected"
    );
}

#[tokio::test]
async fn admin_can_ban_and_unban_selected_user_from_control_center() {
    let test_db = new_test_db().await;
    let admin = create_test_user(&test_db.db, "cc-user-ban-admin").await;
    let target = create_test_user(&test_db.db, "cc-user-ban-target").await;
    let client = test_db.db.get().await.expect("db client");
    client
        .execute(
            "UPDATE users SET is_admin = true WHERE id = $1",
            &[&admin.id],
        )
        .await
        .expect("promote admin");

    let session_registry = SessionRegistry::new();
    let (target_tx, mut target_rx) = tokio::sync::mpsc::channel(4);
    session_registry.register(SessionRegistration {
        session_id: Uuid::now_v7(),
        token: "cc-user-ban-target-token".to_string(),
        user_id: target.id,
        username: target.username.clone(),
        tx: target_tx,
    });

    let mut app = make_app_with_runtime_permissions(
        test_db.db.clone(),
        admin.id,
        "cc-user-ban-admin-flow",
        Permissions::new(true, false),
        Some(session_registry.clone()),
    );

    app.handle_input(b"0");
    wait_for_render_contains(&mut app, "Staff Control Center").await;
    wait_for_render_contains(
        &mut app,
        "@cc-user-ban-target · online now · 1 live session",
    )
    .await;

    app.handle_input(b"j");
    wait_for_render_contains(
        &mut app,
        "> @cc-user-ban-target · online now · 1 live session",
    )
    .await;

    app.handle_input(b"b");
    wait_for_render_contains(&mut app, " Ban User ").await;
    wait_for_render_contains(&mut app, "reason (required)").await;
    wait_for_render_contains(&mut app, "duration (blank =").await;

    app.handle_input(b"policy violation\r");
    wait_for_render_contains(&mut app, "Type @cc-user-ban-target to confirm ban").await;

    app.handle_input(b"@wrong\r");
    assert_render_not_contains_for(
        &mut app,
        "Banning @cc-user-ban-target...",
        Duration::from_millis(200),
    )
    .await;

    app.handle_input(b"\x17");
    app.handle_input(b"@cc-user-ban-target\r");
    wait_for_render_contains(&mut app, "Banning @cc-user-ban-target...").await;
    wait_for_render_contains(
        &mut app,
        "Banned @cc-user-ban-target and disconnected 1 live session",
    )
    .await;
    wait_for_render_contains(&mut app, "> @cc-user-ban-target · banned").await;
    wait_for_render_contains(&mut app, "server ban:").await;
    wait_for_render_contains(&mut app, "active").await;

    wait_until(
        || async {
            ServerBan::find_active_for_user_id(&client, target.id)
                .await
                .expect("lookup active server ban")
                .is_some()
        },
        "server ban row to be created",
    )
    .await;

    let disconnect = tokio::time::timeout(Duration::from_secs(1), target_rx.recv())
        .await
        .expect("ban disconnect to arrive")
        .expect("ban disconnect message");
    match disconnect {
        SessionMessage::Disconnect { reason } => {
            assert_eq!(reason, "You were banned: policy violation");
        }
        other => panic!("expected disconnect message, got {other:?}"),
    }

    app.handle_input(b"u");
    wait_for_render_contains(&mut app, " Unban User ").await;
    wait_for_render_contains(&mut app, "Type @cc-user-ban-target to confirm unban").await;
    app.handle_input(b"@cc-user-ban-target\r");
    wait_for_render_contains(&mut app, "Unbanning @cc-user-ban-target...").await;
    wait_for_render_contains(&mut app, "Unbanned @cc-user-ban-target").await;
    wait_for_render_contains(&mut app, "server ban:").await;
    wait_for_render_contains(&mut app, "clear").await;

    wait_until(
        || async {
            ServerBan::find_active_for_user_id(&client, target.id)
                .await
                .expect("lookup active server ban")
                .is_none()
        },
        "server ban row to be cleared",
    )
    .await;
}

#[tokio::test]
async fn artboard_view_mode_allows_cursor_movement_and_screen_hotkeys() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "artboard-view-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "artboard-view-flow-it");

    app.handle_input(b"5");
    wait_for_render_contains(&mut app, "Mode       view").await;
    wait_for_render_contains(&mut app, "Cursor     0,0").await;

    app.handle_input(b"\x1b[C");
    wait_for_render_contains(&mut app, "Cursor     1,0").await;

    app.handle_input(b"1");
    wait_for_render_contains(&mut app, " Dashboard ").await;
}

#[tokio::test]
async fn artboard_view_mode_click_enters_active_mode_at_clicked_canvas_cell() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "artboard-click-enter-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "artboard-click-enter-flow-it");

    app.handle_input(b"5");
    wait_for_render_contains(&mut app, "Mode       view").await;
    wait_for_render_contains(&mut app, "Cursor     0,0").await;

    app.handle_input(b"\x1b[<0;10;5M");
    wait_for_render_contains(&mut app, "Mode       active").await;
    wait_for_render_contains(&mut app, "Cursor     8,3").await;
}

#[tokio::test]
async fn active_artboard_blocks_screen_number_hotkeys_until_escape() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "artboard-active-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "artboard-active-flow-it");

    app.handle_input(b"5");
    wait_for_render_contains(&mut app, "Mode       view").await;

    app.handle_input(b"i");
    wait_for_render_contains(&mut app, "Mode       active").await;

    app.handle_input(b"1");
    tokio::time::sleep(Duration::from_millis(60)).await;
    let frame = render_plain(&mut app);
    assert!(
        frame.contains("Mode       active"),
        "expected active artboard mode to keep focus after numeric hotkeys; frame={frame:?}"
    );
    assert!(
        !frame.contains(" Dashboard "),
        "expected active artboard mode to block screen switching; frame={frame:?}"
    );

    app.handle_input(b"\x1b");
    wait_for_render_contains(&mut app, "Mode       view").await;

    app.handle_input(b"1");
    wait_for_render_contains(&mut app, " Dashboard ").await;
}

#[tokio::test]
async fn active_artboard_ctrl_c_copies_without_quitting() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "artboard-ctrl-c-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "artboard-ctrl-c-flow-it");

    app.handle_input(b"5");
    wait_for_render_contains(&mut app, "Mode       view").await;

    app.handle_input(b"i");
    wait_for_render_contains(&mut app, "Mode       active").await;

    app.handle_input(b"\x03");
    tokio::time::sleep(Duration::from_millis(60)).await;
    let frame = render_plain(&mut app);
    assert!(
        frame.contains("Mode       swatch"),
        "expected Ctrl+C to copy into the primary swatch and stay inside active artboard; frame={frame:?}"
    );
    assert!(
        !frame.contains(" Quit? "),
        "expected Ctrl+C to avoid the global quit flow; frame={frame:?}"
    );
}

#[tokio::test]
async fn artboard_help_modal_tab_switches_help_tabs_instead_of_pages() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "artboard-help-tab-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "artboard-help-tab-flow-it");

    app.handle_input(b"5");
    wait_for_render_contains(&mut app, "Mode       view").await;

    app.handle_input(b"\x10");
    wait_for_render_contains(&mut app, "Two modes").await;

    app.handle_input(b"\t");
    wait_for_render_contains(&mut app, "Draw / erase").await;

    let frame = render_plain(&mut app);
    assert!(
        !frame.contains(" Dashboard "),
        "expected Artboard help Tab to stay on Artboard instead of switching page; frame={frame:?}"
    );
}

#[tokio::test]
async fn artboard_view_mode_question_mark_opens_local_help() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "artboard-view-help-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "artboard-view-help-flow-it");

    app.handle_input(b"5");
    wait_for_render_contains(&mut app, "Mode       view").await;

    app.handle_input(b"?");
    wait_for_render_contains(&mut app, "Two modes").await;

    let frame = render_plain(&mut app);
    assert!(
        !frame.contains(" Guide "),
        "expected ? on Artboard view mode to open local help, not the global guide; frame={frame:?}"
    );
}

#[tokio::test]
async fn active_artboard_question_mark_types_into_canvas_instead_of_opening_help() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "artboard-questionmark-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "artboard-questionmark-flow-it");

    app.handle_input(b"5");
    wait_for_render_contains(&mut app, "Mode       view").await;
    wait_for_render_contains(&mut app, "Cursor     0,0").await;

    app.handle_input(b"i");
    wait_for_render_contains(&mut app, "Mode       active").await;

    app.handle_input(b"?");
    wait_for_render_contains(&mut app, "Cursor     1,0").await;

    let frame = render_plain(&mut app);
    assert!(
        frame.contains("Mode       active"),
        "expected ? to stay inside active artboard mode; frame={frame:?}"
    );
    assert!(
        !frame.contains(" Guide "),
        "expected ? in active artboard mode to avoid the global guide; frame={frame:?}"
    );
}

#[tokio::test]
async fn dashboard_chat_compose_treats_screen_hotkeys_as_text() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "dash-chat-compose-it").await;
    let client = test_db.db.get().await.expect("db client");
    let general = ChatRoom::ensure_general(&client)
        .await
        .expect("ensure general room");
    ChatRoomMember::join(&client, general.id, user.id)
        .await
        .expect("join general room");
    let mut app = make_app(test_db.db.clone(), user.id, "dash-chat-compose-flow-it");

    // See `dashboard_chat_compose_blocks_quit_shortcut` — hop through chat
    // once to guarantee the room snapshot has populated `general_room_id`.
    app.handle_input(b"2");
    wait_for_render_contains(&mut app, "> general").await;
    app.handle_input(b"1");
    wait_for_render_contains(&mut app, " Dashboard ").await;

    app.handle_input(b"i3abc");

    wait_for_render_contains(&mut app, " Dashboard ").await;
    wait_for_render_contains(&mut app, "3abc").await;
}

#[tokio::test]
async fn chat_compose_treats_screen_hotkeys_as_text() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "chat-compose-it").await;
    let client = test_db.db.get().await.expect("db client");
    let general = ChatRoom::ensure_general(&client)
        .await
        .expect("ensure general room");
    ChatRoomMember::join(&client, general.id, user.id)
        .await
        .expect("join general room");
    let mut app = make_app(test_db.db.clone(), user.id, "chat-compose-flow-it");

    app.handle_input(b"2");
    wait_for_render_contains(&mut app, " Rooms ").await;

    app.handle_input(b"i2hey");
    wait_for_render_contains(&mut app, "2hey").await;
    wait_for_render_contains(
        &mut app,
        "Compose (Enter send, Alt+S stay, Alt+Enter newline, Esc cancel)",
    )
    .await;

    // Real terminals send CR (0x0D) for Enter in raw mode. Bare LF (0x0A) is
    // Ctrl+J and is aliased to "insert newline in chat composer", so we'd
    // end up composing "2hey\n" instead of submitting.
    app.handle_input(b"\r");
    wait_for_render_contains(&mut app, "Compose (press i)").await;
}

#[rstest]
#[case::cyrillic("cyrillic", "тест")]
#[case::han("han", "漢字")]
#[case::latin_diacritic("accented", "café")]
#[case::greek("greek", "αβγ")]
#[tokio::test]
async fn chat_compose_accepts_non_ascii_typing(#[case] label: &str, #[case] input: &str) {
    let (_db, mut app) = chat_compose_app(&format!("utf8-{label}")).await;
    app.handle_input(input.as_bytes());
    wait_for_render_contains(&mut app, input).await;
}

#[tokio::test]
async fn split_read_alt_backspace_deletes_word_without_wedging_parser() {
    let (_db, mut app) = chat_compose_app("alt-backspace-split").await;

    app.handle_input(b"one two");
    let frame = render_plain(&mut app);
    assert!(
        frame.contains("one") && frame.contains("two"),
        "expected compose render to show the initial text; frame={frame:?}"
    );

    // Simulate a terminal splitting Alt+Backspace across reads: lone ESC
    // first, then DEL on the next input chunk.
    app.handle_input(b"\x1b");
    app.handle_input(b"\x7f");
    let frame = render_plain(&mut app);
    assert!(
        frame.contains("│one │") || frame.contains("│one  │"),
        "expected split Alt+Backspace to leave the composer in the intermediate `one ` state (allowing for the cursor cell to render as an extra blank); frame={frame:?}"
    );
    assert!(
        !frame.contains("two"),
        "expected split Alt+Backspace to delete the previous word; frame={frame:?}"
    );

    // Plain Backspace must still work after the word-delete chord. Insert a
    // fresh sentinel byte first so we can verify backspace removed it without
    // depending on whether delete-word keeps the separating space.
    app.handle_input(b"x\x7f!");
    let frame = render_plain(&mut app);
    assert!(
        (frame.contains("│one!│")
            || frame.contains("│one !│")
            || frame.contains("│one ! │")
            || frame.contains("│one! │"))
            && !frame.contains("x"),
        "expected composer to keep accepting backspace and text after Alt+Backspace split, allowing for cursor-cell spacing in the rendered composer; frame={frame:?}"
    );
    assert!(
        !frame.contains("two"),
        "expected Alt+Backspace split read to delete the previous word; frame={frame:?}"
    );
}

#[tokio::test]
async fn chat_room_switch_ctrl_keys_wrap() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "chat-room-switch-it").await;
    let client = test_db.db.get().await.expect("db client");
    let general = ChatRoom::ensure_general(&client)
        .await
        .expect("ensure general room");
    ChatRoomMember::join(&client, general.id, user.id)
        .await
        .expect("join general room");
    let mut app = make_app(test_db.db.clone(), user.id, "chat-room-switch-flow-it");

    app.handle_input(b"2");
    wait_for_render_contains(&mut app, " Rooms ").await;
    wait_for_render_contains(&mut app, "> general").await;

    app.handle_input(b"\x10");
    wait_for_render_contains(&mut app, "> discover").await;

    app.handle_input(b"\x0e");
    wait_for_render_contains(&mut app, "> general").await;
}

#[tokio::test]
async fn chat_reaction_leader_uses_digits_without_switching_screens() {
    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "f-react-viewer").await;
    let author = create_test_user(&test_db.db, "f-react-author").await;
    let client = test_db.db.get().await.expect("db client");
    let general = ChatRoom::ensure_general(&client)
        .await
        .expect("ensure general room");
    ChatRoomMember::join(&client, general.id, viewer.id)
        .await
        .expect("join viewer");
    ChatRoomMember::join(&client, general.id, author.id)
        .await
        .expect("join author");
    let message = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: general.id,
            user_id: author.id,
            body: "reaction target".to_string(),
        },
    )
    .await
    .expect("create message");

    let mut app = make_app(test_db.db.clone(), viewer.id, "f-react-flow-it");
    app.handle_input(b"2");
    wait_for_render_contains(&mut app, " Rooms ").await;
    wait_for_render_contains(&mut app, "reaction target").await;

    app.handle_input(b"j");
    app.handle_input(b"f");
    wait_for_render_contains(&mut app, "1 👍").await;
    app.handle_input(b"1");

    wait_for_render_contains(&mut app, " Rooms ").await;
    wait_until(
        || async {
            ChatMessageReaction::get_by_user_and_message(&client, message.id, viewer.id)
                .await
                .expect("load reaction")
                .is_some_and(|reaction| reaction.kind == 1)
        },
        "f leader reaction to persist",
    )
    .await;
}

#[tokio::test]
async fn chat_room_list_is_mouse_clickable() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "chat-room-mouse-it").await;
    let author = create_test_user(&test_db.db, "chat-room-mouse-author-it").await;
    let client = test_db.db.get().await.expect("db client");
    let general = ChatRoom::ensure_general(&client)
        .await
        .expect("ensure general room");
    let rust = ChatRoom::get_or_create_public_room(&client, "rust")
        .await
        .expect("create rust room");
    for room in [general.id, rust.id] {
        ChatRoomMember::join(&client, room, user.id)
            .await
            .expect("join viewer");
        ChatRoomMember::join(&client, room, author.id)
            .await
            .expect("join author");
    }
    ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: rust.id,
            user_id: author.id,
            body: "rust room backlog".to_string(),
        },
    )
    .await
    .expect("create rust message");

    let mut app = make_app(test_db.db.clone(), user.id, "chat-room-mouse-flow-it");
    app.handle_input(b"2");
    wait_for_render_contains(&mut app, " Rooms ").await;
    wait_for_render_contains(&mut app, "rust").await;

    let plain = render_plain(&mut app);
    let rust_offset = plain
        .find("rust")
        .unwrap_or_else(|| panic!("rust room row should render: {plain:?}"));
    let rust_y = rust_offset / 100 + 1;
    let click = format!("\x1b[<0;5;{rust_y}M");
    app.handle_input(click.as_bytes());

    wait_for_render_contains(&mut app, "rust room backlog").await;
}

#[tokio::test]
async fn chat_reaction_leader_persists_extended_reaction_digits() {
    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "f-react-extended-viewer").await;
    let author = create_test_user(&test_db.db, "f-react-extended-author").await;
    let client = test_db.db.get().await.expect("db client");
    let general = ChatRoom::ensure_general(&client)
        .await
        .expect("ensure general room");
    ChatRoomMember::join(&client, general.id, viewer.id)
        .await
        .expect("join viewer");
    ChatRoomMember::join(&client, general.id, author.id)
        .await
        .expect("join author");
    let message = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: general.id,
            user_id: author.id,
            body: "extended reaction target".to_string(),
        },
    )
    .await
    .expect("create message");

    let mut app = make_app(test_db.db.clone(), viewer.id, "f-react-extended-flow-it");
    app.handle_input(b"2");
    wait_for_render_contains(&mut app, " Rooms ").await;
    wait_for_render_contains(&mut app, "extended reaction target").await;

    app.handle_input(b"j");
    app.handle_input(b"f");
    wait_for_render_contains(&mut app, "8 🤔").await;
    app.handle_input(b"8");

    wait_for_render_contains(&mut app, " Rooms ").await;
    wait_until(
        || async {
            ChatMessageReaction::get_by_user_and_message(&client, message.id, viewer.id)
                .await
                .expect("load reaction")
                .is_some_and(|reaction| reaction.kind == 8)
        },
        "extended f leader reaction to persist",
    )
    .await;
}

#[tokio::test]
async fn chat_reaction_leader_cancels_and_consumes_non_digit_input() {
    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "f-cancel-viewer").await;
    let author = create_test_user(&test_db.db, "f-cancel-author").await;
    let client = test_db.db.get().await.expect("db client");
    let general = ChatRoom::ensure_general(&client)
        .await
        .expect("ensure general room");
    ChatRoomMember::join(&client, general.id, viewer.id)
        .await
        .expect("join viewer");
    ChatRoomMember::join(&client, general.id, author.id)
        .await
        .expect("join author");
    let message = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: general.id,
            user_id: author.id,
            body: "cancel target".to_string(),
        },
    )
    .await
    .expect("create message");

    let mut app = make_app(test_db.db.clone(), viewer.id, "f-cancel-flow-it");
    app.handle_input(b"2");
    wait_for_render_contains(&mut app, "cancel target").await;

    app.handle_input(b"j");
    app.handle_input(b"f");
    wait_for_render_contains(&mut app, "1 👍").await;

    app.handle_input(b"r");
    assert_render_not_contains_for(
        &mut app,
        "Reply to @f-cancel-author",
        Duration::from_millis(250),
    )
    .await;

    let plain = render_plain(&mut app);
    assert!(!plain.contains("1 👍"), "picker should close: {plain:?}");
    assert!(
        plain.contains("cancel target"),
        "message should remain selected: {plain:?}"
    );
    assert!(
        ChatMessageReaction::get_by_user_and_message(&client, message.id, viewer.id)
            .await
            .expect("load reaction")
            .is_none(),
        "non-digit input should not react",
    );
}

#[tokio::test]
async fn help_command_renders_chat_feedback_without_persisting_message() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "help-notice-it").await;
    let client = test_db.db.get().await.expect("db client");
    let general = ChatRoom::ensure_general(&client)
        .await
        .expect("ensure general room");
    ChatRoomMember::join(&client, general.id, user.id)
        .await
        .expect("join general room");
    let mut app = make_app(test_db.db.clone(), user.id, "help-notice-flow-it");

    app.handle_input(b"2");
    wait_for_render_contains(&mut app, " Rooms ").await;

    app.handle_input(b"i/binds\r");
    wait_for_render_contains(&mut app, " Guide ").await;
    wait_for_render_contains(&mut app, " Chat ").await;
    wait_for_render_contains(&mut app, "/ignore [@user]").await;

    let messages = ChatMessage::list_recent(&client, general.id, 20)
        .await
        .expect("list recent messages");
    assert!(messages.is_empty(), "expected /binds to stay client-side");
}

#[tokio::test]
async fn members_command_shows_room_members_without_persisting_message() {
    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "list-flow-viewer").await;
    let target = create_test_user(&test_db.db, "list-flow-target").await;
    let client = test_db.db.get().await.expect("db client");
    let general = ChatRoom::ensure_general(&client)
        .await
        .expect("ensure general room");
    ChatRoomMember::join(&client, general.id, viewer.id)
        .await
        .expect("join viewer to general");

    let private_room = ChatRoom::create_private_room(&client, "side")
        .await
        .expect("create room");
    ChatRoomMember::join(&client, private_room.id, viewer.id)
        .await
        .expect("join viewer to side");
    ChatRoomMember::join(&client, private_room.id, target.id)
        .await
        .expect("join target to side");

    let mut app = make_app(test_db.db.clone(), viewer.id, "list-room-members-flow-it");

    app.handle_input(b"2");
    wait_for_render_contains(&mut app, " Rooms ").await;
    wait_for_render_contains(&mut app, "> general").await;
    wait_for_render_contains(&mut app, " Private ").await;
    wait_for_render_contains(&mut app, " side").await;

    app.handle_input(b" ");
    wait_for_render_contains(&mut app, "[h] side").await;
    app.handle_input(b"h");
    wait_for_render_contains(&mut app, "> side").await;

    app.handle_input(b"i/members\r");
    wait_for_render_contains(&mut app, "#side Members").await;
    wait_for_render_contains(&mut app, "@list-flow-viewer").await;
    wait_for_render_contains(&mut app, "@list-flow-target").await;

    let messages = ChatMessage::list_recent(&client, private_room.id, 20)
        .await
        .expect("list recent messages");
    assert!(messages.is_empty(), "expected /members to stay client-side");
}

#[tokio::test]
async fn exit_command_opens_quit_confirm_and_stays_client_side() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "exit-command-it").await;
    let client = test_db.db.get().await.expect("db client");
    let general = ChatRoom::ensure_general(&client)
        .await
        .expect("ensure general room");
    ChatRoomMember::join(&client, general.id, user.id)
        .await
        .expect("join user to general");

    let mut app = make_app(test_db.db.clone(), user.id, "exit-command-flow-it");

    app.handle_input(b"2");
    wait_for_render_contains(&mut app, " Rooms ").await;
    wait_for_render_contains(&mut app, "> general").await;

    app.handle_input(b"i/exit\r");
    wait_for_render_contains(&mut app, " Quit? ").await;

    let messages = ChatMessage::list_recent(&client, general.id, 20)
        .await
        .expect("list recent messages");
    assert!(messages.is_empty(), "expected /exit to stay client-side");
}

#[tokio::test]
async fn ignore_command_hides_messages_and_persists_across_refresh() {
    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "ignore-flow-viewer").await;
    let target = create_test_user(&test_db.db, "ignore-flow-target").await;
    let client = test_db.db.get().await.expect("db client");
    let general = ChatRoom::ensure_general(&client)
        .await
        .expect("ensure general room");
    ChatRoomMember::join(&client, general.id, viewer.id)
        .await
        .expect("join viewer");
    ChatRoomMember::join(&client, general.id, target.id)
        .await
        .expect("join target");
    ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: general.id,
            user_id: target.id,
            body: "message from ignored user".to_string(),
        },
    )
    .await
    .expect("create message");

    let (mut app, chat_service) =
        make_app_with_chat_service(test_db.db.clone(), viewer.id, "ignore-command-flow-it");
    app.handle_input(b"2");
    wait_for_render_contains(&mut app, " Rooms ").await;
    wait_for_render_contains(&mut app, "message from ignored user").await;

    app.handle_input(b"i");
    app.handle_input(b"/ignore ignore-flow-target\r");
    wait_for_render_contains(&mut app, "Ignored @ignore-flow-target").await;

    let ignored = User::ignored_user_ids(&client, viewer.id)
        .await
        .expect("load ignore list");
    assert_eq!(ignored, vec![target.id]);

    let post_ignore_body = "fresh message from ignored user";
    chat_service.send_message_task(
        target.id,
        general.id,
        Some("general".to_string()),
        post_ignore_body.to_string(),
        Uuid::now_v7(),
        Permissions::default(),
    );
    wait_until(
        || async {
            ChatMessage::list_recent(&client, general.id, 20)
                .await
                .expect("list recent messages")
                .iter()
                .any(|message| message.body == post_ignore_body)
        },
        "post-ignore message to persist",
    )
    .await;

    helpers::assert_render_not_contains_for(&mut app, post_ignore_body, Duration::from_millis(300))
        .await;

    let mut refreshed_app = make_app(test_db.db.clone(), viewer.id, "ignore-command-refresh-it");
    refreshed_app.handle_input(b"2");
    wait_for_render_contains(&mut refreshed_app, " Rooms ").await;
    helpers::assert_render_not_contains_for(
        &mut refreshed_app,
        post_ignore_body,
        Duration::from_millis(300),
    )
    .await;
}

#[tokio::test]
async fn mod_room_command_opens_overlay_and_kicks_selected_room_member() {
    let test_db = new_test_db().await;
    let moderator = create_test_user(&test_db.db, "mod-room-flow-viewer").await;
    let target = create_test_user(&test_db.db, "mod-room-flow-target").await;
    let client = test_db.db.get().await.expect("db client");
    let general = ChatRoom::ensure_general(&client)
        .await
        .expect("ensure general room");
    ChatRoomMember::join(&client, general.id, moderator.id)
        .await
        .expect("join moderator to general");
    let room = ChatRoom::create_private_room(&client, "mod-side")
        .await
        .expect("create room");
    ChatRoomMember::join(&client, room.id, moderator.id)
        .await
        .expect("join moderator to side");
    ChatRoomMember::join(&client, room.id, target.id)
        .await
        .expect("join target to side");
    client
        .execute(
            "UPDATE users SET is_moderator = true WHERE id = $1",
            &[&moderator.id],
        )
        .await
        .expect("promote moderator");

    let mut app = make_app_with_permissions(
        test_db.db.clone(),
        moderator.id,
        "mod-room-flow-it",
        Permissions::new(false, true),
    );

    app.handle_input(b"2");
    wait_for_render_contains(&mut app, " Rooms ").await;
    wait_for_render_contains(&mut app, "> general").await;
    wait_for_render_contains(&mut app, " mod-side").await;

    app.handle_input(b" ");
    wait_for_render_contains(&mut app, "[g] mod-side").await;
    app.handle_input(b"g");
    wait_for_render_contains(&mut app, "> mod-side").await;

    app.handle_input(b"i/mod room\r");
    wait_for_render_contains(&mut app, "Mod Room").await;
    wait_for_render_contains(&mut app, "/mod room kick @user").await;

    app.handle_input(b"\x1b");
    app.handle_input(b"i/mod room kick @mod-room-flow-target\r");
    wait_for_render_contains(&mut app, "Kicking @mod-room-flow-target in #mod-side...").await;
    wait_for_render_contains(&mut app, "Kicked @mod-room-flow-target in #mod-side").await;

    wait_until(
        || async {
            !ChatRoomMember::is_member(&client, room.id, target.id)
                .await
                .expect("load target membership")
        },
        "target to be removed from selected room",
    )
    .await;
}

#[tokio::test]
async fn admin_room_command_opens_overlay_and_renames_selected_room() {
    let test_db = new_test_db().await;
    let admin = create_test_user(&test_db.db, "admin-room-flow-viewer").await;
    let client = test_db.db.get().await.expect("db client");
    let general = ChatRoom::ensure_general(&client)
        .await
        .expect("ensure general room");
    ChatRoomMember::join(&client, general.id, admin.id)
        .await
        .expect("join admin to general");
    let room = ChatRoom::create_private_room(&client, "admin-side")
        .await
        .expect("create room");
    ChatRoomMember::join(&client, room.id, admin.id)
        .await
        .expect("join admin to side");
    client
        .execute(
            "UPDATE users SET is_admin = true WHERE id = $1",
            &[&admin.id],
        )
        .await
        .expect("promote admin");

    let mut app = make_app_with_permissions(
        test_db.db.clone(),
        admin.id,
        "admin-room-flow-it",
        Permissions::new(true, false),
    );

    app.handle_input(b"2");
    wait_for_render_contains(&mut app, " Rooms ").await;
    wait_for_render_contains(&mut app, "> general").await;
    wait_for_render_contains(&mut app, " admin-side").await;

    app.handle_input(b" ");
    wait_for_render_contains(&mut app, "[g] admin-side").await;
    app.handle_input(b"g");
    wait_for_render_contains(&mut app, "> admin-side").await;

    app.handle_input(b"i/admin room\r");
    wait_for_render_contains(&mut app, "Admin Room").await;
    wait_for_render_contains(&mut app, "/admin room rename #new").await;

    app.handle_input(b"\x1b");
    app.handle_input(b"i/admin room rename #admin-suite\r");
    wait_for_render_contains(&mut app, "Renaming #admin-side to #admin-suite...").await;
    wait_for_render_contains(&mut app, "Renamed #admin-side to #admin-suite").await;

    wait_until(
        || async {
            ChatRoom::get(&client, room.id)
                .await
                .expect("reload room")
                .and_then(|room| room.slug)
                .as_deref()
                == Some("admin-suite")
        },
        "selected room to be renamed",
    )
    .await;
}
