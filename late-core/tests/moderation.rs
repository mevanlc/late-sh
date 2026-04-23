use chrono::{Duration, Utc};
use late_core::{
    models::{
        chat_room::ChatRoom,
        moderation_audit_log::ModerationAuditLog,
        room_ban::{RoomBan, RoomBanParams},
        server_ban::{ServerBan, ServerBanParams},
    },
    test_utils::{create_test_user, test_db},
};
use serde_json::json;

#[tokio::test]
async fn moderation_audit_log_records_actions() {
    let test_db = test_db().await;
    let actor = create_test_user(&test_db.db, "audit-actor").await;
    let client = test_db.db.get().await.expect("db client");
    let target_id = uuid::Uuid::now_v7();

    let entry = ModerationAuditLog::record(
        &client,
        actor.id,
        "room_ban_created",
        "room_ban",
        Some(target_id),
        json!({ "reason": "spam" }),
    )
    .await
    .expect("record audit entry");

    assert_eq!(entry.actor_user_id, actor.id);
    assert_eq!(entry.action, "room_ban_created");
    assert_eq!(entry.target_kind, "room_ban");
    assert_eq!(entry.target_id, Some(target_id));
    assert_eq!(entry.metadata, json!({ "reason": "spam" }));
}

#[tokio::test]
async fn room_ban_active_lookup_ignores_expired_rows() {
    let test_db = test_db().await;
    let actor = create_test_user(&test_db.db, "room-ban-actor").await;
    let target = create_test_user(&test_db.db, "room-ban-target").await;
    let client = test_db.db.get().await.expect("db client");
    let room = ChatRoom::ensure_general(&client)
        .await
        .expect("ensure general room");

    let expired = RoomBan::create(
        &client,
        RoomBanParams {
            room_id: room.id,
            target_user_id: target.id,
            actor_user_id: actor.id,
            reason: "expired".to_string(),
            expires_at: Some(Utc::now() - Duration::minutes(5)),
        },
    )
    .await
    .expect("create expired room ban");

    assert!(
        RoomBan::find_active_for_room_and_user(&client, room.id, target.id)
            .await
            .expect("lookup expired room ban")
            .is_none()
    );

    RoomBan::update(
        &client,
        expired.id,
        RoomBanParams {
            room_id: room.id,
            target_user_id: target.id,
            actor_user_id: actor.id,
            reason: "active".to_string(),
            expires_at: Some(Utc::now() + Duration::minutes(5)),
        },
    )
    .await
    .expect("update room ban");

    let active = RoomBan::find_active_for_room_and_user(&client, room.id, target.id)
        .await
        .expect("lookup active room ban")
        .expect("active room ban");
    assert_eq!(active.reason, "active");
    assert!(
        RoomBan::is_active_for_room_and_user(&client, room.id, target.id)
            .await
            .expect("active room ban check")
    );
}

#[tokio::test]
async fn server_ban_can_be_found_by_user_id_or_fingerprint() {
    let test_db = test_db().await;
    let actor = create_test_user(&test_db.db, "server-ban-actor").await;
    let target = create_test_user(&test_db.db, "server-ban-target").await;
    let client = test_db.db.get().await.expect("db client");

    ServerBan::create(
        &client,
        ServerBanParams {
            target_user_id: Some(target.id),
            fingerprint: Some(target.fingerprint.clone()),
            actor_user_id: actor.id,
            reason: "sockpuppet".to_string(),
            expires_at: None,
        },
    )
    .await
    .expect("create server ban");

    let by_user = ServerBan::find_active_for_user_id(&client, target.id)
        .await
        .expect("lookup by user id")
        .expect("server ban by user id");
    assert_eq!(by_user.reason, "sockpuppet");

    let by_fingerprint = ServerBan::find_active_for_fingerprint(&client, &target.fingerprint)
        .await
        .expect("lookup by fingerprint")
        .expect("server ban by fingerprint");
    assert_eq!(by_fingerprint.actor_user_id, actor.id);
}
