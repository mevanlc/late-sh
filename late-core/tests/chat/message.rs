use late_core::{
    models::{
        chat_message::{ChatMessage, ChatMessageParams},
        chat_room::ChatRoom,
        user::{User, UserParams},
    },
    test_utils::test_db,
};

#[tokio::test]
async fn test_chat_message() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let room = ChatRoom::ensure_general(&client)
        .await
        .expect("ensure general");

    let user = User::create(
        &client,
        UserParams {
            fingerprint: "msg-user-1".to_string(),
            username: "u1".to_string(),
            settings: serde_json::json!({}),
        },
    )
    .await
    .unwrap();

    let msg1 = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: room.id,
            user_id: user.id,
            body: "Hello world".to_string(),
        },
    )
    .await
    .unwrap();

    let msgs = ChatMessage::list_recent(&client, room.id, 10)
        .await
        .unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].id, msg1.id);

    let edited = ChatMessage::edit_by_author(&client, msg1.id, user.id, "Hello modified")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(edited.body, "Hello modified");
    assert!(edited.updated > edited.created);

    ChatMessage::delete_by_author(&client, msg1.id, user.id)
        .await
        .unwrap();

    let msgs_after_delete = ChatMessage::list_recent(&client, room.id, 10)
        .await
        .unwrap();
    assert!(msgs_after_delete.is_empty());
}
