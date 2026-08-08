use std::collections::VecDeque;

use late_core::models::chat_message::ChatMessage;
use uuid::Uuid;

use super::{
    DmPeer, IrcCapabilities, apply_cap_request, dm_route_from_peer, nick_from_ban_mask,
    remember_send, should_project_dm_message,
};

fn message(room_id: Uuid, user_id: Uuid, body: &str) -> ChatMessage {
    ChatMessage {
        id: Uuid::new_v4(),
        created: chrono::Utc::now(),
        updated: chrono::Utc::now(),
        reply_to_message_id: None,
        reply_to_user_id: None,
        room_id,
        user_id,
        body: body.to_string(),
    }
}

#[test]
fn ban_mask_accepts_nick_identity_shape() {
    assert_eq!(nick_from_ban_mask("alice!*@*"), Some("alice"));
    assert_eq!(nick_from_ban_mask("Alice_123!*@*"), Some("Alice_123"));
}

#[test]
fn ban_mask_rejects_wildcards_hosts_and_plain_nicks() {
    assert_eq!(nick_from_ban_mask("*!*@*"), None);
    assert_eq!(nick_from_ban_mask("alice!*@example.com"), None);
    assert_eq!(nick_from_ban_mask("alice@host!*@*"), None);
    assert_eq!(nick_from_ban_mask("alice"), None);
}

#[test]
fn cap_request_enables_and_lists_supported_caps() {
    let mut caps = IrcCapabilities::default();

    assert!(apply_cap_request(
        &mut caps,
        "message-tags server-time echo-message"
    ));

    assert!(caps.message_tags);
    assert!(caps.server_time);
    assert!(caps.echo_message);
    assert_eq!(caps.as_list(), "message-tags server-time echo-message");
}

#[test]
fn cap_request_naks_unknown_without_changing_enabled_caps() {
    let mut caps = IrcCapabilities::default();
    assert!(apply_cap_request(&mut caps, "message-tags"));

    assert!(!apply_cap_request(&mut caps, "server-time chathistory"));

    assert!(caps.message_tags);
    assert!(!caps.server_time);
    assert!(!caps.echo_message);
    assert_eq!(caps.as_list(), "message-tags");
}

#[test]
fn cap_request_can_disable_supported_caps() {
    let mut caps = IrcCapabilities::default();
    assert!(apply_cap_request(&mut caps, "message-tags server-time"));

    assert!(apply_cap_request(&mut caps, "-server-time"));

    assert!(caps.message_tags);
    assert!(!caps.server_time);
    assert_eq!(caps.as_list(), "message-tags");
}

#[test]
fn negotiated_echo_projects_own_dm() {
    let user_id = Uuid::new_v4();
    let peer_id = Uuid::new_v4();
    let room_id = Uuid::new_v4();
    let message = message(room_id, user_id, "hello from IRC");
    let mut recent_sends = VecDeque::new();
    remember_send(&mut recent_sends, room_id, &message.body);

    assert!(should_project_dm_message(
        &[user_id, peer_id],
        &message,
        user_id,
        false,
        true,
        &mut recent_sends,
    ));
    assert!(recent_sends.is_empty());
}

#[test]
fn own_dm_without_echo_suppresses_only_the_connection_send() {
    let user_id = Uuid::new_v4();
    let peer_id = Uuid::new_v4();
    let room_id = Uuid::new_v4();
    let message = message(room_id, user_id, "hello from IRC");
    let mut recent_sends = VecDeque::new();
    remember_send(&mut recent_sends, room_id, &message.body);

    assert!(!should_project_dm_message(
        &[user_id, peer_id],
        &message,
        user_id,
        false,
        false,
        &mut recent_sends,
    ));
    assert!(should_project_dm_message(
        &[user_id, peer_id],
        &message,
        user_id,
        false,
        false,
        &mut recent_sends,
    ));
}

#[test]
fn dm_projection_routes_own_echo_to_peer() {
    let user_id = Uuid::new_v4();
    let peer_id = Uuid::new_v4();
    let peer = DmPeer {
        peer_user_id: peer_id,
        peer_nick: "crk".to_string(),
    };

    assert_eq!(
        dm_route_from_peer(user_id, "mevanlc", user_id, &peer),
        Some(("mevanlc".to_string(), "crk".to_string()))
    );
    assert_eq!(
        dm_route_from_peer(user_id, "mevanlc", peer_id, &peer),
        Some(("crk".to_string(), "mevanlc".to_string()))
    );
    assert_eq!(
        dm_route_from_peer(user_id, "mevanlc", Uuid::new_v4(), &peer),
        None
    );
}
