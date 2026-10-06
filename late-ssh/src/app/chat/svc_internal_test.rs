use super::*;
use chrono::Duration as ChronoDuration;
use late_core::models::chat_poll::{ChatPoll, ChatPollOptionSummary};

#[tokio::test]
async fn channel_block_guards_tui_opening_and_sending_while_irc_keeps_membership() {
    let db = crate::test_helpers::new_test_db().await;
    let user = late_core::test_utils::create_test_user(&db.db, "block_service").await;
    let service = ChatService::new(
        db.db.clone(),
        super::super::notifications::svc::NotificationService::new(db.db.clone()),
    );
    let room_id = service
        .open_public_room(user.id, "blocked_service_room")
        .await
        .unwrap();
    let client = db.db.get().await.unwrap();
    let channel = VoiceChannel::upsert_for_target(
        &client,
        late_core::models::voice_channel::TARGET_CHAT_ROOM,
        room_id,
        "Voice",
        true,
    )
    .await
    .unwrap();
    drop(client);
    service
        .set_channel_blocked(user.id, room_id, true)
        .await
        .unwrap();
    assert!(
        service
            .open_public_room(user.id, "blocked_service_room")
            .await
            .unwrap_err()
            .to_string()
            .contains("blocked")
    );
    assert!(service.join_public_room(user.id, room_id).await.is_err());
    assert!(service.load_room_tail(user.id, room_id).await.is_err());
    assert!(service.list_room_members(user.id, room_id).await.is_err());
    let snapshot = service.build_chat_snapshot(user.id).await.unwrap();
    assert!(
        snapshot
            .chat_rooms
            .iter()
            .all(|(room, _)| room.id != room_id)
    );
    assert!(snapshot.blocked_room_ids.contains(&room_id));
    // Keep the channel identity so a session already in voice can leave it.
    assert_eq!(snapshot.voice_channels_by_room_id[&room_id].id, channel.id);
    assert!(
        service
            .list_public_rooms(user.id)
            .await
            .unwrap()
            .1
            .iter()
            .all(|label| !label.contains("blocked-service-room"))
    );
    for (origin, expected) in [(MessageOrigin::Tui, false), (MessageOrigin::Irc, true)] {
        let result = service
            .send_message(SendMessageParams {
                origin,
                user_id: user.id,
                room_id,
                room_slug: Some("blocked_service_room".into()),
                body: "origin check".into(),
                reply_to_message_id: None,
                reply_to_user_id: None,
                is_admin: false,
            })
            .await;
        assert_eq!(result.is_ok(), expected, "{origin:?}: {result:?}");
    }
    service
        .set_channel_blocked(user.id, room_id, false)
        .await
        .unwrap();
    assert!(
        service
            .build_chat_snapshot(user.id)
            .await
            .unwrap()
            .chat_rooms
            .iter()
            .any(|(room, _)| room.id == room_id)
    );
    let client = db.db.get().await.unwrap();
    let unjoined = ChatRoom::get_or_create_public_room(&client, "blocked_discovery")
        .await
        .unwrap();
    service
        .set_channel_blocked(user.id, unjoined.id, true)
        .await
        .unwrap();
    assert!(
        service
            .list_discover_rooms(user.id)
            .await
            .unwrap()
            .iter()
            .all(|room| room.room_id != unjoined.id)
    );
    service
        .set_channel_blocked(user.id, unjoined.id, false)
        .await
        .unwrap();
    assert!(
        service
            .list_discover_rooms(user.id)
            .await
            .unwrap()
            .iter()
            .any(|room| room.room_id == unjoined.id)
    );
    assert!(
        !ChatRoomMember::is_member(&client, unjoined.id, user.id)
            .await
            .unwrap()
    );
}

#[test]
fn contains_link_catches_schemes_www_and_bare_domains() {
    for spam in [
        "click https://evil.example/win",
        "HTTP://EVIL.io free chips",
        "go to www.evil.io now",
        "buy at evil.io/now",
        "join evil.gg or evil.xyz",
        "dm me on telegram t.me/scammer",
    ] {
        assert!(contains_link(spam), "should flag: {spam}");
    }
    for clean in [
        "hello there, how are you?",
        "i finished 2048 and got a high score",
        "see you at 3pm. thanks!",
        "node.js is fine to mention",
        "e.g. that idea is good",
    ] {
        assert!(!contains_link(clean), "should not flag: {clean}");
    }
}

#[test]
fn link_cooldown_tiers_by_account_age() {
    let hour = 3_600;
    let day = 24 * hour;
    // Fresh (< 1 day): 30 minutes.
    assert_eq!(link_cooldown_for_age(0), Some(LINK_COOLDOWN_FRESH));
    assert_eq!(link_cooldown_for_age(23 * hour), Some(LINK_COOLDOWN_FRESH));
    // Young (1–7 days): 5 minutes.
    assert_eq!(link_cooldown_for_age(day), Some(LINK_COOLDOWN_YOUNG));
    assert_eq!(link_cooldown_for_age(6 * day), Some(LINK_COOLDOWN_YOUNG));
    // Established (7d+): no cooldown.
    assert_eq!(link_cooldown_for_age(7 * day), None);
    assert_eq!(link_cooldown_for_age(365 * day), None);
}

#[test]
fn send_error_message_explains_report_only_rooms() {
    let bugs = send_error_message(&anyhow::anyhow!("report-only:bugs"));
    assert!(bugs.contains("#bugs"), "{bugs}");
    assert!(bugs.contains("/bug"), "{bugs}");
    let suggestions = send_error_message(&anyhow::anyhow!("report-only:suggestions"));
    assert!(suggestions.contains("#suggestions"), "{suggestions}");
    assert!(suggestions.contains("/suggest"), "{suggestions}");
}

#[test]
fn report_kind_maps_room_slugs() {
    assert_eq!(ReportKind::for_room_slug("bugs"), Some(ReportKind::Bug));
    assert_eq!(
        ReportKind::for_room_slug("suggestions"),
        Some(ReportKind::Suggestion)
    );
    assert_eq!(ReportKind::for_room_slug("lounge"), None);
}

#[test]
fn format_cooldown_is_compact() {
    assert_eq!(format_cooldown(0), "1s");
    assert_eq!(format_cooldown(45), "45s");
    assert_eq!(format_cooldown(60), "1m 00s");
    assert_eq!(format_cooldown(29 * 60 + 30), "29m 30s");
}

fn test_poll(options: Vec<(&str, i64)>) -> ActiveChatPoll {
    let now = Utc::now();
    ActiveChatPoll {
        poll: ChatPoll {
            id: Uuid::from_u128(1),
            created: now,
            updated: now,
            room_id: Uuid::from_u128(2),
            user_id: Uuid::from_u128(3),
            question: "Which editor wins?".to_string(),
            starts_at: now - ChronoDuration::minutes(10),
            ends_at: now,
            active: false,
        },
        options: options
            .into_iter()
            .enumerate()
            .map(|(index, (label, vote_count))| ChatPollOptionSummary {
                id: Uuid::from_u128(10 + index as u128),
                position: (index + 1) as i32,
                label: label.to_string(),
                vote_count,
            })
            .collect(),
        my_vote_option_id: None,
        author_username: Some("polly".to_string()),
    }
}

#[test]
fn poll_results_message_reports_winner_and_percentages() {
    let poll = test_poll(vec![("vim", 4), ("emacs", 3), ("nano", 0)]);

    assert_eq!(
        format_poll_results_message(&poll),
        "---POLL RESULTS---\nWhich editor wins?\n1. vim - 4 votes (57%)\n2. emacs - 3 votes (43%)\n3. nano - 0 votes (0%)\nWinner: vim"
    );
}

#[test]
fn poll_results_message_reports_tie() {
    let poll = test_poll(vec![("vim", 2), ("emacs", 2)]);

    assert_eq!(
        format_poll_results_message(&poll),
        "---POLL RESULTS---\nWhich editor wins?\n1. vim - 2 votes (50%)\n2. emacs - 2 votes (50%)\nTie: vim, emacs"
    );
}

#[test]
fn poll_results_message_reports_no_votes() {
    let poll = test_poll(vec![("vim", 0), ("emacs", 0)]);

    assert_eq!(
        format_poll_results_message(&poll),
        "---POLL RESULTS---\nWhich editor wins?\n1. vim - 0 votes (0%)\n2. emacs - 0 votes (0%)\nWinner: no votes cast"
    );
}
