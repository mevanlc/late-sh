use late_core::{
    models::{
        chat_room::ChatRoom,
        chat_room_member::ChatRoomMember,
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
