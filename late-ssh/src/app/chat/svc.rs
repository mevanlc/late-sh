use anyhow::Result;
use chrono::{DateTime, Utc};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

use late_core::{
    db::Db,
    models::{
        bonsai::Tree,
        chat_message::{ChatMessage, ChatMessageParams},
        chat_message_reaction::{ChatMessageReaction, ChatMessageReactionSummary},
        chat_room::ChatRoom,
        chat_room_member::ChatRoomMember,
        moderation_audit_log::ModerationAuditLog,
        room_ban::RoomBan,
        server_ban::ServerBan,
        user::User,
    },
};
use serde_json::json;
use tokio::sync::{broadcast, watch};
use tokio_postgres::Client;
use tracing::{Instrument, info_span};

use crate::app::bonsai::state::stage_for;
use crate::authz::{Action, Permissions, TargetTier};
use crate::metrics;
use crate::session::SessionRegistry;

const HISTORY_LIMIT: i64 = 1000;
const DELTA_LIMIT: i64 = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StaffViewScope {
    Admin,
    Moderator,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StaffUserRecord {
    pub user_id: Uuid,
    pub username: String,
    pub is_admin: bool,
    pub is_moderator: bool,
    pub active_server_ban: Option<ActiveBanSummary>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveBanSummary {
    pub reason: String,
    pub actor_user_id: Uuid,
    pub actor_username: Option<String>,
    pub created: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StaffRoomRecord {
    pub room_id: Uuid,
    pub kind: String,
    pub visibility: String,
    pub auto_join: bool,
    pub permanent: bool,
    pub slug: Option<String>,
    pub language_code: Option<String>,
    pub member_count: i64,
    pub active_ban_count: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoomModerationAction {
    Kick,
    Ban,
    Unban,
}

impl RoomModerationAction {
    pub fn verb(self) -> &'static str {
        match self {
            Self::Kick => "kick",
            Self::Ban => "ban",
            Self::Unban => "unban",
        }
    }

    pub fn progress_verb(self) -> &'static str {
        match self {
            Self::Kick => "Kicking",
            Self::Ban => "Banning",
            Self::Unban => "Unbanning",
        }
    }

    pub fn success_verb(self) -> &'static str {
        match self {
            Self::Kick => "Kicked",
            Self::Ban => "Banned",
            Self::Unban => "Unbanned",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdminRoomAction {
    Rename { new_slug: String },
    SetVisibility { visibility: String },
    Delete,
}

impl AdminRoomAction {
    pub fn verb(&self) -> &'static str {
        match self {
            Self::Rename { .. } => "rename",
            Self::SetVisibility { visibility } if visibility == "public" => "make public",
            Self::SetVisibility { .. } => "make private",
            Self::Delete => "delete",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdminUserAction {
    DisconnectAllSessions,
    DisconnectSession {
        session_id: Uuid,
    },
    Ban {
        reason: String,
        expires_at: Option<DateTime<Utc>>,
    },
    Unban,
}

impl AdminUserAction {
    pub const fn verb(&self) -> &'static str {
        match self {
            Self::DisconnectAllSessions => "disconnect",
            Self::DisconnectSession { .. } => "disconnect_session",
            Self::Ban { .. } => "ban",
            Self::Unban => "unban",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RoomModerationResult {
    room_id: Uuid,
    room_slug: String,
    target_user_id: Uuid,
    target_username: String,
    action: RoomModerationAction,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AdminRoomResult {
    room_id: Uuid,
    old_slug: String,
    new_slug: Option<String>,
    visibility: Option<String>,
    deleted: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AdminUserResult {
    target_user_id: Uuid,
    target_username: String,
    action: AdminUserAction,
    disconnected_sessions: usize,
}

#[derive(Clone)]
pub struct ChatService {
    db: Db,
    snapshot_tx: watch::Sender<ChatSnapshot>,
    snapshot_rx: watch::Receiver<ChatSnapshot>,
    evt_tx: broadcast::Sender<ChatEvent>,
    notification_svc: super::notifications::svc::NotificationService,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoverRoomItem {
    pub room_id: Uuid,
    pub slug: String,
    pub member_count: i64,
    pub message_count: i64,
    pub last_message_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Default)]
pub struct ChatSnapshot {
    pub user_id: Option<Uuid>,
    pub chat_rooms: Vec<(ChatRoom, Vec<ChatMessage>)>,
    pub discover_rooms: Vec<DiscoverRoomItem>,
    pub message_reactions: HashMap<Uuid, Vec<ChatMessageReactionSummary>>,
    pub general_room_id: Option<Uuid>,
    pub usernames: HashMap<Uuid, String>,
    pub countries: HashMap<Uuid, String>,
    pub unread_counts: HashMap<Uuid, i64>,
    pub all_usernames: Vec<String>,
    pub bonsai_glyphs: HashMap<Uuid, String>,
    pub ignored_user_ids: Vec<Uuid>,
}

#[derive(Clone, Debug)]
pub enum ChatEvent {
    MessageCreated {
        message: ChatMessage,
        target_user_ids: Option<Vec<Uuid>>,
    },
    MessageEdited {
        message: ChatMessage,
        target_user_ids: Option<Vec<Uuid>>,
    },
    MessageReactionsUpdated {
        room_id: Uuid,
        message_id: Uuid,
        reactions: Vec<ChatMessageReactionSummary>,
        target_user_ids: Option<Vec<Uuid>>,
    },
    SendSucceeded {
        user_id: Uuid,
        request_id: Uuid,
    },
    SendFailed {
        user_id: Uuid,
        request_id: Uuid,
        message: String,
    },
    EditSucceeded {
        user_id: Uuid,
        request_id: Uuid,
    },
    EditFailed {
        user_id: Uuid,
        request_id: Uuid,
        message: String,
    },
    DeltaSynced {
        user_id: Uuid,
        room_id: Uuid,
        messages: Vec<ChatMessage>,
    },
    DmOpened {
        user_id: Uuid,
        room_id: Uuid,
    },
    DmFailed {
        user_id: Uuid,
        message: String,
    },
    RoomJoined {
        user_id: Uuid,
        room_id: Uuid,
        slug: String,
    },
    RoomFailed {
        user_id: Uuid,
        message: String,
    },
    RoomLeft {
        user_id: Uuid,
        slug: String,
    },
    LeaveFailed {
        user_id: Uuid,
        message: String,
    },
    RoomCreated {
        user_id: Uuid,
        room_id: Uuid,
        slug: String,
    },
    RoomCreateFailed {
        user_id: Uuid,
        message: String,
    },
    PermanentRoomCreated {
        user_id: Uuid,
        slug: String,
    },
    PermanentRoomDeleted {
        user_id: Uuid,
        slug: String,
    },
    AdminRoomUpdated {
        actor_user_id: Uuid,
        room_id: Uuid,
        old_slug: String,
        new_slug: Option<String>,
        visibility: Option<String>,
        deleted: bool,
    },
    AdminUserModerated {
        actor_user_id: Uuid,
        target_user_id: Uuid,
        target_username: String,
        action: AdminUserAction,
        disconnected_sessions: usize,
    },
    ModerationFailed {
        user_id: Uuid,
        message: String,
    },
    MessageDeleted {
        user_id: Uuid,
        room_id: Uuid,
        message_id: Uuid,
    },
    DeleteFailed {
        user_id: Uuid,
        message: String,
    },
    IgnoreListUpdated {
        user_id: Uuid,
        ignored_user_ids: Vec<Uuid>,
        message: String,
    },
    RoomMembersListed {
        user_id: Uuid,
        title: String,
        members: Vec<String>,
    },
    PublicRoomsListed {
        user_id: Uuid,
        title: String,
        rooms: Vec<String>,
    },
    StaffUsersListed {
        user_id: Uuid,
        title: String,
        lines: Vec<String>,
    },
    StaffUsersSnapshotUpdated {
        user_id: Uuid,
        users: Vec<StaffUserRecord>,
    },
    StaffRoomsSnapshotUpdated {
        user_id: Uuid,
        rooms: Vec<StaffRoomRecord>,
    },
    StaffRoomsListed {
        user_id: Uuid,
        title: String,
        lines: Vec<String>,
    },
    ModeratorsListed {
        user_id: Uuid,
        title: String,
        lines: Vec<String>,
    },
    InviteSucceeded {
        user_id: Uuid,
        room_id: Uuid,
        room_slug: String,
        username: String,
    },
    IgnoreFailed {
        user_id: Uuid,
        message: String,
    },
    RoomMembersListFailed {
        user_id: Uuid,
        message: String,
    },
    PublicRoomsListFailed {
        user_id: Uuid,
        message: String,
    },
    StaffQueryFailed {
        user_id: Uuid,
        message: String,
    },
    RoomModerated {
        actor_user_id: Uuid,
        target_user_id: Uuid,
        room_id: Uuid,
        room_slug: String,
        target_username: String,
        action: RoomModerationAction,
    },
    InviteFailed {
        user_id: Uuid,
        message: String,
    },
}

impl ChatService {
    pub fn new(db: Db, notification_svc: super::notifications::svc::NotificationService) -> Self {
        let (snapshot_tx, snapshot_rx) = watch::channel(ChatSnapshot::default());
        let (evt_tx, _) = broadcast::channel(512);

        Self {
            db,
            snapshot_tx,
            snapshot_rx,
            evt_tx,
            notification_svc,
        }
    }
    pub fn subscribe_state(&self) -> watch::Receiver<ChatSnapshot> {
        self.snapshot_rx.clone()
    }
    pub fn subscribe_events(&self) -> broadcast::Receiver<ChatEvent> {
        self.evt_tx.subscribe()
    }

    pub fn publish_snapshot(&self, snapshot: ChatSnapshot) -> Result<()> {
        self.snapshot_tx.send(snapshot)?;
        Ok(())
    }

    #[tracing::instrument(skip(self), fields(user_id = %user_id, selected_room_id = ?selected_room_id))]
    async fn list_chat_rooms(&self, user_id: Uuid, selected_room_id: Option<Uuid>) -> Result<()> {
        let client = &self.db.get().await?;
        let rooms = ChatRoom::list_for_user(client, user_id).await?;
        let discover_rooms = self.list_discover_rooms(client, user_id).await?;
        let unread_counts = ChatRoomMember::unread_counts_for_user(client, user_id).await?;
        let favorite_room_ids = User::favorite_room_ids(client, user_id).await?;
        let general_room_id = rooms
            .iter()
            .find(|room| room.kind == "general" && room.slug.as_deref() == Some("general"))
            .map(|room| room.id);
        let active_room_id = selected_room_id
            .filter(|selected| rooms.iter().any(|room| room.id == *selected))
            .or_else(|| rooms.first().map(|room| room.id));

        // Preload the same histories regardless of whether the room is opened
        // from the chat page or surfaced on the dashboard: active room,
        // `#general`, and any currently-joined pinned favorites.
        let joined_ids: HashSet<Uuid> = rooms.iter().map(|room| room.id).collect();
        let mut preload_room_ids = Vec::new();
        let mut seen = HashSet::new();
        let mut push_preload = |room_id: Uuid| {
            if joined_ids.contains(&room_id) && seen.insert(room_id) {
                preload_room_ids.push(room_id);
            }
        };
        if let Some(room_id) = active_room_id {
            push_preload(room_id);
        }
        if let Some(room_id) = general_room_id {
            push_preload(room_id);
        }
        for room_id in favorite_room_ids {
            push_preload(room_id);
        }

        let recent_messages =
            ChatMessage::list_recent_for_rooms(client, &preload_room_ids, HISTORY_LIMIT).await?;
        let message_ids: Vec<Uuid> = recent_messages
            .values()
            .flat_map(|messages| messages.iter().map(|message| message.id))
            .collect();
        let message_reactions =
            ChatMessageReaction::list_summaries_for_messages(client, &message_ids).await?;
        // General stays warm for the dashboard even when another room is
        // selected. Favorites ride in the same preload set so the dashboard
        // quick-switch never depends on a prior manual visit or lucky delta.
        let usernames = User::list_all_username_map(client).await?;
        let countries = User::list_all_country_map(client).await?;
        let mut all_usernames: Vec<String> = usernames.values().cloned().collect();
        all_usernames.sort();
        let ignored_user_ids = User::ignored_user_ids(client, user_id).await?;
        let bonsai_glyphs: HashMap<Uuid, String> = Tree::list_all(client)
            .await?
            .into_iter()
            .filter_map(|t| {
                let glyph = stage_for(t.is_alive, t.growth_points).glyph();
                if glyph.is_empty() {
                    None
                } else {
                    Some((t.user_id, glyph.to_string()))
                }
            })
            .collect();

        let rooms = rooms
            .into_iter()
            .map(|chat| {
                let messages = recent_messages.get(&chat.id).cloned().unwrap_or_default();
                (chat, messages)
            })
            .collect();

        self.publish_snapshot(ChatSnapshot {
            user_id: Some(user_id),
            chat_rooms: rooms,
            discover_rooms,
            message_reactions,
            general_room_id,
            usernames,
            countries,
            unread_counts,
            all_usernames,
            bonsai_glyphs,
            ignored_user_ids,
        })
    }

    async fn list_discover_rooms(
        &self,
        client: &tokio_postgres::Client,
        user_id: Uuid,
    ) -> Result<Vec<DiscoverRoomItem>> {
        let rows = client
            .query(
                "SELECT r.id,
                        r.slug,
                        COUNT(DISTINCT m.user_id)::bigint AS member_count,
                        COUNT(DISTINCT msg.id)::bigint AS message_count,
                        MAX(msg.created) AS last_message_at
                 FROM chat_rooms r
                 LEFT JOIN chat_room_members m ON m.room_id = r.id
                 LEFT JOIN chat_messages msg ON msg.room_id = r.id
                 WHERE r.kind = 'topic'
                   AND r.visibility = 'public'
                   AND r.permanent = false
                   AND NOT EXISTS (
                       SELECT 1
                       FROM chat_room_members self_member
                       WHERE self_member.room_id = r.id
                         AND self_member.user_id = $1
                   )
                 GROUP BY r.id, r.slug
                 ORDER BY
                    COALESCE(MAX(msg.created), r.created) DESC,
                    message_count DESC,
                    member_count DESC,
                    r.slug ASC",
                &[&user_id],
            )
            .await?;

        Ok(rows
            .into_iter()
            .filter_map(|row| {
                let slug: Option<String> = row.get("slug");
                slug.map(|slug| DiscoverRoomItem {
                    room_id: row.get("id"),
                    slug,
                    member_count: row.get("member_count"),
                    message_count: row.get("message_count"),
                    last_message_at: row.get("last_message_at"),
                })
            })
            .collect())
    }

    pub fn start_user_refresh_task(
        &self,
        user_id: Uuid,
        room_rx: watch::Receiver<Option<Uuid>>,
    ) -> tokio::task::AbortHandle {
        let service = self.clone();
        let handle = tokio::spawn(
            async move {
                loop {
                    let room_id = *room_rx.borrow();
                    if let Err(e) = service.list_chat_rooms(user_id, room_id).await {
                        late_core::error_span!(
                            "chat_refresh_failed",
                            error = ?e,
                            "chat service refresh failed"
                        );
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                }
            }
            .instrument(info_span!("chat.refresh_loop", user_id = %user_id)),
        );
        handle.abort_handle()
    }

    pub fn list_chats_task(&self, user_id: Uuid, selected_room_id: Option<Uuid>) {
        let service = self.clone();
        tokio::spawn(
            async move {
                if let Err(e) = service.list_chat_rooms(user_id, selected_room_id).await {
                    late_core::error_span!("chat_list_failed", error = ?e, "failed to list chats");
                }
            }
            .instrument(info_span!(
                "chat.list_task",
                user_id = %user_id,
                selected_room_id = ?selected_room_id
            )),
        );
    }

    #[tracing::instrument(skip(self), fields(user_id = %user_id))]
    pub async fn auto_join_public_rooms(&self, user_id: Uuid) -> Result<u64> {
        let client = self.db.get().await?;
        let joined = ChatRoomMember::auto_join_public_rooms(&client, user_id).await?;
        Ok(joined)
    }

    async fn ensure_user_not_banned_from_room(
        &self,
        client: &Client,
        room_id: Uuid,
        user_id: Uuid,
    ) -> Result<()> {
        if RoomBan::is_active_for_room_and_user(client, room_id, user_id).await? {
            anyhow::bail!("You are banned from this room");
        }
        Ok(())
    }

    #[tracing::instrument(skip(self), fields(user_id = %user_id, room_id = %room_id))]
    async fn mark_room_read(&self, user_id: Uuid, room_id: Uuid) -> Result<()> {
        let client = &self.db.get().await?;
        let is_member = ChatRoomMember::is_member(client, room_id, user_id).await?;
        if !is_member {
            anyhow::bail!("user is not a member of room");
        }
        ChatRoomMember::mark_read_now(client, room_id, user_id).await?;
        Ok(())
    }

    pub fn mark_room_read_task(&self, user_id: Uuid, room_id: Uuid) {
        let service = self.clone();
        tokio::spawn(
            async move {
                if let Err(e) = service.mark_room_read(user_id, room_id).await {
                    late_core::error_span!(
                        "chat_mark_read_failed",
                        error = ?e,
                        "failed to mark room read"
                    );
                }
            }
            .instrument(info_span!(
                "chat.mark_room_read_task",
                user_id = %user_id,
                room_id = %room_id
            )),
        );
    }

    #[tracing::instrument(skip(self), fields(user_id = %user_id, room_id = %room_id, after_created = %after_created, after_id = %after_id))]
    async fn sync_room_after(
        &self,
        user_id: Uuid,
        room_id: Uuid,
        after_created: DateTime<Utc>,
        after_id: Uuid,
    ) -> Result<()> {
        let client = &self.db.get().await?;
        let is_member = ChatRoomMember::is_member(client, room_id, user_id).await?;
        if !is_member {
            anyhow::bail!("user is not a member of room");
        }

        let messages =
            ChatMessage::list_after(client, room_id, after_created, after_id, DELTA_LIMIT).await?;
        if !messages.is_empty() {
            let _ = self.evt_tx.send(ChatEvent::DeltaSynced {
                user_id,
                room_id,
                messages,
            });
        }
        Ok(())
    }

    pub fn sync_room_after_task(
        &self,
        user_id: Uuid,
        room_id: Uuid,
        after_created: DateTime<Utc>,
        after_id: Uuid,
    ) {
        let service = self.clone();
        tokio::spawn(
            async move {
                if let Err(e) = service
                    .sync_room_after(user_id, room_id, after_created, after_id)
                    .await
                {
                    late_core::error_span!(
                        "chat_sync_failed",
                        error = ?e,
                        "failed to sync chat room delta"
                    );
                }
            }
            .instrument(info_span!(
                "chat.sync_room_after_task",
                user_id = %user_id,
                room_id = %room_id,
                after_created = %after_created,
                after_id = %after_id
            )),
        );
    }

    pub fn send_message_task(
        &self,
        user_id: Uuid,
        room_id: Uuid,
        room_slug: Option<String>,
        body: String,
        request_id: Uuid,
        permissions: Permissions,
    ) {
        let service = self.clone();
        tokio::spawn(
            async move {
                match service
                    .send_message(user_id, room_id, room_slug, body, permissions)
                    .await
                {
                    Err(e) => {
                        let message = if e.to_string().contains("not a member") {
                            "You are not a member of this room."
                        } else if e.to_string().contains("banned from this room") {
                            "You are banned from this room."
                        } else if e.to_string().contains("admin-only") {
                            "Only admins can post in #announcements."
                        } else {
                            "Could not send message. Please try again."
                        };
                        let _ = service.evt_tx.send(ChatEvent::SendFailed {
                            user_id,
                            request_id,
                            message: message.to_string(),
                        });
                        late_core::error_span!(
                            "chat_send_failed",
                            error = ?e,
                            "failed to send message"
                        );
                    }
                    Ok(()) => {
                        let _ = service.evt_tx.send(ChatEvent::SendSucceeded {
                            user_id,
                            request_id,
                        });
                    }
                }
            }
            .instrument(info_span!(
                "chat.send_message_task",
                user_id = %user_id,
                room_id = %room_id,
                request_id = %request_id
            )),
        );
    }

    #[tracing::instrument(skip(self, body), fields(user_id = %user_id, room_id = %room_id, body_len = body.len()))]
    async fn send_message(
        &self,
        user_id: Uuid,
        room_id: Uuid,
        room_slug: Option<String>,
        body: String,
        permissions: Permissions,
    ) -> Result<()> {
        let body = body.trim_start_matches('\n').trim_end();
        if body.is_empty() {
            return Ok(());
        }

        if room_slug.as_deref() == Some("announcements") && !permissions.can_post_announcements() {
            anyhow::bail!("announcements is admin-only");
        }

        let client = &self.db.get().await?;
        self.ensure_user_not_banned_from_room(client, room_id, user_id)
            .await?;
        let is_member = ChatRoomMember::is_member(client, room_id, user_id).await?;
        if !is_member {
            anyhow::bail!("user is not a member of room");
        }

        let message = ChatMessageParams {
            room_id,
            user_id,
            body: body.to_string(),
        };
        let chat = ChatMessage::create(client, message).await?;
        ChatRoom::touch_updated(client, room_id).await?;
        ChatRoomMember::mark_read_now(client, room_id, user_id).await?;
        let target_user_ids = ChatRoom::get_target_user_ids(client, room_id).await?;
        let _ = self.evt_tx.send(ChatEvent::MessageCreated {
            message: chat.clone(),
            target_user_ids,
        });
        metrics::record_chat_message_sent();
        self.notification_svc
            .create_mentions_task(user_id, chat.id, room_id, body.to_string());
        tracing::info!(chat_id = %chat.id, "message sent");
        Ok(())
    }

    pub fn edit_message_task(
        &self,
        user_id: Uuid,
        message_id: Uuid,
        new_body: String,
        request_id: Uuid,
        permissions: Permissions,
    ) {
        let service = self.clone();
        tokio::spawn(
            async move {
                match service
                    .edit_message(user_id, message_id, new_body, permissions)
                    .await
                {
                    Err(e) => {
                        let message = if e.to_string().contains("Cannot edit") {
                            "You can only edit your own messages."
                        } else if e.to_string().contains("empty") {
                            "Edited message cannot be empty."
                        } else {
                            "Could not edit message. Please try again."
                        };
                        let _ = service.evt_tx.send(ChatEvent::EditFailed {
                            user_id,
                            request_id,
                            message: message.to_string(),
                        });
                    }
                    Ok(()) => {
                        let _ = service.evt_tx.send(ChatEvent::EditSucceeded {
                            user_id,
                            request_id,
                        });
                    }
                }
            }
            .instrument(info_span!(
                "chat.edit_message_task",
                user_id = %user_id,
                message_id = %message_id,
                request_id = %request_id
            )),
        );
    }

    #[tracing::instrument(skip(self, new_body), fields(user_id = %user_id, message_id = %message_id, body_len = new_body.len()))]
    async fn edit_message(
        &self,
        user_id: Uuid,
        message_id: Uuid,
        new_body: String,
        permissions: Permissions,
    ) -> Result<()> {
        let new_body = new_body.trim_start_matches('\n').trim_end();
        if new_body.is_empty() {
            anyhow::bail!("edited body is empty");
        }

        let client = &self.db.get().await?;
        let existing = ChatMessage::get(client, message_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("message not found"))?;
        let target_tier = if existing.user_id == user_id {
            TargetTier::Own
        } else {
            let target = User::get(client, existing.user_id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("target user not found"))?;
            TargetTier::from_user_flags(target.is_admin, target.is_moderator)
        };
        if !permissions
            .decide(Action::EditMessage, target_tier)
            .is_allowed()
        {
            anyhow::bail!("cannot edit this message");
        }

        let params = ChatMessageParams {
            room_id: existing.room_id,
            user_id: existing.user_id,
            body: new_body.to_string(),
        };
        let updated = ChatMessage::update(client, message_id, params).await?;
        let target_user_ids = ChatRoom::get_target_user_ids(client, existing.room_id).await?;
        let _ = self.evt_tx.send(ChatEvent::MessageEdited {
            message: updated,
            target_user_ids,
        });
        metrics::record_chat_message_edited();
        Ok(())
    }

    pub fn toggle_message_reaction_task(&self, user_id: Uuid, message_id: Uuid, kind: i16) {
        let service = self.clone();
        tokio::spawn(
            async move {
                if let Err(e) = service
                    .toggle_message_reaction(user_id, message_id, kind)
                    .await
                {
                    late_core::error_span!(
                        "chat_toggle_reaction_failed",
                        error = ?e,
                        "failed to toggle message reaction"
                    );
                }
            }
            .instrument(info_span!(
                "chat.toggle_message_reaction_task",
                user_id = %user_id,
                message_id = %message_id,
                kind = kind
            )),
        );
    }

    #[tracing::instrument(skip(self), fields(user_id = %user_id, message_id = %message_id, kind = kind))]
    async fn toggle_message_reaction(
        &self,
        user_id: Uuid,
        message_id: Uuid,
        kind: i16,
    ) -> Result<()> {
        let client = &self.db.get().await?;
        let message = ChatMessage::get(client, message_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("message not found"))?;
        let is_member = ChatRoomMember::is_member(client, message.room_id, user_id).await?;
        if !is_member {
            anyhow::bail!("user is not a member of room");
        }

        ChatMessageReaction::toggle(client, message_id, user_id, kind).await?;
        let reactions = ChatMessageReaction::list_summaries_for_messages(client, &[message_id])
            .await?
            .remove(&message_id)
            .unwrap_or_default();
        let target_user_ids = ChatRoom::get_target_user_ids(client, message.room_id).await?;
        let _ = self.evt_tx.send(ChatEvent::MessageReactionsUpdated {
            room_id: message.room_id,
            message_id,
            reactions,
            target_user_ids,
        });
        Ok(())
    }

    pub fn start_dm_task(&self, user_id: Uuid, target_username: String) {
        let service = self.clone();
        let span = info_span!("chat.start_dm_task", user_id = %user_id, target = %target_username);
        tokio::spawn(
            async move {
                match service.open_dm(user_id, &target_username).await {
                    Ok(room_id) => {
                        let _ = service
                            .evt_tx
                            .send(ChatEvent::DmOpened { user_id, room_id });
                    }
                    Err(e) => {
                        let _ = service.evt_tx.send(ChatEvent::DmFailed {
                            user_id,
                            message: e.to_string(),
                        });
                    }
                }
            }
            .instrument(span),
        );
    }

    async fn open_dm(&self, user_id: Uuid, target_username: &str) -> Result<Uuid> {
        let client = &self.db.get().await?;
        let target = User::find_by_username(client, target_username)
            .await?
            .ok_or_else(|| anyhow::anyhow!("User '{}' not found", target_username))?;
        if target.id == user_id {
            anyhow::bail!("Cannot DM yourself");
        }
        let room = ChatRoom::get_or_create_dm(client, user_id, target.id).await?;
        ChatRoomMember::join(client, room.id, user_id).await?;
        ChatRoomMember::join(client, room.id, target.id).await?;
        Ok(room.id)
    }

    pub fn list_room_members_task(&self, user_id: Uuid, room_id: Uuid) {
        let service = self.clone();
        let span = info_span!(
            "chat.list_room_members_task",
            user_id = %user_id,
            room_id = %room_id
        );
        tokio::spawn(
            async move {
                let event = match service.list_room_members(user_id, room_id).await {
                    Ok((title, members)) => ChatEvent::RoomMembersListed {
                        user_id,
                        title,
                        members,
                    },
                    Err(e) => ChatEvent::RoomMembersListFailed {
                        user_id,
                        message: e.to_string(),
                    },
                };
                let _ = service.evt_tx.send(event);
            }
            .instrument(span),
        );
    }

    async fn list_room_members(
        &self,
        user_id: Uuid,
        room_id: Uuid,
    ) -> Result<(String, Vec<String>)> {
        let client = &self.db.get().await?;
        let room = ChatRoom::get(client, room_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Room not found"))?;
        let is_member = ChatRoomMember::is_member(client, room_id, user_id).await?;
        if !is_member {
            anyhow::bail!("You are not a member of this room");
        }

        let user_ids = ChatRoomMember::list_user_ids(client, room_id).await?;
        let usernames = User::list_usernames_by_ids(client, &user_ids).await?;
        let members = user_ids
            .into_iter()
            .map(|id| {
                usernames
                    .get(&id)
                    .map(|username| format!("@{username}"))
                    .unwrap_or_else(|| format!("@<unknown:{}>", short_user_id(id)))
            })
            .collect();
        let title = if room.kind == "dm" {
            "DM Members".to_string()
        } else {
            room.slug
                .as_deref()
                .map(|slug| format!("#{slug} Members"))
                .unwrap_or_else(|| "Room Members".to_string())
        };

        Ok((title, members))
    }

    pub fn list_public_rooms_task(&self, user_id: Uuid) {
        let service = self.clone();
        let span = info_span!("chat.list_public_rooms_task", user_id = %user_id);
        tokio::spawn(
            async move {
                let event = match service.list_public_rooms().await {
                    Ok((title, rooms)) => ChatEvent::PublicRoomsListed {
                        user_id,
                        title,
                        rooms,
                    },
                    Err(e) => ChatEvent::PublicRoomsListFailed {
                        user_id,
                        message: e.to_string(),
                    },
                };
                let _ = service.evt_tx.send(event);
            }
            .instrument(span),
        );
    }

    async fn list_public_rooms(&self) -> Result<(String, Vec<String>)> {
        let client = &self.db.get().await?;
        let rows = client
            .query(
                "SELECT r.kind,
                        r.slug,
                        r.language_code,
                        COUNT(m.user_id)::bigint AS member_count
                 FROM chat_rooms r
                 LEFT JOIN chat_room_members m ON m.room_id = r.id
                 WHERE r.kind = 'topic'
                   AND r.visibility = 'public'
                   AND r.permanent = false
                 GROUP BY r.id, r.kind, r.slug, r.language_code, r.created
                 ORDER BY
                    member_count DESC,
                    COALESCE(r.slug, COALESCE(r.language_code, '')) ASC,
                    r.created ASC,
                    r.id ASC",
                &[],
            )
            .await?;

        let rooms: Vec<String> = rows
            .into_iter()
            .map(|row| {
                let kind: String = row.get("kind");
                let slug: Option<String> = row.get("slug");
                let language_code: Option<String> = row.get("language_code");
                let member_count: i64 = row.get("member_count");
                let label = slug
                    .map(|slug| format!("#{slug}"))
                    .or_else(|| language_code.map(|code| format!("language:{code}")))
                    .unwrap_or(kind);
                let noun = if member_count == 1 {
                    "member"
                } else {
                    "members"
                };
                format!("{label} ({member_count} {noun})")
            })
            .collect();
        let rooms = if rooms.is_empty() {
            vec!["No public rooms".to_string()]
        } else {
            rooms
        };

        Ok(("Public Rooms".to_string(), rooms))
    }

    pub fn list_staff_users_task(
        &self,
        user_id: Uuid,
        permissions: Permissions,
        scope: StaffViewScope,
    ) {
        let service = self.clone();
        let span = info_span!(
            "chat.list_staff_users_task",
            user_id = %user_id,
            scope = ?scope
        );
        tokio::spawn(
            async move {
                let event = match service.list_staff_users(permissions, scope).await {
                    Ok((title, lines)) => ChatEvent::StaffUsersListed {
                        user_id,
                        title,
                        lines,
                    },
                    Err(e) => ChatEvent::StaffQueryFailed {
                        user_id,
                        message: e.to_string(),
                    },
                };
                let _ = service.evt_tx.send(event);
            }
            .instrument(span),
        );
    }

    pub fn refresh_staff_users_snapshot_task(
        &self,
        user_id: Uuid,
        permissions: Permissions,
        scope: StaffViewScope,
    ) {
        let service = self.clone();
        let span = info_span!(
            "chat.refresh_staff_users_snapshot_task",
            user_id = %user_id,
            scope = ?scope
        );
        tokio::spawn(
            async move {
                let event = match service.list_staff_users_data(permissions, scope).await {
                    Ok(users) => ChatEvent::StaffUsersSnapshotUpdated { user_id, users },
                    Err(e) => ChatEvent::StaffQueryFailed {
                        user_id,
                        message: e.to_string(),
                    },
                };
                let _ = service.evt_tx.send(event);
            }
            .instrument(span),
        );
    }

    async fn list_staff_users(
        &self,
        permissions: Permissions,
        scope: StaffViewScope,
    ) -> Result<(String, Vec<String>)> {
        let users = self.list_staff_users_data(permissions, scope).await?;

        let mut lines = vec![format!("All Users ({})", users.len())];
        if users.is_empty() {
            lines.push("No users".to_string());
        } else {
            lines.extend(users.into_iter().map(|user| {
                let username = if user.username.trim().is_empty() {
                    "<unnamed>".to_string()
                } else {
                    format!("@{}", user.username)
                };
                let mut flags = Vec::new();
                if user.is_admin {
                    flags.push("admin");
                }
                if user.is_moderator {
                    flags.push("mod");
                }
                if flags.is_empty() {
                    username
                } else {
                    format!("{username} [{}]", flags.join(", "))
                }
            }));
        }

        Ok((staff_users_title(scope), lines))
    }

    async fn list_staff_users_data(
        &self,
        permissions: Permissions,
        scope: StaffViewScope,
    ) -> Result<Vec<StaffUserRecord>> {
        ensure_staff_scope(permissions, scope)?;

        let client = &self.db.get().await?;
        let mut users = User::all(client).await?;
        users.sort_by(|a, b| {
            a.username
                .to_ascii_lowercase()
                .cmp(&b.username.to_ascii_lowercase())
                .then_with(|| a.created.cmp(&b.created))
        });

        let active_bans = ServerBan::active_with_actor_username(client).await?;
        let mut active_bans_by_user: HashMap<Uuid, ActiveBanSummary> = HashMap::new();
        for (ban, actor_username) in active_bans {
            let Some(target_user_id) = ban.target_user_id else {
                continue;
            };
            active_bans_by_user
                .entry(target_user_id)
                .or_insert(ActiveBanSummary {
                    reason: ban.reason,
                    actor_user_id: ban.actor_user_id,
                    actor_username,
                    created: ban.created,
                    expires_at: ban.expires_at,
                });
        }

        Ok(users
            .into_iter()
            .map(|user| StaffUserRecord {
                user_id: user.id,
                username: user.username,
                is_admin: user.is_admin,
                is_moderator: user.is_moderator,
                active_server_ban: active_bans_by_user.remove(&user.id),
            })
            .collect())
    }

    pub fn list_staff_rooms_task(
        &self,
        user_id: Uuid,
        permissions: Permissions,
        scope: StaffViewScope,
    ) {
        let service = self.clone();
        let span = info_span!(
            "chat.list_staff_rooms_task",
            user_id = %user_id,
            scope = ?scope
        );
        tokio::spawn(
            async move {
                let event = match service.list_staff_rooms(permissions, scope).await {
                    Ok((title, lines)) => ChatEvent::StaffRoomsListed {
                        user_id,
                        title,
                        lines,
                    },
                    Err(e) => ChatEvent::StaffQueryFailed {
                        user_id,
                        message: e.to_string(),
                    },
                };
                let _ = service.evt_tx.send(event);
            }
            .instrument(span),
        );
    }

    pub fn refresh_staff_rooms_snapshot_task(
        &self,
        user_id: Uuid,
        permissions: Permissions,
        scope: StaffViewScope,
    ) {
        let service = self.clone();
        let span = info_span!(
            "chat.refresh_staff_rooms_snapshot_task",
            user_id = %user_id,
            scope = ?scope
        );
        tokio::spawn(
            async move {
                let event = match service.list_staff_rooms_data(permissions, scope).await {
                    Ok(rooms) => ChatEvent::StaffRoomsSnapshotUpdated { user_id, rooms },
                    Err(e) => ChatEvent::StaffQueryFailed {
                        user_id,
                        message: e.to_string(),
                    },
                };
                let _ = service.evt_tx.send(event);
            }
            .instrument(span),
        );
    }

    async fn list_staff_rooms(
        &self,
        permissions: Permissions,
        scope: StaffViewScope,
    ) -> Result<(String, Vec<String>)> {
        let rooms = self.list_staff_rooms_data(permissions, scope).await?;

        let mut lines = vec![format!("All Rooms ({})", rooms.len())];
        if rooms.is_empty() {
            lines.push("No rooms".to_string());
        } else {
            lines.extend(rooms.into_iter().map(|room| {
                let label = staff_room_label(&room);
                let mut details = vec![
                    room.kind,
                    room.visibility,
                    format!(
                        "{} {}",
                        room.member_count,
                        if room.member_count == 1 {
                            "member"
                        } else {
                            "members"
                        }
                    ),
                ];
                if room.permanent {
                    details.push("permanent".to_string());
                }
                if room.auto_join {
                    details.push("auto-join".to_string());
                }
                if room.active_ban_count > 0 {
                    details.push(format!("{} banned", room.active_ban_count));
                }

                format!("{label} · {}", details.join(" · "))
            }));
        }

        Ok((staff_rooms_title(scope), lines))
    }

    async fn list_staff_rooms_data(
        &self,
        permissions: Permissions,
        scope: StaffViewScope,
    ) -> Result<Vec<StaffRoomRecord>> {
        ensure_staff_scope(permissions, scope)?;

        let client = &self.db.get().await?;
        let rows = client
            .query(
                "SELECT r.id,
                        r.kind,
                        r.visibility,
                        r.auto_join,
                        r.permanent,
                        r.slug,
                        r.language_code,
                        COUNT(DISTINCT m.user_id)::bigint AS member_count,
                        COALESCE(rb.active_ban_count, 0)::bigint AS active_ban_count
                 FROM chat_rooms r
                 LEFT JOIN chat_room_members m ON m.room_id = r.id
                 LEFT JOIN (
                     SELECT room_id, COUNT(*)::bigint AS active_ban_count
                     FROM room_bans
                     WHERE expires_at IS NULL OR expires_at > current_timestamp
                     GROUP BY room_id
                 ) rb ON rb.room_id = r.id
                 GROUP BY
                     r.id,
                     r.kind,
                     r.visibility,
                     r.auto_join,
                     r.permanent,
                     r.slug,
                     r.language_code,
                     r.created,
                     rb.active_ban_count
                 ORDER BY
                     CASE
                         WHEN r.kind = 'general' AND r.slug = 'general' THEN 0
                         WHEN r.permanent THEN 1
                         WHEN r.visibility = 'public' THEN 2
                         WHEN r.kind = 'dm' THEN 4
                         ELSE 3
                     END ASC,
                     COALESCE(r.slug, COALESCE(r.language_code, '')) ASC,
                     r.created ASC,
                     r.id ASC",
                &[],
            )
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| StaffRoomRecord {
                room_id: row.get("id"),
                kind: row.get("kind"),
                visibility: row.get("visibility"),
                auto_join: row.get("auto_join"),
                permanent: row.get("permanent"),
                slug: row.get("slug"),
                language_code: row.get("language_code"),
                member_count: row.get("member_count"),
                active_ban_count: row.get("active_ban_count"),
            })
            .collect())
    }

    pub fn list_moderators_task(&self, user_id: Uuid, permissions: Permissions) {
        let service = self.clone();
        let span = info_span!("chat.list_moderators_task", user_id = %user_id);
        tokio::spawn(
            async move {
                let event = match service.list_moderators(permissions).await {
                    Ok((title, lines)) => ChatEvent::ModeratorsListed {
                        user_id,
                        title,
                        lines,
                    },
                    Err(e) => ChatEvent::StaffQueryFailed {
                        user_id,
                        message: e.to_string(),
                    },
                };
                let _ = service.evt_tx.send(event);
            }
            .instrument(span),
        );
    }

    async fn list_moderators(&self, permissions: Permissions) -> Result<(String, Vec<String>)> {
        if !permissions.can_access_admin_surface() {
            anyhow::bail!("Admin only");
        }

        let client = &self.db.get().await?;
        let mut users = User::all(client).await?;
        users.retain(|user| user.is_admin || user.is_moderator);
        users.sort_by(|a, b| {
            a.username
                .to_ascii_lowercase()
                .cmp(&b.username.to_ascii_lowercase())
        });

        let mut lines = vec![format!("Staff ({})", users.len())];
        if users.is_empty() {
            lines.push("No moderators".to_string());
        } else {
            lines.extend(users.into_iter().map(|user| {
                let username = if user.username.trim().is_empty() {
                    "<unnamed>".to_string()
                } else {
                    format!("@{}", user.username)
                };
                let mut flags = Vec::new();
                if user.is_admin {
                    flags.push("admin");
                }
                if user.is_moderator {
                    flags.push("mod");
                }
                format!("{username} [{}]", flags.join(", "))
            }));
        }

        Ok(("Admin Mods".to_string(), lines))
    }

    pub fn ignore_user_task(&self, user_id: Uuid, target_username: String) {
        let service = self.clone();
        let span =
            info_span!("chat.ignore_user_task", user_id = %user_id, target = %target_username);
        tokio::spawn(
            async move {
                let event = match service.ignore_user(user_id, &target_username).await {
                    Ok((ignored_user_ids, message)) => ChatEvent::IgnoreListUpdated {
                        user_id,
                        ignored_user_ids,
                        message,
                    },
                    Err(e) => ChatEvent::IgnoreFailed {
                        user_id,
                        message: e.to_string(),
                    },
                };
                let _ = service.evt_tx.send(event);
            }
            .instrument(span),
        );
    }

    async fn ignore_user(
        &self,
        user_id: Uuid,
        target_username: &str,
    ) -> Result<(Vec<Uuid>, String)> {
        let client = &self.db.get().await?;
        let target = User::find_by_username(client, target_username)
            .await?
            .ok_or_else(|| anyhow::anyhow!("User '{}' not found", target_username))?;
        if target.id == user_id {
            anyhow::bail!("Cannot ignore yourself");
        }
        let (changed, ids) = User::add_ignored_user_id(client, user_id, target.id).await?;
        if !changed {
            anyhow::bail!("@{} is already ignored", target.username);
        }
        Ok((ids, format!("Ignored @{}", target.username)))
    }

    pub fn unignore_user_task(&self, user_id: Uuid, target_username: String) {
        let service = self.clone();
        let span =
            info_span!("chat.unignore_user_task", user_id = %user_id, target = %target_username);
        tokio::spawn(
            async move {
                let event = match service.unignore_user(user_id, &target_username).await {
                    Ok((ignored_user_ids, message)) => ChatEvent::IgnoreListUpdated {
                        user_id,
                        ignored_user_ids,
                        message,
                    },
                    Err(e) => ChatEvent::IgnoreFailed {
                        user_id,
                        message: e.to_string(),
                    },
                };
                let _ = service.evt_tx.send(event);
            }
            .instrument(span),
        );
    }

    async fn unignore_user(
        &self,
        user_id: Uuid,
        target_username: &str,
    ) -> Result<(Vec<Uuid>, String)> {
        let client = &self.db.get().await?;
        let target = User::find_by_username(client, target_username)
            .await?
            .ok_or_else(|| anyhow::anyhow!("User '{}' not found", target_username))?;
        if target.id == user_id {
            anyhow::bail!("Cannot unignore yourself");
        }
        let (changed, ids) = User::remove_ignored_user_id(client, user_id, target.id).await?;
        if !changed {
            anyhow::bail!("@{} is not ignored", target.username);
        }
        Ok((ids, format!("Unignored @{}", target.username)))
    }

    pub fn open_public_room_task(&self, user_id: Uuid, slug: String) {
        let service = self.clone();
        let span = info_span!("chat.open_public_room_task", user_id = %user_id, slug = %slug);
        tokio::spawn(
            async move {
                match service.open_public_room(user_id, &slug).await {
                    Ok(room_id) => {
                        let _ = service.evt_tx.send(ChatEvent::RoomJoined {
                            user_id,
                            room_id,
                            slug,
                        });
                    }
                    Err(e) => {
                        let _ = service.evt_tx.send(ChatEvent::RoomFailed {
                            user_id,
                            message: e.to_string(),
                        });
                    }
                }
            }
            .instrument(span),
        );
    }

    pub fn join_public_room_task(&self, user_id: Uuid, room_id: Uuid, slug: String) {
        let service = self.clone();
        let span = info_span!("chat.join_public_room_task", user_id = %user_id, room_id = %room_id, slug = %slug);
        tokio::spawn(
            async move {
                match service.join_public_room(user_id, room_id).await {
                    Ok(room_id) => {
                        let _ = service.evt_tx.send(ChatEvent::RoomJoined {
                            user_id,
                            room_id,
                            slug,
                        });
                    }
                    Err(e) => {
                        let _ = service.evt_tx.send(ChatEvent::RoomFailed {
                            user_id,
                            message: e.to_string(),
                        });
                    }
                }
            }
            .instrument(span),
        );
    }

    async fn join_public_room(&self, user_id: Uuid, room_id: Uuid) -> Result<Uuid> {
        let client = &self.db.get().await?;
        let room = ChatRoom::get(client, room_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Room not found"))?;
        if room.kind != "topic" || room.visibility != "public" {
            anyhow::bail!("Only public rooms can be joined from discover");
        }
        self.ensure_user_not_banned_from_room(client, room.id, user_id)
            .await?;
        ChatRoomMember::join(client, room.id, user_id).await?;
        Ok(room.id)
    }

    async fn open_public_room(&self, user_id: Uuid, slug: &str) -> Result<Uuid> {
        let client = &self.db.get().await?;
        let room = ChatRoom::get_or_create_public_room(client, slug).await?;
        self.ensure_user_not_banned_from_room(client, room.id, user_id)
            .await?;
        ChatRoomMember::join(client, room.id, user_id).await?;
        Ok(room.id)
    }

    pub fn create_private_room_task(&self, user_id: Uuid, slug: String) {
        let service = self.clone();
        let span = info_span!("chat.create_private_room_task", user_id = %user_id, slug = %slug);
        tokio::spawn(
            async move {
                match service.create_private_room(user_id, &slug).await {
                    Ok(room_id) => {
                        let _ = service.evt_tx.send(ChatEvent::RoomCreated {
                            user_id,
                            room_id,
                            slug,
                        });
                    }
                    Err(e) => {
                        let _ = service.evt_tx.send(ChatEvent::RoomCreateFailed {
                            user_id,
                            message: e.to_string(),
                        });
                    }
                }
            }
            .instrument(span),
        );
    }

    async fn create_private_room(&self, user_id: Uuid, slug: &str) -> Result<Uuid> {
        let client = &self.db.get().await?;
        let room = ChatRoom::create_private_room(client, slug).await?;
        ChatRoomMember::join(client, room.id, user_id).await?;
        Ok(room.id)
    }

    pub fn leave_room_task(&self, user_id: Uuid, room_id: Uuid, slug: String) {
        let service = self.clone();
        let span = info_span!("chat.leave_room_task", user_id = %user_id, slug = %slug);
        tokio::spawn(
            async move {
                match service.leave_room(user_id, room_id).await {
                    Ok(()) => {
                        let _ = service.evt_tx.send(ChatEvent::RoomLeft { user_id, slug });
                    }
                    Err(e) => {
                        let _ = service.evt_tx.send(ChatEvent::LeaveFailed {
                            user_id,
                            message: e.to_string(),
                        });
                    }
                }
            }
            .instrument(span),
        );
    }

    async fn leave_room(&self, user_id: Uuid, room_id: Uuid) -> Result<()> {
        let client = &self.db.get().await?;
        let room = ChatRoom::get(client, room_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Room not found"))?;
        if room.permanent {
            let name = room.slug.as_deref().unwrap_or("this room");
            anyhow::bail!("Cannot leave #{name} (permanent room)");
        }
        ChatRoomMember::leave(client, room_id, user_id).await?;
        Ok(())
    }

    pub fn create_room_task(&self, user_id: Uuid, slug: String) {
        let service = self.clone();
        let span = info_span!("chat.create_room", user_id = %user_id, slug = %slug);
        tokio::spawn(
            async move {
                match service.create_room(&slug).await {
                    Ok(room_id) => {
                        let _ = service.evt_tx.send(ChatEvent::RoomCreated {
                            user_id,
                            room_id,
                            slug,
                        });
                    }
                    Err(e) => {
                        let _ = service.evt_tx.send(ChatEvent::RoomCreateFailed {
                            user_id,
                            message: e.to_string(),
                        });
                    }
                }
            }
            .instrument(span),
        );
    }

    async fn create_room(&self, slug: &str) -> Result<Uuid> {
        let client = &self.db.get().await?;
        let room = ChatRoom::ensure_auto_join(client, slug).await?;
        let added = ChatRoom::add_all_users(client, room.id).await?;
        tracing::info!(slug = %slug, room_id = %room.id, users_added = added, "room created");
        Ok(room.id)
    }

    pub fn create_permanent_room_task(&self, user_id: Uuid, slug: String) {
        let service = self.clone();
        let span = info_span!("chat.create_permanent_room", user_id = %user_id, slug = %slug);
        tokio::spawn(
            async move {
                match service.create_permanent_room(&slug).await {
                    Ok(_) => {
                        let _ = service
                            .evt_tx
                            .send(ChatEvent::PermanentRoomCreated { user_id, slug });
                    }
                    Err(e) => {
                        let _ = service.evt_tx.send(ChatEvent::ModerationFailed {
                            user_id,
                            message: e.to_string(),
                        });
                    }
                }
            }
            .instrument(span),
        );
    }

    async fn create_permanent_room(&self, slug: &str) -> Result<()> {
        let client = &self.db.get().await?;
        let room = ChatRoom::ensure_permanent(client, slug).await?;
        let added = ChatRoom::add_all_users(client, room.id).await?;
        tracing::info!(slug = %slug, room_id = %room.id, users_added = added, "permanent room created");
        Ok(())
    }

    pub fn invite_user_to_room_task(&self, user_id: Uuid, room_id: Uuid, target_username: String) {
        let service = self.clone();
        let span = info_span!(
            "chat.invite_user_to_room_task",
            user_id = %user_id,
            room_id = %room_id,
            target = %target_username
        );
        tokio::spawn(
            async move {
                let event = match service
                    .invite_user_to_room(user_id, room_id, &target_username)
                    .await
                {
                    Ok((room_slug, username)) => ChatEvent::InviteSucceeded {
                        user_id,
                        room_id,
                        room_slug,
                        username,
                    },
                    Err(e) => ChatEvent::InviteFailed {
                        user_id,
                        message: e.to_string(),
                    },
                };
                let _ = service.evt_tx.send(event);
            }
            .instrument(span),
        );
    }

    async fn invite_user_to_room(
        &self,
        user_id: Uuid,
        room_id: Uuid,
        target_username: &str,
    ) -> Result<(String, String)> {
        let client = &self.db.get().await?;
        let room = ChatRoom::get(client, room_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Room not found"))?;
        if room.kind == "dm" {
            anyhow::bail!("Cannot invite users to a DM");
        }
        let is_member = ChatRoomMember::is_member(client, room_id, user_id).await?;
        if !is_member {
            anyhow::bail!("You are not a member of this room");
        }

        let target = User::find_by_username(client, target_username)
            .await?
            .ok_or_else(|| anyhow::anyhow!("User '{}' not found", target_username))?;
        if target.id == user_id {
            anyhow::bail!("Cannot invite yourself");
        }

        self.ensure_user_not_banned_from_room(client, room_id, target.id)
            .await?;
        ChatRoomMember::join(client, room_id, target.id).await?;
        let room_slug = room.slug.clone().unwrap_or_else(|| room.kind.clone());
        Ok((room_slug, target.username))
    }

    pub fn moderate_room_member_task(
        &self,
        actor_user_id: Uuid,
        room_id: Uuid,
        target_username: String,
        action: RoomModerationAction,
        permissions: Permissions,
    ) {
        let service = self.clone();
        let span = info_span!(
            "chat.moderate_room_member_task",
            actor_user_id = %actor_user_id,
            room_id = %room_id,
            target = %target_username,
            action = action.verb()
        );
        tokio::spawn(
            async move {
                match service
                    .moderate_room_member(
                        actor_user_id,
                        room_id,
                        &target_username,
                        action,
                        permissions,
                    )
                    .await
                {
                    Ok(result) => {
                        let _ = service.evt_tx.send(ChatEvent::RoomModerated {
                            actor_user_id,
                            target_user_id: result.target_user_id,
                            room_id: result.room_id,
                            room_slug: result.room_slug,
                            target_username: result.target_username,
                            action: result.action,
                        });
                    }
                    Err(e) => {
                        let _ = service.evt_tx.send(ChatEvent::ModerationFailed {
                            user_id: actor_user_id,
                            message: e.to_string(),
                        });
                    }
                }
            }
            .instrument(span),
        );
    }

    async fn moderate_room_member(
        &self,
        actor_user_id: Uuid,
        room_id: Uuid,
        target_username: &str,
        action: RoomModerationAction,
        permissions: Permissions,
    ) -> Result<RoomModerationResult> {
        if !permissions.can_moderate() {
            anyhow::bail!("Moderator or admin only");
        }

        let client = &self.db.get().await?;
        let room = ChatRoom::get(client, room_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Room not found"))?;
        let room_slug = room
            .slug
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Room does not have a slug"))?;
        if room.kind != "topic" {
            anyhow::bail!("Room moderation is limited to topic rooms");
        }

        let actor_is_member = ChatRoomMember::is_member(client, room_id, actor_user_id).await?;
        if !actor_is_member {
            anyhow::bail!("You are not a member of this room");
        }

        let target = User::find_by_username(client, target_username)
            .await?
            .ok_or_else(|| anyhow::anyhow!("User '{}' not found", target_username))?;
        if target.id == actor_user_id {
            anyhow::bail!("Cannot {} yourself", action.verb());
        }
        if !permissions.is_admin() && (target.is_admin || target.is_moderator) {
            anyhow::bail!("Only admins can moderate staff");
        }

        match action {
            RoomModerationAction::Kick => {
                let removed = ChatRoomMember::leave(client, room_id, target.id).await?;
                if removed == 0 {
                    anyhow::bail!("@{} is not in #{}", target.username, room_slug);
                }
            }
            RoomModerationAction::Ban => {
                if RoomBan::is_active_for_room_and_user(client, room_id, target.id).await? {
                    anyhow::bail!("@{} is already banned from #{}", target.username, room_slug);
                }
                RoomBan::activate(client, room_id, target.id, actor_user_id, "", None).await?;
                let _ = ChatRoomMember::leave(client, room_id, target.id).await?;
            }
            RoomModerationAction::Unban => {
                if !RoomBan::is_active_for_room_and_user(client, room_id, target.id).await? {
                    anyhow::bail!("@{} is not banned from #{}", target.username, room_slug);
                }
                RoomBan::delete_for_room_and_user(client, room_id, target.id).await?;
            }
        }

        ModerationAuditLog::record(
            client,
            actor_user_id,
            format!("room_{}", action.verb()),
            "user",
            Some(target.id),
            json!({
                "room_id": room_id,
                "room_slug": room_slug.clone(),
                "target_user_id": target.id,
                "target_username": target.username.clone(),
            }),
        )
        .await?;

        Ok(RoomModerationResult {
            room_id,
            room_slug,
            target_user_id: target.id,
            target_username: target.username,
            action,
        })
    }

    pub fn admin_room_task(
        &self,
        actor_user_id: Uuid,
        room_id: Uuid,
        action: AdminRoomAction,
        permissions: Permissions,
    ) {
        let service = self.clone();
        let span = info_span!(
            "chat.admin_room_task",
            actor_user_id = %actor_user_id,
            room_id = %room_id,
            action = action.verb()
        );
        tokio::spawn(
            async move {
                match service
                    .admin_room_action(actor_user_id, room_id, action, permissions)
                    .await
                {
                    Ok(result) => {
                        let _ = service.evt_tx.send(ChatEvent::AdminRoomUpdated {
                            actor_user_id,
                            room_id: result.room_id,
                            old_slug: result.old_slug,
                            new_slug: result.new_slug,
                            visibility: result.visibility,
                            deleted: result.deleted,
                        });
                    }
                    Err(e) => {
                        let _ = service.evt_tx.send(ChatEvent::ModerationFailed {
                            user_id: actor_user_id,
                            message: e.to_string(),
                        });
                    }
                }
            }
            .instrument(span),
        );
    }

    async fn admin_room_action(
        &self,
        actor_user_id: Uuid,
        room_id: Uuid,
        action: AdminRoomAction,
        permissions: Permissions,
    ) -> Result<AdminRoomResult> {
        if !permissions.can_access_admin_surface() {
            anyhow::bail!("Admin only");
        }

        let client = &self.db.get().await?;
        let room = ChatRoom::get(client, room_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Room not found"))?;
        let old_slug = room
            .slug
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Room does not have a slug"))?;
        if room.kind != "topic" {
            anyhow::bail!("Admin room actions are limited to topic rooms");
        }
        if room.permanent {
            anyhow::bail!("Permanent rooms must use the dedicated admin room commands");
        }

        let result = match action {
            AdminRoomAction::Rename { new_slug } => {
                let updated = ChatRoom::rename_topic_room(client, room_id, &new_slug).await?;
                ModerationAuditLog::record(
                    client,
                    actor_user_id,
                    "room_rename",
                    "room",
                    Some(room_id),
                    json!({
                        "old_slug": old_slug.clone(),
                        "new_slug": updated.slug.clone(),
                    }),
                )
                .await?;
                AdminRoomResult {
                    room_id,
                    old_slug,
                    new_slug: updated.slug,
                    visibility: None,
                    deleted: false,
                }
            }
            AdminRoomAction::SetVisibility { visibility } => {
                let updated =
                    ChatRoom::set_topic_room_visibility(client, room_id, &visibility).await?;
                ModerationAuditLog::record(
                    client,
                    actor_user_id,
                    "room_visibility_change",
                    "room",
                    Some(room_id),
                    json!({
                        "room_slug": old_slug.clone(),
                        "visibility": updated.visibility.clone(),
                    }),
                )
                .await?;
                AdminRoomResult {
                    room_id,
                    old_slug,
                    new_slug: None,
                    visibility: Some(updated.visibility),
                    deleted: false,
                }
            }
            AdminRoomAction::Delete => {
                let deleted = ChatRoom::delete_topic_room(client, room_id).await?;
                if deleted == 0 {
                    anyhow::bail!("Room not found");
                }
                ModerationAuditLog::record(
                    client,
                    actor_user_id,
                    "room_delete",
                    "room",
                    Some(room_id),
                    json!({
                        "room_slug": old_slug.clone(),
                    }),
                )
                .await?;
                AdminRoomResult {
                    room_id,
                    old_slug,
                    new_slug: None,
                    visibility: None,
                    deleted: true,
                }
            }
        };

        Ok(result)
    }

    pub fn admin_user_task(
        &self,
        actor_user_id: Uuid,
        target_user_id: Uuid,
        action: AdminUserAction,
        permissions: Permissions,
        session_registry: Option<SessionRegistry>,
    ) {
        let service = self.clone();
        let span = info_span!(
            "chat.admin_user_task",
            actor_user_id = %actor_user_id,
            target_user_id = %target_user_id,
            action = action.verb()
        );
        tokio::spawn(
            async move {
                match service
                    .admin_user_action(
                        actor_user_id,
                        target_user_id,
                        action,
                        permissions,
                        session_registry,
                    )
                    .await
                {
                    Ok(result) => {
                        let _ = service.evt_tx.send(ChatEvent::AdminUserModerated {
                            actor_user_id,
                            target_user_id: result.target_user_id,
                            target_username: result.target_username,
                            action: result.action,
                            disconnected_sessions: result.disconnected_sessions,
                        });
                    }
                    Err(e) => {
                        let _ = service.evt_tx.send(ChatEvent::ModerationFailed {
                            user_id: actor_user_id,
                            message: e.to_string(),
                        });
                    }
                }
            }
            .instrument(span),
        );
    }

    async fn admin_user_action(
        &self,
        actor_user_id: Uuid,
        target_user_id: Uuid,
        action: AdminUserAction,
        permissions: Permissions,
        session_registry: Option<SessionRegistry>,
    ) -> Result<AdminUserResult> {
        if !permissions.can_access_admin_surface() {
            anyhow::bail!("Admin only");
        }
        if actor_user_id == target_user_id {
            anyhow::bail!("Cannot {} yourself", action.verb());
        }

        let session_registry =
            session_registry.ok_or_else(|| anyhow::anyhow!("Live session registry unavailable"))?;
        let client = &self.db.get().await?;
        let target = User::get(client, target_user_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("User not found"))?;
        let disconnected_sessions = match &action {
            AdminUserAction::DisconnectAllSessions => {
                session_registry
                    .disconnect_user_sessions(
                        target_user_id,
                        "You were disconnected by an admin".to_string(),
                    )
                    .await
            }
            AdminUserAction::DisconnectSession { session_id } => {
                let session_exists = session_registry
                    .sessions_for_user(target_user_id)
                    .into_iter()
                    .any(|session| session.session_id == *session_id);
                if !session_exists {
                    anyhow::bail!(
                        "session {} is not live for @{}",
                        session_id,
                        target.username
                    );
                }
                usize::from(
                    session_registry
                        .disconnect_session(
                            *session_id,
                            "You were disconnected by an admin".to_string(),
                        )
                        .await,
                )
            }
            AdminUserAction::Ban { reason, expires_at } => {
                if ServerBan::find_active_for_user_id(client, target_user_id)
                    .await?
                    .is_some()
                    || ServerBan::find_active_for_fingerprint(client, &target.fingerprint)
                        .await?
                        .is_some()
                {
                    anyhow::bail!("@{} is already server banned", target.username);
                }
                ServerBan::activate(
                    client,
                    target_user_id,
                    &target.fingerprint,
                    actor_user_id,
                    reason,
                    *expires_at,
                )
                .await?;
                let kick_msg = if reason.is_empty() {
                    "You were banned by an admin".to_string()
                } else {
                    format!("You were banned: {reason}")
                };
                session_registry
                    .disconnect_user_sessions(target_user_id, kick_msg)
                    .await
            }
            AdminUserAction::Unban => {
                let removed =
                    ServerBan::delete_active_for_user(client, target_user_id, &target.fingerprint)
                        .await?;
                if removed == 0 {
                    anyhow::bail!("@{} is not server banned", target.username);
                }
                0
            }
        };
        if disconnected_sessions == 0 {
            match action {
                AdminUserAction::DisconnectAllSessions => {
                    anyhow::bail!("@{} has no live sessions", target.username);
                }
                AdminUserAction::DisconnectSession { session_id } => {
                    anyhow::bail!("session {} is no longer live", session_id);
                }
                AdminUserAction::Ban { .. } | AdminUserAction::Unban => {}
            }
        }

        let ban_reason = match &action {
            AdminUserAction::Ban { reason, .. } => Some(reason.clone()),
            _ => None,
        };
        let ban_expires_at = match &action {
            AdminUserAction::Ban { expires_at, .. } => *expires_at,
            _ => None,
        };
        ModerationAuditLog::record(
            client,
            actor_user_id,
            format!("server_{}", action.verb()),
            "user",
            Some(target_user_id),
            json!({
                "target_user_id": target_user_id,
                "target_username": target.username.clone(),
                "session_id": match &action {
                    AdminUserAction::DisconnectAllSessions => None::<Uuid>,
                    AdminUserAction::DisconnectSession { session_id } => Some(*session_id),
                    AdminUserAction::Ban { .. } | AdminUserAction::Unban => None::<Uuid>,
                },
                "reason": ban_reason,
                "expires_at": ban_expires_at,
                "disconnected_sessions": disconnected_sessions,
            }),
        )
        .await?;

        Ok(AdminUserResult {
            target_user_id,
            target_username: target.username,
            action,
            disconnected_sessions,
        })
    }

    pub fn delete_permanent_room_task(&self, user_id: Uuid, slug: String) {
        let service = self.clone();
        let span = info_span!("chat.delete_permanent_room", user_id = %user_id, slug = %slug);
        tokio::spawn(
            async move {
                match service.delete_permanent_room(&slug).await {
                    Ok(_) => {
                        let _ = service
                            .evt_tx
                            .send(ChatEvent::PermanentRoomDeleted { user_id, slug });
                    }
                    Err(e) => {
                        let _ = service.evt_tx.send(ChatEvent::ModerationFailed {
                            user_id,
                            message: e.to_string(),
                        });
                    }
                }
            }
            .instrument(span),
        );
    }

    async fn delete_permanent_room(&self, slug: &str) -> Result<()> {
        let client = &self.db.get().await?;
        let count = ChatRoom::delete_permanent(client, slug).await?;
        if count == 0 {
            anyhow::bail!("Permanent room #{slug} not found");
        }
        tracing::info!(slug = %slug, "permanent room deleted");
        Ok(())
    }

    pub fn delete_message_task(&self, user_id: Uuid, message_id: Uuid, permissions: Permissions) {
        let service = self.clone();
        let span = info_span!("chat.delete_message", user_id = %user_id, message_id = %message_id);
        tokio::spawn(
            async move {
                match service
                    .delete_message(user_id, message_id, permissions)
                    .await
                {
                    Ok(room_id) => {
                        let _ = service.evt_tx.send(ChatEvent::MessageDeleted {
                            user_id,
                            room_id,
                            message_id,
                        });
                    }
                    Err(e) => {
                        let _ = service.evt_tx.send(ChatEvent::DeleteFailed {
                            user_id,
                            message: e.to_string(),
                        });
                    }
                }
            }
            .instrument(span),
        );
    }

    async fn delete_message(
        &self,
        user_id: Uuid,
        message_id: Uuid,
        permissions: Permissions,
    ) -> Result<Uuid> {
        let client = &self.db.get().await?;
        // Look up the message to get room_id
        let msg = ChatMessage::get(client, message_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Message not found"))?;
        let target_tier = if msg.user_id == user_id {
            TargetTier::Own
        } else {
            let target = User::get(client, msg.user_id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("target user not found"))?;
            TargetTier::from_user_flags(target.is_admin, target.is_moderator)
        };
        if !permissions
            .decide(Action::DeleteMessage, target_tier)
            .is_allowed()
        {
            anyhow::bail!("Cannot delete this message");
        }
        let count = if matches!(target_tier, TargetTier::Own) {
            ChatMessage::delete_by_author(client, message_id, user_id).await?
        } else {
            ChatMessage::delete_by_admin(client, message_id).await?
        };
        if count == 0 {
            anyhow::bail!("Cannot delete this message");
        }
        tracing::info!(message_id = %message_id, "message deleted");
        Ok(msg.room_id)
    }
}

fn ensure_staff_scope(permissions: Permissions, scope: StaffViewScope) -> Result<()> {
    match scope {
        StaffViewScope::Admin if !permissions.can_access_admin_surface() => {
            anyhow::bail!("Admin only");
        }
        StaffViewScope::Moderator if !permissions.can_access_mod_surface() => {
            anyhow::bail!("Moderator or admin only");
        }
        StaffViewScope::Admin | StaffViewScope::Moderator => {}
    }
    Ok(())
}

fn staff_users_title(scope: StaffViewScope) -> String {
    match scope {
        StaffViewScope::Admin => "Admin Users".to_string(),
        StaffViewScope::Moderator => "Mod Users".to_string(),
    }
}

fn staff_rooms_title(scope: StaffViewScope) -> String {
    match scope {
        StaffViewScope::Admin => "Admin Rooms".to_string(),
        StaffViewScope::Moderator => "Mod Rooms".to_string(),
    }
}

fn staff_room_label(room: &StaffRoomRecord) -> String {
    room.slug
        .as_ref()
        .map(|slug| format!("#{slug}"))
        .or_else(|| {
            room.language_code
                .as_ref()
                .map(|code| format!("#lang-{code}"))
        })
        .unwrap_or_else(|| room.kind.clone())
}

fn short_user_id(user_id: Uuid) -> String {
    let id = user_id.to_string();
    id[..id.len().min(8)].to_string()
}
