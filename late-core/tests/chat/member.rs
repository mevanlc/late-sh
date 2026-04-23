use late_core::{
    models::{
        chat_room::ChatRoom,
        chat_room_member::ChatRoomMember,
        room_ban::RoomBan,
        user::{User, UserParams},
    },
    test_utils::test_db,
};

#[tokio::test]
async fn test_chat_room_member() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let room = ChatRoom::ensure_general(&client)
        .await
        .expect("ensure general");

    let user = User::create(
        &client,
        UserParams {
            fingerprint: "member-user-1".to_string(),
            username: "m1".to_string(),
            settings: serde_json::json!({}),
        },
    )
    .await
    .unwrap();

    // auto join public
    ChatRoomMember::auto_join_public_rooms(&client, user.id)
        .await
        .unwrap();

    assert!(
        ChatRoomMember::is_member(&client, room.id, user.id)
            .await
            .unwrap()
    );

    let ids = ChatRoomMember::list_user_ids(&client, room.id)
        .await
        .unwrap();
    assert!(ids.contains(&user.id));

    ChatRoomMember::mark_read_now(&client, room.id, user.id)
        .await
        .unwrap();
    let counts = ChatRoomMember::unread_counts_for_user(&client, user.id)
        .await
        .unwrap();
    assert_eq!(counts.get(&room.id), Some(&0));
}

#[tokio::test]
async fn auto_join_public_rooms_skips_rooms_with_active_bans() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let banned_room = ChatRoom::get_or_create_public_room(&client, "banned-auto")
        .await
        .expect("create banned room");
    let allowed_room = ChatRoom::ensure_general(&client)
        .await
        .expect("ensure general");

    client
        .execute(
            "UPDATE chat_rooms SET auto_join = true WHERE id = $1",
            &[&banned_room.id],
        )
        .await
        .expect("mark room auto join");

    let user = User::create(
        &client,
        UserParams {
            fingerprint: "member-user-2".to_string(),
            username: "m2".to_string(),
            settings: serde_json::json!({}),
        },
    )
    .await
    .unwrap();
    let actor = User::create(
        &client,
        UserParams {
            fingerprint: "member-user-3".to_string(),
            username: "m3".to_string(),
            settings: serde_json::json!({}),
        },
    )
    .await
    .unwrap();

    RoomBan::activate(&client, banned_room.id, user.id, actor.id, "", None)
        .await
        .expect("create room ban");

    ChatRoomMember::auto_join_public_rooms(&client, user.id)
        .await
        .expect("auto join public rooms");

    assert!(
        ChatRoomMember::is_member(&client, allowed_room.id, user.id)
            .await
            .expect("check general membership")
    );
    assert!(
        !ChatRoomMember::is_member(&client, banned_room.id, user.id)
            .await
            .expect("check banned room membership")
    );
}
