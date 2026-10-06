use super::{
    channel_block::{choices, effective_ids, ensure_visible, is_blockable, set_blocked},
    chat_message::{ChatMessage, ChatMessageParams, HistoryDirection},
    chat_room::ChatRoom,
    chat_room_member::ChatRoomMember,
    notification::Notification,
    user::{User, extract_blocked_room_ids},
};
use crate::test_utils::{create_test_user, test_db};
use serde_json::json;
use uuid::Uuid;

#[tokio::test]
async fn channel_block_policy_protects_core_and_special_rooms_even_with_saved_ids() {
    let db = test_db().await;
    let user = create_test_user(&db.db, "block_policy").await;
    let mut client = db.db.get().await.unwrap();
    let room = ChatRoom::get_or_create_public_room(&client, "ordinary")
        .await
        .unwrap();
    assert!(is_blockable(&room));
    for (kind, slug, permanent, auto_join) in [
        ("topic", "ordinary", true, false),
        ("topic", "ordinary", false, true),
    ] {
        client
            .execute(
                "UPDATE chat_rooms SET kind=$2, slug=$3, permanent=$4, auto_join=$5 WHERE id=$1",
                &[&room.id, &kind, &slug, &permanent, &auto_join],
            )
            .await
            .unwrap();
        assert!(
            set_blocked(&mut client, user.id, room.id, true)
                .await
                .is_err(),
            "{kind}/{slug}"
        );
        User::update_settings(
            &client,
            user.id,
            &json!({"blocked_room_ids": [room.id.to_string()]}),
        )
        .await
        .unwrap();
        assert!(effective_ids(&**client, user.id).await.unwrap().is_empty());
        ensure_visible(&client, user.id, room.id).await.unwrap();
    }
    for kind in ["dm", "game", "nightcap", "deadchannel"] {
        let mut special = room.clone();
        special.kind = kind.into();
        assert!(!is_blockable(&special));
    }
    for slug in [
        "moderators",
        "dnd",
        "lounge",
        "announcements",
        "bugs",
        "suggestions",
        "voice",
    ] {
        let protected = if slug == "lounge" {
            ChatRoom::ensure_lounge(&client).await.unwrap()
        } else {
            ChatRoom::get_or_create_public_room(&client, slug)
                .await
                .unwrap()
        };
        client
            .execute(
                "UPDATE chat_rooms SET permanent=false, auto_join=false WHERE id=$1",
                &[&protected.id],
            )
            .await
            .unwrap();
        assert!(
            set_blocked(&mut client, user.id, protected.id, true)
                .await
                .is_err()
        );
        User::update_settings(
            &client,
            user.id,
            &json!({"blocked_room_ids": [protected.id.to_string()]}),
        )
        .await
        .unwrap();
        ensure_visible(&client, user.id, protected.id)
            .await
            .unwrap();
    }
    let language = ChatRoom::get_or_create_language(&client, "es")
        .await
        .unwrap();
    assert!(is_blockable(&language));
    set_blocked(&mut client, user.id, language.id, true)
        .await
        .unwrap();
}

#[tokio::test]
async fn channel_block_filters_before_limits_and_preserves_unread_membership_and_irc() {
    let db = test_db().await;
    let reader = create_test_user(&db.db, "block_reader").await;
    let actor = create_test_user(&db.db, "block_actor").await;
    let mut client = db.db.get().await.unwrap();
    let visible = ChatRoom::get_or_create_public_room(&client, "visible")
        .await
        .unwrap();
    let hidden = ChatRoom::get_or_create_public_room(&client, "hidden")
        .await
        .unwrap();
    for room in [&visible, &hidden] {
        ChatRoomMember::join(&client, room.id, reader.id)
            .await
            .unwrap();
    }
    let mut messages = Vec::new();
    for room in [&visible, &hidden, &hidden, &hidden] {
        let message = ChatMessage::create(
            &client,
            ChatMessageParams {
                room_id: room.id,
                user_id: actor.id,
                body: "blockneedle @block_reader".into(),
            },
        )
        .await
        .unwrap();
        Notification::create_mentions_batch(&client, &[reader.id], actor.id, message.id, room.id)
            .await
            .unwrap();
        messages.push(message);
    }
    let before = ChatRoom::list_for_user_with_state(&client, reader.id, None)
        .await
        .unwrap();
    set_blocked(&mut client, reader.id, hidden.id, true)
        .await
        .unwrap();
    let search = ChatMessage::search_for_user(&client, reader.id, "blockneedle", None, &[], 1)
        .await
        .unwrap();
    assert_eq!(search.len(), 1);
    assert_eq!(search[0].room_id, visible.id);
    let mentions = Notification::list_for_user(&client, reader.id, 1)
        .await
        .unwrap();
    assert_eq!(mentions.len(), 1);
    assert_eq!(mentions[0].room_id, visible.id);
    assert_eq!(
        Notification::unread_count(&client, reader.id)
            .await
            .unwrap(),
        1
    );
    assert!(
        ChatMessage::get_for_viewer(&client, messages[1].id, reader.id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        ChatMessage::list_page_for_viewer(
            &client,
            hidden.id,
            reader.id,
            None,
            HistoryDirection::Older,
            &[],
            10
        )
        .await
        .unwrap()
        .is_empty()
    );
    assert!(
        ChatRoomMember::is_member(&client, hidden.id, reader.id)
            .await
            .unwrap()
    );
    assert!(
        ChatRoom::list_for_user(&client, reader.id)
            .await
            .unwrap()
            .iter()
            .any(|r| r.id == hidden.id)
    );
    assert!(
        ChatRoom::find_irc_channel_by_slug_for_user(&client, "hidden", reader.id)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        ChatRoom::list_for_user_with_state(&client, reader.id, None)
            .await
            .unwrap()
            .rooms
            .iter()
            .all(|r| r.id != hidden.id)
    );
    assert!(
        ChatMessage::get_for_viewer(&client, messages[1].id, actor.id)
            .await
            .unwrap()
            .is_some()
    );
    Notification::mark_all_read(&client, reader.id)
        .await
        .unwrap();
    set_blocked(&mut client, reader.id, hidden.id, false)
        .await
        .unwrap();
    let after = ChatRoom::list_for_user_with_state(&client, reader.id, None)
        .await
        .unwrap();
    assert_eq!(
        after.unread_counts[&hidden.id],
        before.unread_counts[&hidden.id]
    );
    assert_eq!(
        Notification::unread_count(&client, reader.id)
            .await
            .unwrap(),
        3,
        "hidden mentions remain unread"
    );
}

#[tokio::test]
async fn channel_block_choices_hide_private_metadata_and_follow_identity() {
    let db = test_db().await;
    let user = create_test_user(&db.db, "block_choices").await;
    let owner = create_test_user(&db.db, "block_owner").await;
    let mut client = db.db.get().await.unwrap();
    let public = ChatRoom::get_or_create_public_room(&client, "block_public")
        .await
        .unwrap();
    let private = ChatRoom::create_private_room(&client, "secret_name", owner.id)
        .await
        .unwrap();
    assert!(
        choices(&client, user.id)
            .await
            .unwrap()
            .iter()
            .any(|c| c.room_id == public.id)
    );
    assert!(
        choices(&client, user.id)
            .await
            .unwrap()
            .iter()
            .all(|c| c.room_id != private.id)
    );
    assert!(
        set_blocked(&mut client, user.id, private.id, true)
            .await
            .is_err()
    );
    ChatRoomMember::join(&client, private.id, user.id)
        .await
        .unwrap();
    set_blocked(&mut client, user.id, private.id, true)
        .await
        .unwrap();
    client
        .execute(
            "UPDATE chat_rooms SET slug='renamed_secret' WHERE id=$1",
            &[&private.id],
        )
        .await
        .unwrap();
    assert!(
        choices(&client, user.id)
            .await
            .unwrap()
            .iter()
            .any(|c| c.room_id == private.id && c.blocked && c.label.contains("renamed_secret"))
    );
    ChatRoomMember::leave(&client, private.id, user.id)
        .await
        .unwrap();
    let choice = choices(&client, user.id)
        .await
        .unwrap()
        .into_iter()
        .find(|c| c.room_id == private.id)
        .unwrap();
    assert!(choice.label.starts_with("Unavailable channel"));
    assert!(!choice.label.contains("secret"));
    set_blocked(&mut client, user.id, private.id, false)
        .await
        .unwrap();
    set_blocked(&mut client, user.id, public.id, true)
        .await
        .unwrap();
    client
        .execute("DELETE FROM chat_rooms WHERE id=$1", &[&public.id])
        .await
        .unwrap();
    let replacement = ChatRoom::get_or_create_public_room(&client, "block_public")
        .await
        .unwrap();
    assert_ne!(replacement.id, public.id);
    assert!(
        !effective_ids(&**client, user.id)
            .await
            .unwrap()
            .contains(&replacement.id)
    );
    set_blocked(&mut client, user.id, public.id, false)
        .await
        .unwrap();
    assert!(
        !ChatRoomMember::is_member(&client, replacement.id, user.id)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn channel_block_concurrent_preference_writes_preserve_each_other() {
    let db = test_db().await;
    let user = create_test_user(&db.db, "block_concurrent").await;
    let friend = create_test_user(&db.db, "block_friend").await;
    let ignored = create_test_user(&db.db, "block_ignored").await;
    let mut first = db.db.get().await.unwrap();
    let mut second = db.db.get().await.unwrap();
    let other = db.db.get().await.unwrap();
    let a = ChatRoom::get_or_create_public_room(&first, "concurrent_a")
        .await
        .unwrap();
    let b = ChatRoom::get_or_create_public_room(&first, "concurrent_b")
        .await
        .unwrap();
    let (a_result, b_result, friend_result, ignore_result) = tokio::join!(
        set_blocked(&mut first, user.id, a.id, true),
        set_blocked(&mut second, user.id, b.id, true),
        User::add_friend_user_id(&other, user.id, friend.id),
        User::add_ignored_user_id(&other, user.id, ignored.id),
    );
    a_result.unwrap();
    b_result.unwrap();
    friend_result.unwrap();
    ignore_result.unwrap();
    let stored = User::get(&first, user.id).await.unwrap().unwrap();
    assert_eq!(extract_blocked_room_ids(&stored.settings).len(), 2);
    let (friends, ignored_ids) = User::friend_and_ignored_user_ids(&first, user.id)
        .await
        .unwrap();
    assert_eq!(friends, vec![friend.id]);
    assert_eq!(ignored_ids, vec![ignored.id]);
    set_blocked(&mut first, user.id, a.id, false).await.unwrap();
    assert_eq!(effective_ids(&**first, user.id).await.unwrap(), vec![b.id]);
}

#[test]
fn channel_block_setting_parsing_ignores_invalid_and_duplicate_values() {
    let id = Uuid::new_v4();
    assert_eq!(
        extract_blocked_room_ids(
            &json!({"blocked_room_ids": [id.to_string(), format!(" {id} "), "bad", null, 7]})
        ),
        vec![id]
    );
    assert!(extract_blocked_room_ids(&json!({"blocked_room_ids": "bad"})).is_empty());
}
