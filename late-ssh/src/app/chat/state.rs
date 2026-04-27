use std::collections::{HashMap, HashSet, VecDeque};

use late_core::{
    MutexRecover,
    models::{
        article::NEWS_MARKER, chat_message::ChatMessage,
        chat_message_reaction::ChatMessageReactionSummary, chat_room::ChatRoom,
    },
};
use ratatui_textarea::{CursorMove, Input, TextArea, WrapMode};
use tokio::sync::watch;
use uuid::Uuid;

use crate::app::common::overlay::Overlay;
use crate::authz::Permissions;
use crate::session::{LiveSessionSnapshot, PairedClientRegistry, SessionRegistry};

use crate::app::common::{composer, primitives::Banner};
use crate::app::help_modal::data::HelpTopic;
use crate::state::{ActiveUser, ActiveUsers};

use super::{
    discover, news, notifications,
    notifications::svc::NotificationService,
    showcase,
    svc::{
        AdminRoomAction, AuditLogEntry, ChatEvent, ChatService, ChatSnapshot, RoomModerationAction,
        StaffRoomRecord, StaffUserRecord, StaffViewScope,
    },
};

pub(crate) const ROOM_JUMP_KEYS: &[u8] = b"asdfghjklqwertyuiopzxcvbnm1234567890";

#[derive(Clone, Default)]
pub struct ChatRuntimeState {
    pub active_users: Option<ActiveUsers>,
    pub session_registry: Option<SessionRegistry>,
    pub paired_client_registry: Option<PairedClientRegistry>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MentionMatch {
    pub name: String,
    pub online: bool,
}

#[derive(Default)]
pub(crate) struct MentionAutocomplete {
    pub active: bool,
    pub query: String,
    pub trigger_offset: usize,
    pub matches: Vec<MentionMatch>,
    pub selected: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReplyTarget {
    pub author: String,
    pub preview: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RoomSlot {
    Room(Uuid),
    News,
    Notifications,
    Discover,
    Showcase,
}

pub struct ChatState {
    pub(crate) service: ChatService,
    user_id: Uuid,
    permissions: Permissions,
    active_users: Option<ActiveUsers>,
    session_registry: Option<SessionRegistry>,
    paired_client_registry: Option<PairedClientRegistry>,
    snapshot_rx: watch::Receiver<ChatSnapshot>,
    event_rx: tokio::sync::broadcast::Receiver<ChatEvent>,
    pub(crate) rooms: Vec<(ChatRoom, Vec<ChatMessage>)>,
    general_room_id: Option<Uuid>,
    pub(crate) usernames: HashMap<Uuid, String>,
    pub(crate) countries: HashMap<Uuid, String>,
    ignored_user_ids: HashSet<Uuid>,
    overlay: Option<Overlay>,
    pub(crate) unread_counts: HashMap<Uuid, i64>,
    pending_read_rooms: HashSet<Uuid>,
    visible_room_id: Option<Uuid>,
    room_tx: watch::Sender<Option<Uuid>>,
    pub(crate) selected_room_id: Option<Uuid>,
    pub(crate) room_jump_active: bool,
    composer: TextArea<'static>,
    pub(crate) composing: bool,
    composer_room_id: Option<Uuid>,
    pending_send_notices: VecDeque<Uuid>,
    pub(crate) pending_chat_screen_switch: bool,
    pub(crate) mention_ac: MentionAutocomplete,
    pub(crate) all_usernames: Vec<String>,
    pub(crate) bonsai_glyphs: HashMap<Uuid, String>,
    pub(crate) message_reactions: HashMap<Uuid, Vec<ChatMessageReactionSummary>>,
    pub(crate) selected_message_id: Option<Uuid>,
    pub(crate) reaction_leader_active: bool,
    pub(crate) highlighted_message_id: Option<Uuid>,
    pub(crate) edited_message_id: Option<Uuid>,
    pub(crate) reply_target: Option<ReplyTarget>,
    bg_task: tokio::task::AbortHandle,

    /// News (shown as a virtual room in the room list)
    pub(crate) news_selected: bool,
    pub(crate) news: news::state::State,

    /// Notifications / mentions (shown as a virtual room in the room list)
    pub(crate) notifications_selected: bool,
    pub(crate) notifications: notifications::state::State,
    pub(crate) discover_selected: bool,
    pub(crate) discover: discover::state::State,
    pub(crate) showcase_selected: bool,
    pub(crate) showcase: showcase::state::State,

    /// Pending desktop notifications drained on render. `kind` matches the
    /// string identifiers stored in `users.settings.notify_kinds` ("dms", "mentions").
    pub(crate) pending_notifications: Vec<PendingNotification>,
    requested_help_topic: Option<HelpTopic>,
    requested_settings_modal: bool,
    requested_quit: bool,
    staff_users_snapshot: Vec<StaffUserRecord>,
    staff_rooms_snapshot: Vec<StaffRoomRecord>,
    audit_log_snapshot: Vec<AuditLogEntry>,
}

pub(crate) struct PendingNotification {
    pub kind: &'static str,
    pub title: String,
    pub body: String,
}

impl Drop for ChatState {
    fn drop(&mut self) {
        self.bg_task.abort();
    }
}

impl ChatState {
    pub fn new(
        service: ChatService,
        notification_service: NotificationService,
        user_id: Uuid,
        permissions: Permissions,
        runtime: ChatRuntimeState,
        article_service: news::svc::ArticleService,
        showcase_service: showcase::svc::ShowcaseService,
    ) -> Self {
        let snapshot_rx = service.subscribe_state();
        let event_rx = service.subscribe_events();
        let (room_tx, room_rx) = watch::channel(None);
        let bg_task = service.start_user_refresh_task(user_id, room_rx);

        Self {
            service,
            user_id,
            permissions,
            active_users: runtime.active_users,
            session_registry: runtime.session_registry,
            paired_client_registry: runtime.paired_client_registry,
            snapshot_rx,
            event_rx,
            rooms: Vec::new(),
            general_room_id: None,
            usernames: HashMap::new(),
            countries: HashMap::new(),
            ignored_user_ids: HashSet::new(),
            overlay: None,
            unread_counts: HashMap::new(),
            pending_read_rooms: HashSet::new(),
            visible_room_id: None,
            room_tx,
            selected_room_id: None,
            room_jump_active: false,
            composer: new_chat_textarea(),
            composing: false,
            composer_room_id: None,
            pending_send_notices: VecDeque::new(),
            pending_chat_screen_switch: false,
            mention_ac: MentionAutocomplete::default(),
            all_usernames: Vec::new(),
            bonsai_glyphs: HashMap::new(),
            message_reactions: HashMap::new(),
            selected_message_id: None,
            reaction_leader_active: false,
            highlighted_message_id: None,
            edited_message_id: None,
            reply_target: None,
            bg_task,
            news_selected: false,
            news: news::state::State::new(article_service, user_id, permissions),
            notifications_selected: false,
            notifications: notifications::state::State::new(notification_service, user_id),
            discover_selected: false,
            discover: discover::state::State::new(),
            showcase_selected: false,
            showcase: showcase::state::State::new(
                showcase_service,
                user_id,
                permissions.can_access_admin_surface(),
            ),
            pending_notifications: Vec::new(),
            requested_help_topic: None,
            requested_settings_modal: false,
            requested_quit: false,
            staff_users_snapshot: Vec::new(),
            staff_rooms_snapshot: Vec::new(),
            audit_log_snapshot: Vec::new(),
        }
    }

    pub(crate) fn composer(&self) -> &TextArea<'static> {
        &self.composer
    }

    pub(crate) fn refresh_composer_theme(&mut self) {
        composer::apply_themed_textarea_style(&mut self.composer, self.composing);
        self.news.refresh_composer_theme();
        self.showcase.refresh_composer_theme();
    }

    pub fn is_composing(&self) -> bool {
        self.composing
    }

    pub fn start_composing(&mut self) {
        if let Some(room_id) = self.selected_room_id {
            self.start_composing_in_room(room_id);
        }
    }

    pub fn start_composing_in_room(&mut self, room_id: Uuid) {
        self.room_jump_active = false;
        self.composing = true;
        self.composer_room_id = Some(room_id);
        self.selected_message_id = None;
        self.reply_target = None;
        self.edited_message_id = None;
        composer::set_themed_textarea_cursor_visible(&mut self.composer, true);
    }

    pub fn request_list(&self) {
        self.service
            .list_chats_task(self.user_id, self.selected_room_id);
    }

    pub fn sync_selection(&mut self) {
        if self.rooms.is_empty() {
            self.selected_room_id = None;
            self.room_jump_active = false;
            return;
        }

        if let Some(selected_id) = self.selected_room_id
            && self.rooms.iter().any(|(room, _)| room.id == selected_id)
        {
            return;
        }

        self.selected_room_id = Some(self.rooms[0].0.id);
    }

    pub fn mark_room_read(&mut self, room_id: Uuid) {
        self.pending_read_rooms.insert(room_id);
        self.unread_counts.insert(room_id, 0);
        self.service.mark_room_read_task(self.user_id, room_id);
    }

    pub fn mark_selected_room_read(&mut self) {
        let Some(room_id) = self.selected_room_id else {
            return;
        };

        self.mark_room_read(room_id);
    }

    pub fn visible_room_id(&self) -> Option<Uuid> {
        self.visible_room_id
    }

    pub fn set_visible_room_id(&mut self, room_id: Option<Uuid>) {
        self.visible_room_id = room_id;
    }

    /// Returns visible messages for the given room.
    fn visible_messages_for_room(&self, room_id: Uuid) -> Vec<&ChatMessage> {
        self.rooms
            .iter()
            .find(|(room, _)| room.id == room_id)
            .map(|(_, msgs)| msgs.iter().collect())
            .unwrap_or_default()
    }

    pub(crate) fn overlay(&self) -> Option<&Overlay> {
        self.overlay.as_ref()
    }

    pub(crate) fn has_overlay(&self) -> bool {
        self.overlay.is_some()
    }

    pub fn close_overlay(&mut self) {
        self.overlay = None;
    }

    pub fn scroll_overlay(&mut self, delta: i16) {
        if let Some(overlay) = &mut self.overlay {
            overlay.scroll(delta);
        }
    }

    pub fn take_requested_help_topic(&mut self) -> Option<HelpTopic> {
        self.requested_help_topic.take()
    }

    pub fn take_requested_settings_modal(&mut self) -> bool {
        std::mem::take(&mut self.requested_settings_modal)
    }

    pub fn take_requested_quit(&mut self) -> bool {
        std::mem::take(&mut self.requested_quit)
    }

    fn select_from_ids(&mut self, ids: &[Uuid], delta: isize) {
        self.reaction_leader_active = false;
        if ids.is_empty() {
            self.selected_message_id = None;
            return;
        }

        let current_idx = self
            .selected_message_id
            .and_then(|id| ids.iter().position(|mid| *mid == id));

        let new_idx = match current_idx {
            Some(idx) => (idx as isize)
                .saturating_add(delta)
                .clamp(0, ids.len() as isize - 1) as usize,
            None => 0,
        };

        self.selected_message_id = Some(ids[new_idx]);
    }

    /// Move message cursor by delta. Positive = toward older, negative = toward newer.
    /// First press activates cursor on the newest message.
    pub fn select_message_in_room(&mut self, room_id: Uuid, delta: isize) {
        self.highlighted_message_id = None;
        let ids: Vec<Uuid> = self
            .visible_messages_for_room(room_id)
            .iter()
            .map(|m| m.id)
            .collect();
        self.select_from_ids(&ids, delta);
    }

    pub fn clear_message_selection(&mut self) {
        self.reaction_leader_active = false;
        self.selected_message_id = None;
    }

    pub fn begin_reaction_leader(&mut self) -> bool {
        if self.selected_message_id.is_none() {
            return false;
        }
        self.reaction_leader_active = true;
        true
    }

    pub fn cancel_reaction_leader(&mut self) {
        self.reaction_leader_active = false;
    }

    pub fn is_reaction_leader_active(&self) -> bool {
        self.reaction_leader_active
    }

    pub fn begin_reply_to_selected_in_room(&mut self, room_id: Uuid) -> Option<Banner> {
        self.reaction_leader_active = false;
        let message = self.selected_message_in_room(room_id)?;
        let message_user_id = message.user_id;
        let message_body = message.body.clone();
        let author = self
            .usernames
            .get(&message_user_id)
            .map(|name| name.trim())
            .filter(|name| !name.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| short_user_id(message_user_id));
        self.reply_target = Some(ReplyTarget {
            author,
            preview: reply_preview_text(&message_body),
        });
        self.composing = true;
        self.composer_room_id = Some(room_id);
        self.edited_message_id = None;
        composer::set_themed_textarea_cursor_visible(&mut self.composer, true);
        None
    }

    pub fn begin_edit_selected_in_room(&mut self, room_id: Uuid) -> Option<Banner> {
        self.reaction_leader_active = false;
        let selected_id = self.selected_message_id?;
        let Some(message) = self.find_message_in_room(room_id, selected_id) else {
            return Some(Banner::error("Selected message not found"));
        };
        let message_user_id = message.user_id;
        let room_id = message.room_id;
        let body = message.body.clone();
        self.begin_edit_message(selected_id, message_user_id, room_id, &body)
    }

    fn begin_edit_message(
        &mut self,
        selected_id: Uuid,
        message_user_id: Uuid,
        room_id: Uuid,
        body: &str,
    ) -> Option<Banner> {
        let is_own = message_user_id == self.user_id;
        if !self.permissions.can_edit_message(is_own) {
            return Some(Banner::error("Can only edit your own messages"));
        }
        self.edited_message_id = Some(selected_id);
        self.composer = new_chat_textarea();
        self.composer.insert_str(body);
        self.composing = true;
        self.composer_room_id = Some(room_id);
        composer::set_themed_textarea_cursor_visible(&mut self.composer, true);
        None
    }

    pub(crate) fn reply_target(&self) -> Option<&ReplyTarget> {
        self.reply_target.as_ref()
    }

    /// Delete the selected message if owned by user (or if admin).
    /// Moves selection to the adjacent message (prefer the next/older one,
    /// fall back to the previous/newer one) so pressing `d` repeatedly
    /// cleanly reaps a run of own messages without the cursor jumping
    /// back to the newest every time.
    pub fn delete_selected_message_in_room(&mut self, room_id: Uuid) -> Option<Banner> {
        let selected_id = self.selected_message_id?;
        let msg_user_id = self
            .find_message_in_room(room_id, selected_id)
            .map(|m| m.user_id)?;
        let is_own = msg_user_id == self.user_id;
        if !self.permissions.can_delete_message(is_own) {
            return Some(Banner::error("Can only delete your own messages"));
        }
        self.service
            .delete_message_task(self.user_id, selected_id, self.permissions);
        self.selected_message_id = self
            .rooms
            .iter()
            .find(|(room, _)| room.id == room_id)
            .and_then(|(_, msgs)| adjacent_message_id(msgs, selected_id));
        Some(Banner::success("Deleting message..."))
    }

    fn selected_message_in_room(&self, room_id: Uuid) -> Option<&ChatMessage> {
        let selected_id = self.selected_message_id?;
        self.find_message_in_room(room_id, selected_id)
    }

    pub fn selected_message_body_in_room(&self, room_id: Uuid) -> Option<String> {
        self.selected_message_in_room(room_id)
            .map(|m| m.body.clone())
    }

    pub fn selected_message_author_in_room(&self, room_id: Uuid) -> Option<(Uuid, String)> {
        let message = self.selected_message_in_room(room_id)?;
        let user_id = message.user_id;
        let display_name = self
            .usernames
            .get(&user_id)
            .map(|name| name.trim())
            .filter(|name| !name.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| short_user_id(user_id));
        Some((user_id, display_name))
    }

    pub fn react_to_selected_message_in_room(
        &mut self,
        room_id: Uuid,
        kind: i16,
    ) -> Option<Banner> {
        self.reaction_leader_active = false;
        let message = self.selected_message_in_room(room_id)?;
        self.service
            .toggle_message_reaction_task(self.user_id, message.id, kind);
        None
    }

    fn find_message_in_room(&self, room_id: Uuid, message_id: Uuid) -> Option<&ChatMessage> {
        self.rooms
            .iter()
            .find(|(room, _)| room.id == room_id)
            .and_then(|(_, msgs)| msgs.iter().find(|m| m.id == message_id))
    }

    fn room_slug(&self, room_id: Uuid) -> Option<String> {
        room_slug_for(&self.rooms, room_id)
    }

    fn selected_room_slug(&self) -> Option<String> {
        self.selected_room().and_then(|room| room.slug.clone())
    }

    fn selected_room(&self) -> Option<&ChatRoom> {
        let room_id = self.selected_room_id?;
        self.rooms
            .iter()
            .find(|(room, _)| room.id == room_id)
            .map(|(room, _)| room)
    }

    pub fn general_room_id(&self) -> Option<Uuid> {
        self.general_room_id.or_else(|| {
            self.rooms
                .iter()
                .find(|(room, _)| room.kind == "general" && room.slug.as_deref() == Some("general"))
                .map(|(room, _)| room.id)
        })
    }

    /// Flatten joined rooms into the pick-list the settings modal shows in
    /// its Favorites tab. Labels are pre-resolved here (DMs → `@peer`, rooms
    /// → `#slug`, language rooms → `#lang-xx`) so the modal stays ignorant of
    /// `ChatRoom` internals.
    pub fn favorite_room_options(&self) -> Vec<crate::app::settings_modal::state::RoomOption> {
        use crate::app::settings_modal::state::RoomOption;
        self.rooms
            .iter()
            .map(|(room, _)| {
                let label = if room.kind == "dm" {
                    self.dm_display_name(room)
                } else if let Some(slug) = room.slug.as_deref().filter(|s| !s.is_empty()) {
                    format!("#{slug}")
                } else if let Some(code) = room.language_code.as_deref() {
                    format!("#lang-{code}")
                } else {
                    format!("#{}", room.kind)
                };
                RoomOption { id: room.id, label }
            })
            .collect()
    }

    fn dm_display_name(&self, room: &ChatRoom) -> String {
        dm_sort_key(room, self.user_id, &self.usernames)
    }

    /// Build the flat visual navigation order.
    /// Order: core (general, announcements) → news → mentions → public rooms
    /// (alpha) → private rooms (alpha) → DMs
    pub(crate) fn visual_order(&self) -> Vec<RoomSlot> {
        let mut order = Vec::new();

        // Core: permanent rooms, hardcoded order
        let core_order = ["general", "announcements", "suggestions", "bugs"];
        for slug in &core_order {
            if let Some((room, _)) = self
                .rooms
                .iter()
                .find(|(r, _)| r.permanent && r.slug.as_deref() == Some(slug))
            {
                order.push(RoomSlot::Room(room.id));
            }
        }
        // Any other permanent rooms not in the hardcoded list
        for (room, _) in &self.rooms {
            if room.kind != "dm"
                && room.permanent
                && !core_order.contains(&room.slug.as_deref().unwrap_or(""))
            {
                order.push(RoomSlot::Room(room.id));
            }
        }

        // News
        order.push(RoomSlot::News);

        // Mentions / notifications
        order.push(RoomSlot::Notifications);

        // Showcase
        order.push(RoomSlot::Showcase);

        // Discover
        order.push(RoomSlot::Discover);

        // Public rooms (non-DM, non-permanent, alpha by slug)
        let mut public: Vec<_> = self
            .rooms
            .iter()
            .filter(|(r, _)| r.kind != "dm" && !r.permanent && r.visibility == "public")
            .collect();
        public.sort_by(|(a, _), (b, _)| a.slug.cmp(&b.slug));
        order.extend(public.iter().map(|(r, _)| RoomSlot::Room(r.id)));

        // Private rooms (visibility=private, alpha by slug)
        let mut private: Vec<_> = self
            .rooms
            .iter()
            .filter(|(r, _)| r.kind != "dm" && !r.permanent && r.visibility == "private")
            .collect();
        private.sort_by(|(a, _), (b, _)| a.slug.cmp(&b.slug));
        order.extend(private.iter().map(|(r, _)| RoomSlot::Room(r.id)));

        // DMs (sorted by display name to match nav rendering)
        let mut dms: Vec<_> = self.rooms.iter().filter(|(r, _)| r.kind == "dm").collect();
        dms.sort_by(|(a, _), (b, _)| {
            let name_a = self.dm_display_name(a);
            let name_b = self.dm_display_name(b);
            name_a.cmp(&name_b)
        });
        order.extend(dms.iter().map(|(r, _)| RoomSlot::Room(r.id)));

        order
    }

    pub(crate) fn room_jump_targets(&self) -> Vec<(u8, RoomSlot)> {
        self.visual_order()
            .into_iter()
            .zip(ROOM_JUMP_KEYS.iter().copied())
            .map(|(slot, key)| (key, slot))
            .collect()
    }

    fn adjacent_composer_room(&self, delta: isize) -> Option<Uuid> {
        adjacent_composer_room(
            &self.visual_order(),
            self.composer_room_id.or(self.selected_room_id),
            delta,
        )
    }

    pub(crate) fn select_room_slot(&mut self, slot: RoomSlot) -> bool {
        self.selected_message_id = None;
        self.reaction_leader_active = false;
        self.highlighted_message_id = None;

        match slot {
            RoomSlot::News => {
                let changed = !self.news_selected;
                if changed {
                    self.select_news();
                }
                changed
            }
            RoomSlot::Notifications => {
                let changed = !self.notifications_selected;
                if changed {
                    self.select_notifications();
                }
                changed
            }
            RoomSlot::Discover => {
                let changed = !self.discover_selected;
                if changed {
                    self.select_discover();
                }
                changed
            }
            RoomSlot::Showcase => {
                let changed = !self.showcase_selected;
                if changed {
                    self.select_showcase();
                }
                changed
            }
            RoomSlot::Room(next_id) => {
                let changed = self.news_selected
                    || self.notifications_selected
                    || self.discover_selected
                    || self.showcase_selected
                    || self.selected_room_id != Some(next_id);
                self.news_selected = false;
                self.notifications_selected = false;
                self.discover_selected = false;
                self.showcase_selected = false;
                self.selected_room_id = Some(next_id);
                changed
            }
        }
    }

    /// Switch to the adjacent room while keeping an in-progress composer
    /// draft in place. Reply/edit targets are dropped (they reference a
    /// message in the prior room, and carrying them across would submit
    /// to the wrong thread) and the composer is re-anchored to the new
    /// room so `submit_composer` posts to the correct place.
    ///
    /// Returns `true` if the selection actually changed.
    pub fn switch_room_preserving_draft(&mut self, delta: isize) -> bool {
        let Some(next_room_id) = self.adjacent_composer_room(delta) else {
            return false;
        };
        if !self.select_room_slot(RoomSlot::Room(next_room_id)) {
            return false;
        }
        self.reply_target = None;
        self.edited_message_id = None;
        self.composer_room_id = Some(next_room_id);
        self.visible_room_id = Some(next_room_id);
        self.mark_room_read(next_room_id);
        self.request_list();
        true
    }

    pub fn move_selection(&mut self, delta: isize) -> bool {
        let order = self.visual_order();
        if order.is_empty() {
            return false;
        }

        let current_item = if self.notifications_selected {
            RoomSlot::Notifications
        } else if self.discover_selected {
            RoomSlot::Discover
        } else if self.showcase_selected {
            RoomSlot::Showcase
        } else if self.news_selected {
            RoomSlot::News
        } else {
            self.selected_room_id
                .map(RoomSlot::Room)
                .unwrap_or(RoomSlot::News)
        };
        let current = order
            .iter()
            .position(|item| *item == current_item)
            .unwrap_or(0) as isize;
        let next = wrapped_index(current, delta, order.len());
        self.select_room_slot(order[next])
    }

    pub fn activate_room_jump(&mut self) {
        self.room_jump_active = !self.composing && !self.rooms.is_empty();
    }

    pub fn cancel_room_jump(&mut self) {
        self.room_jump_active = false;
    }

    pub fn handle_room_jump_key(&mut self, byte: u8) -> bool {
        let targets = self.room_jump_targets();
        let Some(slot) = resolve_room_jump_target(&targets, byte) else {
            self.room_jump_active = false;
            return false;
        };

        self.room_jump_active = false;
        self.select_room_slot(slot)
    }

    pub fn stop_composing(&mut self) {
        self.composing = false;
        self.room_jump_active = false;
        self.composer_room_id = None;
        self.reaction_leader_active = false;
        self.reply_target = None;
        composer::set_themed_textarea_cursor_visible(&mut self.composer, false);
    }

    pub fn reset_composer(&mut self) {
        self.composer = new_chat_textarea();
        self.composing = false;
        self.room_jump_active = false;
        self.composer_room_id = None;
        self.reaction_leader_active = false;
        self.reply_target = None;
        self.edited_message_id = None;
        self.mention_ac = MentionAutocomplete::default();
    }

    fn clear_composer_after_submit(&mut self) {
        self.composer = new_chat_textarea();
        self.composing = false;
        self.room_jump_active = false;
        self.composer_room_id = None;
        self.reaction_leader_active = false;
        self.reply_target = None;
        self.edited_message_id = None;
        self.mention_ac = MentionAutocomplete::default();
    }

    fn clear_composer_after_send(&mut self) {
        self.composer = new_chat_textarea();
        composer::set_themed_textarea_cursor_visible(&mut self.composer, self.composing);
        self.room_jump_active = false;
        self.reaction_leader_active = false;
        self.reply_target = None;
        self.edited_message_id = None;
        self.mention_ac = MentionAutocomplete::default();
    }

    fn open_overlay(&mut self, title: &str, lines: Vec<String>) {
        if lines.is_empty() {
            return;
        }
        self.overlay = Some(Overlay::new(title, lines));
    }

    fn ignore_list_lines(&self) -> Vec<String> {
        if self.ignored_user_ids.is_empty() {
            return vec!["Ignore list is empty".to_string()];
        }

        let mut labels: Vec<String> = self
            .ignored_user_ids
            .iter()
            .map(|id| {
                self.usernames
                    .get(id)
                    .map(|name| format!("@{name}"))
                    .unwrap_or_else(|| format!("@<unknown:{}>", short_user_id(*id)))
            })
            .collect();
        labels.sort();
        labels
    }

    fn active_user_lines(&self) -> Vec<String> {
        format_active_user_lines(self.active_users.as_ref())
    }

    pub fn refresh_staff_users_snapshot(&self) {
        if !self.permissions.can_access_mod_surface() {
            return;
        }
        let scope = if self.permissions.can_access_admin_surface() {
            StaffViewScope::Admin
        } else {
            StaffViewScope::Moderator
        };
        self.service
            .refresh_staff_users_snapshot_task(self.user_id, self.permissions, scope);
    }

    pub fn refresh_audit_log_snapshot(&self) {
        if !self.permissions.can_access_mod_surface() {
            return;
        }
        self.service
            .refresh_audit_log_snapshot_task(self.user_id, self.permissions);
    }

    pub fn refresh_staff_rooms_snapshot(&self) {
        if !self.permissions.can_access_mod_surface() {
            return;
        }
        let scope = if self.permissions.can_access_admin_surface() {
            StaffViewScope::Admin
        } else {
            StaffViewScope::Moderator
        };
        self.service
            .refresh_staff_rooms_snapshot_task(self.user_id, self.permissions, scope);
    }

    pub fn control_center_user_ids(&self) -> Vec<Uuid> {
        self.staff_users_snapshot
            .iter()
            .map(|user| user.user_id)
            .collect()
    }

    pub fn control_center_user_list_lines(&self, selected_user_id: Option<Uuid>) -> Vec<String> {
        format_control_center_user_list_lines(
            &self.staff_users_snapshot,
            self.session_registry.as_ref(),
            selected_user_id,
        )
    }

    pub fn control_center_user_detail_lines(&self, selected_user_id: Option<Uuid>) -> Vec<String> {
        let live_session_count = self
            .user_sessions_for_control_center(selected_user_id)
            .len();
        format_control_center_user_detail_lines(
            &self.staff_users_snapshot,
            control_center_selected_user(&self.staff_users_snapshot, selected_user_id),
            live_session_count,
            self.permissions.can_access_admin_surface(),
        )
    }

    pub fn control_center_user_session_ids(&self, selected_user_id: Option<Uuid>) -> Vec<Uuid> {
        let Some(user_id) = selected_user_id else {
            return Vec::new();
        };
        self.session_registry
            .as_ref()
            .map(|registry| {
                registry
                    .sessions_for_user(user_id)
                    .into_iter()
                    .map(|session| session.session_id)
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn control_center_user_session_lines(
        &self,
        selected_user_id: Option<Uuid>,
        selected_session_id: Option<Uuid>,
    ) -> Vec<String> {
        format_control_center_user_session_lines(
            self.user_sessions_for_control_center(selected_user_id)
                .as_slice(),
            self.paired_client_registry.as_ref(),
            selected_session_id,
            self.permissions.can_access_admin_surface(),
        )
    }

    pub fn control_center_user_label(&self, user_id: Uuid) -> Option<String> {
        self.staff_users_snapshot
            .iter()
            .find(|user| user.user_id == user_id)
            .map(control_center_user_label)
    }

    pub fn control_center_user_tier_flags(&self, user_id: Uuid) -> Option<(bool, bool)> {
        self.staff_users_snapshot
            .iter()
            .find(|user| user.user_id == user_id)
            .map(|user| (user.is_admin, user.is_moderator))
    }

    pub fn control_center_user_session_label(&self, session_id: Uuid) -> Option<String> {
        self.session_registry
            .as_ref()
            .and_then(|registry| {
                registry
                    .snapshot_all()
                    .into_iter()
                    .find(|session| session.session_id == session_id)
            })
            .map(|session| short_session_id(session.session_id))
    }

    pub fn control_center_staff_user_ids(&self) -> Vec<Uuid> {
        self.staff_users_snapshot
            .iter()
            .filter(|user| user.is_admin || user.is_moderator)
            .map(|user| user.user_id)
            .collect()
    }

    pub fn control_center_staff_list_lines(&self, selected_staff_id: Option<Uuid>) -> Vec<String> {
        format_control_center_staff_list_lines(&self.staff_users_snapshot, selected_staff_id)
    }

    pub fn control_center_staff_detail_lines(
        &self,
        selected_staff_id: Option<Uuid>,
    ) -> Vec<String> {
        format_control_center_staff_detail_lines(&self.staff_users_snapshot, selected_staff_id)
    }

    pub fn control_center_audit_ids(&self, filter: &AuditFilter) -> Vec<Uuid> {
        self.audit_log_snapshot
            .iter()
            .filter(|entry| audit_entry_matches(entry, filter))
            .map(|entry| entry.id)
            .collect()
    }

    pub fn control_center_audit_list_lines(
        &self,
        selected_audit_id: Option<Uuid>,
        filter: &AuditFilter,
    ) -> Vec<String> {
        let entries: Vec<&AuditLogEntry> = self
            .audit_log_snapshot
            .iter()
            .filter(|entry| audit_entry_matches(entry, filter))
            .collect();
        format_control_center_audit_list_lines(&entries, selected_audit_id)
    }

    pub fn control_center_audit_detail_lines(
        &self,
        selected_audit_id: Option<Uuid>,
        filter: &AuditFilter,
    ) -> Vec<String> {
        let entries: Vec<&AuditLogEntry> = self
            .audit_log_snapshot
            .iter()
            .filter(|entry| audit_entry_matches(entry, filter))
            .collect();
        format_control_center_audit_detail_lines(&entries, selected_audit_id)
    }

    pub fn control_center_room_ids(&self) -> Vec<Uuid> {
        self.staff_rooms_snapshot
            .iter()
            .map(|room| room.room_id)
            .collect()
    }

    pub fn control_center_room_list_lines(&self, selected_room_id: Option<Uuid>) -> Vec<String> {
        format_control_center_room_list_lines(
            &self.staff_rooms_snapshot,
            control_center_selected_room(&self.staff_rooms_snapshot, selected_room_id)
                .map(|room| room.room_id),
        )
    }

    pub fn control_center_room_detail_lines(&self, selected_room_id: Option<Uuid>) -> Vec<String> {
        format_control_center_room_detail_lines(
            &self.staff_rooms_snapshot,
            control_center_selected_room(&self.staff_rooms_snapshot, selected_room_id),
        )
    }

    pub fn control_center_room_label(&self, room_id: Uuid) -> Option<String> {
        self.staff_rooms_snapshot
            .iter()
            .find(|room| room.room_id == room_id)
            .map(control_center_room_label)
    }

    pub fn moderate_control_center_room_member(
        &self,
        room_id: Uuid,
        target_username: &str,
        action: RoomModerationAction,
    ) -> Banner {
        let target_username = target_username.trim().trim_start_matches('@');
        if target_username.is_empty() {
            return Banner::error("Enter a username");
        }
        self.service.moderate_room_member_task(
            self.user_id,
            room_id,
            target_username.to_string(),
            action,
            self.permissions,
        );
        let room_label = self
            .control_center_room_label(room_id)
            .unwrap_or_else(|| "room".to_string());
        Banner::success(&format!(
            "{} @{} in {}...",
            action.progress_verb(),
            target_username,
            room_label
        ))
    }

    pub fn admin_control_center_room_action(
        &self,
        room_id: Uuid,
        action: AdminRoomAction,
    ) -> Banner {
        let room_label = self
            .control_center_room_label(room_id)
            .unwrap_or_else(|| "room".to_string());
        let banner = match &action {
            AdminRoomAction::Rename { new_slug } => {
                let new_slug = new_slug.trim().trim_start_matches('#');
                if new_slug.is_empty() {
                    return Banner::error("Enter a room name");
                }
                Banner::success(&format!("Renaming {} to #{}...", room_label, new_slug))
            }
            AdminRoomAction::SetVisibility { visibility } => {
                Banner::success(&format!("Making {} {}...", room_label, visibility))
            }
            AdminRoomAction::Delete => Banner::success(&format!("Deleting {}...", room_label)),
        };
        self.service
            .admin_room_task(self.user_id, room_id, action, self.permissions);
        banner
    }

    pub fn admin_control_center_user_action(
        &self,
        user_id: Uuid,
        action: super::svc::AdminUserAction,
    ) -> Banner {
        let user_label = self
            .control_center_user_label(user_id)
            .unwrap_or_else(|| "user".to_string());
        let banner = match &action {
            super::svc::AdminUserAction::DisconnectAllSessions => {
                Banner::success(&format!("Disconnecting {}...", user_label))
            }
            super::svc::AdminUserAction::DisconnectSession { session_id } => {
                Banner::success(&format!(
                    "Disconnecting session {} for {}...",
                    self.control_center_user_session_label(*session_id)
                        .unwrap_or_else(|| "session".to_string()),
                    user_label
                ))
            }
            super::svc::AdminUserAction::Ban { .. } => {
                Banner::success(&format!("Banning {}...", user_label))
            }
            super::svc::AdminUserAction::Unban => {
                Banner::success(&format!("Unbanning {}...", user_label))
            }
        };
        self.service.admin_user_task(
            self.user_id,
            user_id,
            action.clone(),
            self.permissions,
            self.session_registry.clone(),
        );
        banner
    }

    pub fn tier_change_user_action(
        &self,
        target_user_id: Uuid,
        action: super::svc::TierChangeAction,
    ) -> Banner {
        let user_label = self
            .control_center_user_label(target_user_id)
            .unwrap_or_else(|| "user".to_string());
        let verb = match action {
            super::svc::TierChangeAction::GrantModerator => "Granting moderator to",
            super::svc::TierChangeAction::RevokeModerator => "Revoking moderator from",
            super::svc::TierChangeAction::GrantAdmin => "Granting admin to",
        };
        let banner = Banner::success(&format!("{verb} {user_label}..."));
        self.service
            .change_user_tier_task(self.user_id, target_user_id, action, self.permissions);
        banner
    }

    fn user_sessions_for_control_center(
        &self,
        selected_user_id: Option<Uuid>,
    ) -> Vec<LiveSessionSnapshot> {
        let Some(user_id) = selected_user_id else {
            return Vec::new();
        };
        self.session_registry
            .as_ref()
            .map(|registry| registry.sessions_for_user(user_id))
            .unwrap_or_default()
    }

    fn open_staff_users_overlay(&mut self, title: &str, mut lines: Vec<String>) {
        if self.active_users.is_some() {
            let active_lines = self.active_user_lines();
            let mut prefixed = vec!["Online Now".to_string()];
            prefixed.extend(active_lines.into_iter().map(|line| format!("  {line}")));
            prefixed.push(String::new());
            prefixed.append(&mut lines);
            lines = prefixed;
        }
        lines = annotate_staff_user_lines(
            lines,
            self.session_registry.as_ref(),
            self.paired_client_registry.as_ref(),
        );
        self.open_overlay(title, lines);
    }

    pub fn submit_composer(&mut self, keep_open: bool, from_dashboard: bool) -> Option<Banner> {
        let body = self.composer.lines().join("\n").trim_end().to_string();

        // Room-membership commands are intentionally chat-page-only: they
        // operate on `selected_room_id`, which the dashboard never drives.
        // Rather than silently target the wrong room, refuse here and point
        // the user at page 2.
        if from_dashboard && parse_leave_command(&body) {
            self.clear_composer_after_submit();
            return Some(Banner::error(
                "open the chat page (press 2) to leave a room",
            ));
        }
        if from_dashboard && parse_user_command(&body, "/invite").is_some() {
            self.clear_composer_after_submit();
            return Some(Banner::error(
                "open the chat page (press 2) to invite a user",
            ));
        }
        if from_dashboard && parse_room_moderation_command(&body).is_some() {
            self.clear_composer_after_submit();
            return Some(Banner::error(
                "open the chat page (press 2) to moderate a room",
            ));
        }
        if from_dashboard && parse_admin_room_command(&body).is_some() {
            self.clear_composer_after_submit();
            return Some(Banner::error(
                "open the chat page (press 2) to manage a room",
            ));
        }

        if body.trim() == "/binds" {
            self.clear_composer_after_submit();
            self.requested_help_topic = Some(HelpTopic::Chat);
            return None;
        }

        if body.trim() == "/music" {
            self.clear_composer_after_submit();
            self.requested_help_topic = Some(HelpTopic::Music);
            return None;
        }

        if body.trim() == "/settings" {
            self.clear_composer_after_submit();
            self.requested_settings_modal = true;
            return None;
        }

        if body.trim() == "/exit" {
            self.clear_composer_after_submit();
            self.requested_quit = true;
            return None;
        }

        if body.trim() == "/active" {
            self.clear_composer_after_submit();
            self.open_overlay("Active Users", self.active_user_lines());
            return None;
        }

        if body.trim() == "/members" {
            // Resolve the target room BEFORE clearing the composer —
            // `clear_composer_after_submit` nulls `composer_room_id`, so
            // reading after would always fall back to the chat-page
            // `selected_room_id` and miss the dashboard's active favorite.
            let target = self.composer_room_id.or(self.selected_room_id);
            self.clear_composer_after_submit();
            let Some(room_id) = target else {
                return Some(Banner::error("no room selected"));
            };
            self.service.list_room_members_task(self.user_id, room_id);
            return None;
        }

        if body.trim() == "/list" {
            self.clear_composer_after_submit();
            self.service.list_public_rooms_task(self.user_id);
            return None;
        }

        if let Some(subcommand) = parse_subcommand(&body, "/admin") {
            self.clear_composer_after_submit();
            if !self.permissions.can_access_admin_surface() {
                return Some(Banner::error("Admin only: /admin"));
            }
            match subcommand {
                Some("help") | None => {
                    self.open_overlay("Admin Help", admin_help_lines());
                }
                Some("users") => {
                    self.service.list_staff_users_task(
                        self.user_id,
                        self.permissions,
                        StaffViewScope::Admin,
                    );
                }
                Some("rooms") => {
                    self.service.list_staff_rooms_task(
                        self.user_id,
                        self.permissions,
                        StaffViewScope::Admin,
                    );
                }
                Some("mods") => {
                    self.service
                        .list_moderators_task(self.user_id, self.permissions);
                }
                Some(rest) => {
                    let Some(command) = parse_admin_room_subcommand(rest) else {
                        return Some(Banner::error(&format!("Unknown admin command: {rest}")));
                    };
                    let Some(room_id) = self.selected_room_id else {
                        return Some(Banner::error("No room selected"));
                    };
                    let Some(room) = self.rooms.iter().find(|(room, _)| room.id == room_id) else {
                        return Some(Banner::error("No room selected"));
                    };
                    if room.0.kind != "topic" {
                        return Some(Banner::error(
                            "Admin room actions are only available for topic rooms",
                        ));
                    }
                    if room.0.permanent {
                        return Some(Banner::error(
                            "Permanent rooms must use the dedicated admin room commands",
                        ));
                    }
                    let Some(room_slug) = room.0.slug.clone() else {
                        return Some(Banner::error(
                            "Admin room actions are only available for topic rooms",
                        ));
                    };
                    match command {
                        ParsedAdminRoomCommand::Show => {
                            self.open_overlay("Admin Room", admin_room_lines(&room_slug));
                        }
                        ParsedAdminRoomCommand::Rename {
                            new_slug: Some(new_slug),
                        } => {
                            self.service.admin_room_task(
                                self.user_id,
                                room_id,
                                AdminRoomAction::Rename {
                                    new_slug: new_slug.to_string(),
                                },
                                self.permissions,
                            );
                            return Some(Banner::success(&format!(
                                "Renaming #{} to #{}...",
                                room_slug, new_slug
                            )));
                        }
                        ParsedAdminRoomCommand::Rename { new_slug: None } => {
                            return Some(Banner::error("Usage: /admin room rename #new"));
                        }
                        ParsedAdminRoomCommand::SetVisibility { visibility } => {
                            self.service.admin_room_task(
                                self.user_id,
                                room_id,
                                AdminRoomAction::SetVisibility {
                                    visibility: visibility.to_string(),
                                },
                                self.permissions,
                            );
                            return Some(Banner::success(&format!(
                                "Making #{} {}...",
                                room_slug, visibility
                            )));
                        }
                        ParsedAdminRoomCommand::Delete => {
                            self.service.admin_room_task(
                                self.user_id,
                                room_id,
                                AdminRoomAction::Delete,
                                self.permissions,
                            );
                            return Some(Banner::success(&format!("Deleting #{}...", room_slug)));
                        }
                    }
                }
            }
            return None;
        }

        if let Some(subcommand) = parse_subcommand(&body, "/mod") {
            self.clear_composer_after_submit();
            if !self.permissions.can_access_mod_surface() {
                return Some(Banner::error("Moderator or admin only: /mod"));
            }
            match subcommand {
                Some("users") => {
                    self.service.list_staff_users_task(
                        self.user_id,
                        self.permissions,
                        StaffViewScope::Moderator,
                    );
                }
                Some("rooms") => {
                    self.service.list_staff_rooms_task(
                        self.user_id,
                        self.permissions,
                        StaffViewScope::Moderator,
                    );
                }
                Some(rest) => {
                    let Some(command) = parse_room_moderation_subcommand(rest) else {
                        return Some(Banner::error(&format!("Unknown mod command: {rest}")));
                    };
                    let Some(room_id) = self.selected_room_id else {
                        return Some(Banner::error("No room selected"));
                    };
                    let Some(room_slug) = room_slug_for(&self.rooms, room_id) else {
                        return Some(Banner::error(
                            "Room moderation is only available for topic rooms",
                        ));
                    };
                    match command {
                        ParsedRoomModerationCommand::Show => {
                            self.open_overlay("Mod Room", mod_room_lines(&room_slug));
                        }
                        ParsedRoomModerationCommand::Action {
                            action,
                            target_username: Some(target_username),
                        } => {
                            self.service.moderate_room_member_task(
                                self.user_id,
                                room_id,
                                target_username.to_string(),
                                action,
                                self.permissions,
                            );
                            return Some(Banner::success(&format!(
                                "{} @{} in #{}...",
                                action.progress_verb(),
                                target_username,
                                room_slug,
                            )));
                        }
                        ParsedRoomModerationCommand::Action {
                            action,
                            target_username: None,
                        } => {
                            return Some(Banner::error(&format!(
                                "Usage: /mod room {} @user",
                                action.verb()
                            )));
                        }
                    }
                }
                None => {
                    return Some(Banner::error("Usage: /mod users | /mod rooms | /mod room"));
                }
            }
            return None;
        }

        if let Some(target) = parse_user_command(&body, "/ignore") {
            self.clear_composer_after_submit();
            match target {
                None => self.open_overlay("Ignored Users", self.ignore_list_lines()),
                Some(name) => self
                    .service
                    .ignore_user_task(self.user_id, name.to_string()),
            }
            return None;
        }
        if let Some(target) = parse_user_command(&body, "/unignore") {
            self.clear_composer_after_submit();
            match target {
                None => self.open_overlay("Ignored Users", self.ignore_list_lines()),
                Some(name) => self
                    .service
                    .unignore_user_task(self.user_id, name.to_string()),
            }
            return None;
        }

        if let Some(target) = parse_dm_command(&body) {
            self.service.start_dm_task(self.user_id, target.to_string());
            self.clear_composer_after_submit();
            return Some(Banner::success(&format!("Opening DM with {target}...")));
        }

        if let Some(room) = parse_room_command(&body, "/public") {
            self.service
                .open_public_room_task(self.user_id, room.to_string());
            self.clear_composer_after_submit();
            return Some(Banner::success(&format!("Opening public #{room}...")));
        }

        if let Some(room) = parse_room_command(&body, "/private") {
            self.clear_composer_after_submit();
            self.service
                .create_private_room_task(self.user_id, room.to_string());
            return Some(Banner::success(&format!("Creating private #{room}...")));
        }

        if let Some(target) = parse_user_command(&body, "/invite") {
            self.clear_composer_after_submit();
            let Some(room_id) = self.selected_room_id else {
                return Some(Banner::error("No room selected"));
            };
            let Some(target) = target else {
                return Some(Banner::error("Usage: /invite @user"));
            };
            self.service
                .invite_user_to_room_task(self.user_id, room_id, target.to_string());
            return Some(Banner::success(&format!("Inviting @{target}...")));
        }

        if parse_leave_command(&body) {
            self.clear_composer_after_submit();
            if let Some(room_id) = self.selected_room_id {
                let slug = self.selected_room_slug().unwrap_or_default();
                self.service
                    .leave_room_task(self.user_id, room_id, slug.clone());
                return Some(Banner::success(&format!("Leaving #{slug}...")));
            } else {
                return Some(Banner::error("No room selected"));
            }
        }

        if let Some(slug) = parse_create_room_command(&body) {
            self.clear_composer_after_submit();
            if !self.permissions.can_manage_permanent_rooms() {
                return Some(Banner::error("Admin only: /create-room"));
            }
            self.service
                .create_permanent_room_task(self.user_id, slug.to_string());
            return Some(Banner::success(&format!("Creating #{slug}...")));
        }

        if let Some(slug) = parse_delete_room_command(&body) {
            self.clear_composer_after_submit();
            if !self.permissions.can_manage_permanent_rooms() {
                return Some(Banner::error("Admin only: /delete-room"));
            }
            self.service
                .delete_permanent_room_task(self.user_id, slug.to_string());
            return Some(Banner::success(&format!("Deleting #{slug}...")));
        }

        if let Some(slug) = parse_fill_room_command(&body) {
            self.clear_composer_after_submit();
            if !self.permissions.is_admin() {
                return Some(Banner::error("Admin only: /fill-room"));
            }
            self.service.fill_room_task(self.user_id, slug.to_string());
            return Some(Banner::success(&format!("Filling #{slug}...")));
        }

        if let Some(command) = unknown_slash_command(&body) {
            self.clear_composer_after_submit();
            return Some(Banner::error(&format!("Unknown command: {command}")));
        }

        if let Some(room_id) = self.composer_room_id
            && !body.is_empty()
        {
            let request_id = Uuid::now_v7();
            let body = if let Some(reply) = &self.reply_target {
                format!("> @{}: {}\n{}", reply.author, reply.preview, body)
            } else {
                body
            };
            if let Some(message_id) = self.edited_message_id {
                self.service.edit_message_task(
                    self.user_id,
                    message_id,
                    body,
                    request_id,
                    self.permissions,
                );
            } else {
                self.service.send_message_task(
                    self.user_id,
                    room_id,
                    self.room_slug(room_id),
                    body,
                    request_id,
                    self.permissions,
                );
            }
            self.pending_send_notices.push_back(request_id);
        }
        if keep_open {
            self.clear_composer_after_send();
        } else {
            self.clear_composer_after_submit();
        }
        None
    }

    pub fn composer_clear(&mut self) {
        let composing = self.composing;
        self.composer = new_chat_textarea();
        composer::set_themed_textarea_cursor_visible(&mut self.composer, composing);
    }

    pub fn composer_backspace(&mut self) {
        self.composer.delete_char();
    }

    pub fn composer_delete_right(&mut self) {
        self.composer.delete_next_char();
    }

    pub fn composer_delete_word_right(&mut self) {
        self.composer.delete_next_word();
    }

    pub fn composer_delete_word_left(&mut self) {
        self.composer.delete_word();
    }

    pub fn composer_push(&mut self, ch: char) {
        self.composer.insert_char(ch);
    }

    pub fn composer_cursor_left(&mut self) {
        self.composer.move_cursor(CursorMove::Back);
    }

    pub fn composer_cursor_right(&mut self) {
        self.composer.move_cursor(CursorMove::Forward);
    }

    pub fn composer_cursor_word_left(&mut self) {
        self.composer.move_cursor(CursorMove::WordBack);
    }

    pub fn composer_cursor_word_right(&mut self) {
        self.composer.move_cursor(CursorMove::WordForward);
    }

    pub fn composer_cursor_up(&mut self) {
        self.composer.move_cursor(CursorMove::Up);
    }

    pub fn composer_cursor_down(&mut self) {
        self.composer.move_cursor(CursorMove::Down);
    }

    pub fn composer_paste(&mut self) {
        self.composer.paste();
    }

    pub fn composer_undo(&mut self) {
        self.composer.undo();
    }

    /// Readline ^U: drop everything from the cursor back to the start of the
    /// current line, leaving later lines intact. Replaces the earlier
    /// clear-the-whole-composer behavior.
    pub fn composer_kill_to_head(&mut self) {
        self.composer.delete_line_by_head();
    }

    /// Forward a synthesized `Input` to the TextArea so it can dispatch via
    /// its built-in emacs/readline keymap (^A/^E/^K/^F/^B/...).
    pub fn composer_input(&mut self, input: Input) {
        self.composer.input(input);
    }

    pub fn tick(&mut self) -> Option<Banner> {
        let _ = self.room_tx.send(self.selected_room_id);
        self.drain_snapshot();
        let banner = self.drain_events();
        let news_banner = self.news.tick();
        let notif_banner = self.notifications.tick();
        let showcase_banner = self.showcase.tick();
        banner.or(news_banner).or(notif_banner).or(showcase_banner)
    }

    pub fn select_news(&mut self) {
        self.room_jump_active = false;
        self.news_selected = true;
        self.notifications_selected = false;
        self.discover_selected = false;
        self.showcase_selected = false;
        self.selected_message_id = None;
        self.highlighted_message_id = None;
        self.news.list_articles();
        self.news.mark_read();
    }

    pub fn deselect_news(&mut self) {
        self.news_selected = false;
    }

    pub fn select_notifications(&mut self) {
        self.room_jump_active = false;
        self.notifications_selected = true;
        self.news_selected = false;
        self.discover_selected = false;
        self.showcase_selected = false;
        self.selected_message_id = None;
        self.highlighted_message_id = None;
        self.notifications.list();
        self.notifications.mark_read();
    }

    pub fn select_discover(&mut self) {
        self.room_jump_active = false;
        self.discover_selected = true;
        self.notifications_selected = false;
        self.news_selected = false;
        self.showcase_selected = false;
        self.selected_message_id = None;
        self.highlighted_message_id = None;
    }

    pub fn select_showcase(&mut self) {
        self.room_jump_active = false;
        self.showcase_selected = true;
        self.discover_selected = false;
        self.notifications_selected = false;
        self.news_selected = false;
        self.selected_message_id = None;
        self.highlighted_message_id = None;
        self.showcase.list();
        self.showcase.mark_read();
    }

    pub fn join_selected_discover_room(&mut self) -> Option<Banner> {
        let item = self.discover.selected_item()?.clone();
        self.service
            .join_public_room_task(self.user_id, item.room_id, item.slug.clone());
        Some(Banner::success(&format!("Joining #{}...", item.slug)))
    }

    pub fn cursor_visible(&self) -> bool {
        self.composing
    }

    pub fn is_autocomplete_active(&self) -> bool {
        self.mention_ac.active
    }

    pub fn update_autocomplete(&mut self) {
        // Scan backward from end of composer to find a trigger `@`
        let text = self.composer.lines().join("\n");
        let bytes = text.as_bytes();
        let mut at_offset = None;
        for i in (0..bytes.len()).rev() {
            if bytes[i] == b'@' {
                // Valid if at start or preceded by whitespace (space or newline)
                if i == 0 || bytes[i - 1].is_ascii_whitespace() {
                    at_offset = Some(i);
                }
                break;
            }
            // Stop scanning if we hit whitespace (no @ in this word)
            if bytes[i].is_ascii_whitespace() {
                break;
            }
        }

        let Some(offset) = at_offset else {
            self.mention_ac.active = false;
            return;
        };

        let query = &text[offset + 1..];
        let query_lower = query.to_ascii_lowercase();
        let active_users = self.active_users.as_ref();
        let matches = rank_mention_matches(&self.all_usernames, &query_lower, || {
            online_username_set(active_users)
        });

        if matches.is_empty() {
            self.mention_ac.active = false;
            return;
        }

        self.mention_ac.active = true;
        self.mention_ac.query = query.to_string();
        self.mention_ac.trigger_offset = offset;
        self.mention_ac.selected = self
            .mention_ac
            .selected
            .min(matches.len().saturating_sub(1));
        self.mention_ac.matches = matches;
    }

    pub fn ac_move_selection(&mut self, delta: isize) {
        if !self.mention_ac.active || self.mention_ac.matches.is_empty() {
            return;
        }
        let len = self.mention_ac.matches.len() as isize;
        let cur = self.mention_ac.selected as isize;
        self.mention_ac.selected = (cur + delta).clamp(0, len - 1) as usize;
    }

    pub fn ac_confirm(&mut self) {
        if !self.mention_ac.active || self.mention_ac.matches.is_empty() {
            return;
        }
        let username = self.mention_ac.matches[self.mention_ac.selected]
            .name
            .clone();
        let text = self.composer.lines().join("\n");
        let next = format!("{}@{} ", &text[..self.mention_ac.trigger_offset], username);
        let composing = self.composing;
        self.composer = new_chat_textarea();
        self.composer.insert_str(next);
        composer::set_themed_textarea_cursor_visible(&mut self.composer, composing);
        self.mention_ac = MentionAutocomplete::default();
    }

    pub fn autocomplete_exact_slash_command_mention(&self) -> bool {
        if !self.mention_ac.active || self.mention_ac.matches.is_empty() {
            return false;
        }
        let text = self.composer.lines().join("\n");
        if !text.trim_start().starts_with('/') {
            return false;
        }
        let Some(selected) = self.mention_ac.matches.get(self.mention_ac.selected) else {
            return false;
        };
        selected.name.eq_ignore_ascii_case(&self.mention_ac.query)
    }

    pub fn ac_dismiss(&mut self) {
        self.mention_ac = MentionAutocomplete::default();
    }

    pub fn general_messages(&self) -> &[ChatMessage] {
        let Some(general_id) = self.general_room_id else {
            return &[];
        };
        self.messages_for_room(general_id)
    }

    /// Messages for any joined room — used by the dashboard chat card when
    /// the user pins favorites and cycles between them.
    pub fn messages_for_room(&self, room_id: Uuid) -> &[ChatMessage] {
        self.rooms
            .iter()
            .find(|(room, _)| room.id == room_id)
            .map(|(_, msgs)| msgs.as_slice())
            .unwrap_or(&[])
    }

    pub fn usernames(&self) -> &HashMap<Uuid, String> {
        &self.usernames
    }

    pub fn countries(&self) -> &HashMap<Uuid, String> {
        &self.countries
    }

    pub fn bonsai_glyphs(&self) -> &HashMap<Uuid, String> {
        &self.bonsai_glyphs
    }

    pub fn message_reactions(&self) -> &HashMap<Uuid, Vec<ChatMessageReactionSummary>> {
        &self.message_reactions
    }

    fn drain_snapshot(&mut self) {
        if !self.snapshot_rx.has_changed().unwrap_or(false) {
            return;
        }

        let snapshot = self.snapshot_rx.borrow_and_update().clone();
        if snapshot.user_id != Some(self.user_id) {
            return;
        }

        self.usernames = snapshot.usernames;
        self.countries = snapshot.countries;
        self.ignored_user_ids = snapshot.ignored_user_ids.into_iter().collect();
        self.rooms = self.merge_rooms(snapshot.chat_rooms);
        self.discover.set_items(snapshot.discover_rooms);
        self.general_room_id = snapshot.general_room_id;
        self.unread_counts = self.merge_unread_counts(snapshot.unread_counts);
        self.all_usernames = snapshot.all_usernames;
        self.bonsai_glyphs = snapshot.bonsai_glyphs;
        self.message_reactions = self.merge_message_reactions(snapshot.message_reactions);
        self.sync_selection();
    }

    fn drain_events(&mut self) -> Option<Banner> {
        let mut banner = None;
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                ChatEvent::MessageCreated {
                    message,
                    target_user_ids,
                } => {
                    let is_targeted = target_user_ids.is_some();
                    if let Some(targets) = target_user_ids
                        && !targets.contains(&self.user_id)
                    {
                        continue;
                    }
                    if is_targeted
                        && !self
                            .rooms
                            .iter()
                            .any(|(room, _)| room.id == message.room_id)
                    {
                        self.request_list();
                    }
                    // Desktop notification queueing. target_user_ids is Some for
                    // DM/private rooms, None for public rooms. Don't notify on
                    // messages we authored ourselves.
                    if message.user_id != self.user_id {
                        let nickname = self
                            .usernames
                            .get(&message.user_id)
                            .cloned()
                            .unwrap_or_else(|| "someone".to_string());
                        let preview: String =
                            message.body.replace('\n', " ").chars().take(80).collect();

                        if is_targeted {
                            self.pending_notifications.push(PendingNotification {
                                kind: "dms",
                                title: format!("New DM from {nickname}"),
                                body: preview,
                            });
                        } else if let Some(me) = self.usernames.get(&self.user_id) {
                            let me_lc = me.to_ascii_lowercase();
                            if crate::app::common::mentions::extract_mentions(&message.body)
                                .iter()
                                .any(|m| m == &me_lc)
                            {
                                self.pending_notifications.push(PendingNotification {
                                    kind: "mentions",
                                    title: format!("{nickname} mentioned you"),
                                    body: preview,
                                });
                            }
                        }
                    }
                    self.push_message(message);
                }
                ChatEvent::SendSucceeded {
                    user_id,
                    request_id,
                } if self.user_id == user_id => {
                    self.pending_send_notices.retain(|id| *id != request_id);
                    banner = Some(Banner::success("Message sent"));
                }
                ChatEvent::DeltaSynced {
                    user_id,
                    room_id,
                    messages,
                } if self.user_id == user_id => {
                    for message in messages {
                        if message.room_id == room_id {
                            self.push_message(message);
                        }
                    }
                }
                ChatEvent::SendFailed {
                    user_id,
                    request_id,
                    message,
                } if self.user_id == user_id => {
                    self.pending_send_notices.retain(|id| *id != request_id);
                    banner = Some(Banner::error(&message));
                }
                ChatEvent::DmOpened { user_id, room_id } if self.user_id == user_id => {
                    self.news_selected = false;
                    self.notifications_selected = false;
                    self.discover_selected = false;
                    self.showcase_selected = false;
                    self.selected_room_id = Some(room_id);
                    self.request_list();
                    self.pending_chat_screen_switch = true;
                    banner = Some(Banner::success("DM opened"));
                }
                ChatEvent::DmFailed { user_id, message } if self.user_id == user_id => {
                    banner = Some(Banner::error(&message));
                }
                ChatEvent::RoomJoined {
                    user_id,
                    room_id,
                    slug,
                } if self.user_id == user_id => {
                    self.news_selected = false;
                    self.notifications_selected = false;
                    self.discover_selected = false;
                    self.showcase_selected = false;
                    self.selected_room_id = Some(room_id);
                    self.request_list();
                    self.pending_chat_screen_switch = true;
                    banner = Some(Banner::success(&format!("Joined #{slug}")));
                }
                ChatEvent::RoomFailed { user_id, message } if self.user_id == user_id => {
                    banner = Some(Banner::error(&message));
                }
                ChatEvent::RoomLeft { user_id, slug } if self.user_id == user_id => {
                    self.selected_room_id = None;
                    self.request_list();
                    banner = Some(Banner::success(&format!("Left #{slug}")));
                }
                ChatEvent::LeaveFailed { user_id, message } if self.user_id == user_id => {
                    banner = Some(Banner::error(&message));
                }
                ChatEvent::RoomCreated {
                    user_id,
                    room_id,
                    slug,
                } if self.user_id == user_id => {
                    self.news_selected = false;
                    self.notifications_selected = false;
                    self.discover_selected = false;
                    self.showcase_selected = false;
                    self.selected_room_id = Some(room_id);
                    self.request_list();
                    self.pending_chat_screen_switch = true;
                    banner = Some(Banner::success(&format!("Created #{slug}")));
                }
                ChatEvent::RoomCreateFailed { user_id, message } if self.user_id == user_id => {
                    banner = Some(Banner::error(&message));
                }
                ChatEvent::PermanentRoomCreated { user_id, slug } if self.user_id == user_id => {
                    self.request_list();
                    banner = Some(Banner::success(&format!("Created permanent #{slug}")));
                }
                ChatEvent::PermanentRoomDeleted { user_id, slug } if self.user_id == user_id => {
                    self.request_list();
                    banner = Some(Banner::success(&format!("Deleted permanent #{slug}")));
                }
                ChatEvent::AdminRoomUpdated {
                    actor_user_id,
                    room_id,
                    old_slug,
                    new_slug,
                    visibility,
                    deleted,
                } => {
                    let was_selected = Some(room_id) == self.selected_room_id;
                    if deleted && Some(room_id) == self.selected_room_id {
                        self.selected_room_id = None;
                    }
                    self.request_list();
                    if self.user_id == actor_user_id {
                        banner = Some(Banner::success(&admin_room_success_message(
                            &old_slug,
                            new_slug.as_deref(),
                            visibility.as_deref(),
                            deleted,
                        )));
                    } else if deleted && was_selected {
                        banner = Some(Banner::error(&format!("Room #{} was deleted", old_slug)));
                    }
                    if self.permissions.can_access_mod_surface() {
                        self.refresh_staff_rooms_snapshot();
                        self.refresh_audit_log_snapshot();
                    }
                }
                ChatEvent::AdminUserModerated {
                    actor_user_id,
                    target_user_id: _,
                    target_username,
                    action,
                    disconnected_sessions,
                } => {
                    if self.user_id == actor_user_id {
                        match action {
                            super::svc::AdminUserAction::DisconnectAllSessions => {
                                banner = Some(Banner::success(&format!(
                                    "Disconnected @{} ({} live {})",
                                    target_username,
                                    disconnected_sessions,
                                    if disconnected_sessions == 1 {
                                        "session"
                                    } else {
                                        "sessions"
                                    }
                                )));
                            }
                            super::svc::AdminUserAction::DisconnectSession { session_id } => {
                                banner = Some(Banner::success(&format!(
                                    "Disconnected session {} for @{}",
                                    short_session_id(session_id),
                                    target_username
                                )));
                            }
                            super::svc::AdminUserAction::Ban { .. } => {
                                banner = Some(Banner::success(&if disconnected_sessions == 0 {
                                    format!("Banned @{}", target_username)
                                } else {
                                    format!(
                                        "Banned @{} and disconnected {} live {}",
                                        target_username,
                                        disconnected_sessions,
                                        if disconnected_sessions == 1 {
                                            "session"
                                        } else {
                                            "sessions"
                                        }
                                    )
                                }));
                            }
                            super::svc::AdminUserAction::Unban => {
                                banner = Some(Banner::success(&format!(
                                    "Unbanned @{}",
                                    target_username
                                )));
                            }
                        }
                    }
                    if self.permissions.can_access_mod_surface() {
                        self.refresh_staff_users_snapshot();
                        self.refresh_audit_log_snapshot();
                    }
                }
                ChatEvent::UserTierChanged {
                    actor_user_id,
                    target_user_id: _,
                    target_username,
                    action,
                } => {
                    if self.user_id == actor_user_id {
                        let verb = match action {
                            super::svc::TierChangeAction::GrantModerator => "Granted moderator to",
                            super::svc::TierChangeAction::RevokeModerator => {
                                "Revoked moderator from"
                            }
                            super::svc::TierChangeAction::GrantAdmin => "Granted admin to",
                        };
                        banner = Some(Banner::success(&format!("{verb} @{target_username}")));
                    }
                    if self.permissions.can_access_mod_surface() {
                        self.refresh_staff_users_snapshot();
                        self.refresh_audit_log_snapshot();
                    }
                }
                ChatEvent::ModerationFailed { user_id, message } if self.user_id == user_id => {
                    banner = Some(Banner::error(&message));
                }
                ChatEvent::RoomFilled {
                    user_id,
                    slug,
                    users_added,
                } if self.user_id == user_id => {
                    self.request_list();
                    banner = Some(Banner::success(&format!(
                        "Filled #{slug} ({users_added} users added)"
                    )));
                }
                ChatEvent::AdminFailed { user_id, message } if self.user_id == user_id => {
                    banner = Some(Banner::error(&message));
                }
                ChatEvent::RoomModerated {
                    actor_user_id,
                    target_user_id,
                    room_id,
                    room_slug,
                    target_username,
                    action,
                } => {
                    if self.user_id == actor_user_id {
                        self.request_list();
                        banner = Some(Banner::success(&format!(
                            "{} @{} in #{}",
                            action.success_verb(),
                            target_username,
                            room_slug,
                        )));
                    }
                    if self.user_id == target_user_id {
                        if matches!(
                            action,
                            RoomModerationAction::Kick | RoomModerationAction::Ban
                        ) && Some(room_id) == self.selected_room_id
                        {
                            self.selected_room_id = None;
                        }
                        self.request_list();
                        if matches!(
                            action,
                            RoomModerationAction::Kick | RoomModerationAction::Ban
                        ) {
                            banner = Some(Banner::error(&format!(
                                "You were {} from #{}",
                                action.success_verb().to_ascii_lowercase(),
                                room_slug,
                            )));
                        }
                    }
                    if self.permissions.can_access_mod_surface() {
                        self.refresh_staff_rooms_snapshot();
                        self.refresh_audit_log_snapshot();
                    }
                }
                ChatEvent::MessageDeleted {
                    user_id,
                    room_id,
                    message_id,
                } => {
                    self.remove_message(room_id, message_id);
                    if self.user_id == user_id {
                        banner = Some(Banner::success("Message deleted"));
                    }
                }
                ChatEvent::MessageEdited {
                    message,
                    target_user_ids,
                } => {
                    if let Some(targets) = target_user_ids
                        && !targets.contains(&self.user_id)
                    {
                        continue;
                    }
                    self.replace_message(message);
                }
                ChatEvent::MessageReactionsUpdated {
                    room_id: _,
                    message_id,
                    reactions,
                    target_user_ids,
                } => {
                    if let Some(targets) = target_user_ids
                        && !targets.contains(&self.user_id)
                    {
                        continue;
                    }
                    self.message_reactions.insert(message_id, reactions);
                }
                ChatEvent::EditSucceeded {
                    user_id,
                    request_id,
                } if self.user_id == user_id => {
                    self.pending_send_notices.retain(|id| *id != request_id);
                    banner = Some(Banner::success("Message edited"));
                }
                ChatEvent::EditFailed {
                    user_id,
                    request_id,
                    message,
                } if self.user_id == user_id => {
                    self.pending_send_notices.retain(|id| *id != request_id);
                    banner = Some(Banner::error(&message));
                }
                ChatEvent::DeleteFailed { user_id, message } if self.user_id == user_id => {
                    banner = Some(Banner::error(&message));
                }
                ChatEvent::IgnoreListUpdated {
                    user_id,
                    ignored_user_ids,
                    message,
                } if self.user_id == user_id => {
                    self.ignored_user_ids = ignored_user_ids.into_iter().collect();
                    self.refilter_local_messages();
                    banner = Some(Banner::success(&message));
                }
                ChatEvent::IgnoreFailed { user_id, message } if self.user_id == user_id => {
                    banner = Some(Banner::error(&message));
                }
                ChatEvent::RoomMembersListed {
                    user_id,
                    title,
                    members,
                } if self.user_id == user_id => {
                    self.open_overlay(&title, members);
                }
                ChatEvent::PublicRoomsListed {
                    user_id,
                    title,
                    rooms,
                } if self.user_id == user_id => {
                    self.open_overlay(&title, rooms);
                }
                ChatEvent::StaffUsersListed {
                    user_id,
                    title,
                    lines,
                } if self.user_id == user_id => {
                    self.open_staff_users_overlay(&title, lines);
                }
                ChatEvent::StaffUsersSnapshotUpdated { user_id, users }
                    if self.user_id == user_id =>
                {
                    self.staff_users_snapshot = users;
                }
                ChatEvent::StaffRoomsSnapshotUpdated { user_id, rooms }
                    if self.user_id == user_id =>
                {
                    self.staff_rooms_snapshot = rooms;
                }
                ChatEvent::AuditLogSnapshotUpdated { user_id, entries }
                    if self.user_id == user_id =>
                {
                    self.audit_log_snapshot = entries;
                }
                ChatEvent::StaffRoomsListed {
                    user_id,
                    title,
                    lines,
                } if self.user_id == user_id => {
                    self.open_overlay(&title, lines);
                }
                ChatEvent::ModeratorsListed {
                    user_id,
                    title,
                    lines,
                } if self.user_id == user_id => {
                    self.open_overlay(&title, lines);
                }
                ChatEvent::InviteSucceeded {
                    user_id,
                    room_id,
                    room_slug,
                    username,
                } if self.user_id == user_id => {
                    if Some(room_id) == self.selected_room_id {
                        self.request_list();
                    }
                    banner = Some(Banner::success(&format!(
                        "Invited @{username} to #{room_slug}"
                    )));
                }
                ChatEvent::RoomMembersListFailed { user_id, message }
                    if self.user_id == user_id =>
                {
                    banner = Some(Banner::error(&message));
                }
                ChatEvent::PublicRoomsListFailed { user_id, message }
                    if self.user_id == user_id =>
                {
                    banner = Some(Banner::error(&message));
                }
                ChatEvent::StaffQueryFailed { user_id, message } if self.user_id == user_id => {
                    banner = Some(Banner::error(&message));
                }
                ChatEvent::InviteFailed { user_id, message } if self.user_id == user_id => {
                    banner = Some(Banner::error(&message));
                }
                _ => {}
            }
        }
        banner
    }

    fn push_message(&mut self, message: ChatMessage) {
        let in_dm_room = self
            .rooms
            .iter()
            .any(|(room, _)| room.id == message.room_id && room.kind == "dm");

        if !in_dm_room && self.message_is_ignored(&message) {
            return;
        }

        let is_viewing_room = Some(message.room_id) == self.visible_room_id;

        let Some((_, messages)) = self
            .rooms
            .iter_mut()
            .find(|(room, _)| room.id == message.room_id)
        else {
            return;
        };

        if messages.iter().any(|existing| existing.id == message.id) {
            return;
        }

        // Service snapshots are newest-first; keep same order for cheap appends at the front.
        let room_id = message.room_id;
        messages.insert(0, message);
        if messages.len() > 1000 {
            let removed_ids: Vec<Uuid> = messages
                .iter()
                .skip(1000)
                .map(|message| message.id)
                .collect();
            messages.truncate(1000);
            for message_id in removed_ids {
                self.message_reactions.remove(&message_id);
            }
        }

        // Only mark the room as read if the user is actually viewing it.
        // Other warm rooms keep their unread badge until the user opens them.
        if is_viewing_room {
            self.unread_counts.insert(room_id, 0);
        }
    }

    fn remove_message(&mut self, room_id: Uuid, message_id: Uuid) {
        if let Some((_, messages)) = self.rooms.iter_mut().find(|(room, _)| room.id == room_id) {
            messages.retain(|m| m.id != message_id);
        }
        self.message_reactions.remove(&message_id);
    }

    fn replace_message(&mut self, message: ChatMessage) {
        if let Some((_, messages)) = self
            .rooms
            .iter_mut()
            .find(|(room, _)| room.id == message.room_id)
            && let Some(existing) = messages.iter_mut().find(|m| m.id == message.id)
        {
            *existing = message;
        }
    }

    fn merge_rooms(
        &self,
        incoming: Vec<(ChatRoom, Vec<ChatMessage>)>,
    ) -> Vec<(ChatRoom, Vec<ChatMessage>)> {
        let previous_by_room: HashMap<Uuid, &Vec<ChatMessage>> = self
            .rooms
            .iter()
            .map(|(room, msgs)| (room.id, msgs))
            .collect();

        incoming
            .into_iter()
            .map(|(room, messages)| {
                let messages = if messages.is_empty() {
                    previous_by_room
                        .get(&room.id)
                        .map(|previous| (*previous).clone())
                        .unwrap_or_default()
                } else {
                    messages
                };
                // DMs: don't filter. Users leave the DM room if they want it gone.
                let messages = if room.kind == "dm" {
                    messages
                } else {
                    self.filter_messages(messages)
                };
                (room, messages)
            })
            .collect()
    }

    fn merge_unread_counts(&mut self, mut incoming: HashMap<Uuid, i64>) -> HashMap<Uuid, i64> {
        self.pending_read_rooms
            .retain(|room_id| match incoming.get(room_id).copied() {
                Some(0) => false,
                Some(_) => {
                    incoming.insert(*room_id, 0);
                    true
                }
                None => true,
            });
        incoming
    }

    fn merge_message_reactions(
        &self,
        incoming: HashMap<Uuid, Vec<ChatMessageReactionSummary>>,
    ) -> HashMap<Uuid, Vec<ChatMessageReactionSummary>> {
        let visible_message_ids: HashSet<Uuid> = self
            .rooms
            .iter()
            .flat_map(|(_, messages)| messages.iter().map(|message| message.id))
            .collect();
        let mut merged: HashMap<Uuid, Vec<ChatMessageReactionSummary>> = self
            .message_reactions
            .iter()
            .filter(|(message_id, _)| visible_message_ids.contains(message_id))
            .map(|(message_id, reactions)| (*message_id, reactions.clone()))
            .collect();
        for (message_id, reactions) in incoming {
            merged.insert(message_id, reactions);
        }
        merged
    }

    fn filter_messages(&self, messages: Vec<ChatMessage>) -> Vec<ChatMessage> {
        messages
            .into_iter()
            .filter(|message| !self.message_is_ignored(message))
            .collect()
    }

    fn message_is_ignored(&self, message: &ChatMessage) -> bool {
        self.ignored_user_ids.contains(&message.user_id)
    }

    /// Strip already-stored messages from any newly-ignored author.
    /// DM rooms are exempt -leaving the DM room is the way to dismiss them.
    fn refilter_local_messages(&mut self) {
        let ignored = &self.ignored_user_ids;
        for (room, messages) in &mut self.rooms {
            if room.kind == "dm" {
                continue;
            }
            messages.retain(|m| !ignored.contains(&m.user_id));
        }
        self.sync_selection();
    }
}

/// Sort key for DMs: resolves the other participant's username.
/// Must match the sort used by the nav UI (`dm_label` in `ui.rs`).
fn dm_sort_key(room: &ChatRoom, user_id: Uuid, usernames: &HashMap<Uuid, String>) -> String {
    let other_id = if room.dm_user_a == Some(user_id) {
        room.dm_user_b
    } else {
        room.dm_user_a
    };
    other_id
        .and_then(|id| usernames.get(&id))
        .map(|name| format!("@{name}"))
        .unwrap_or_else(|| "DM".to_string())
}

/// Parse `/dm @username` or `/dm username` from the composer text.
/// Returns the target username if the input matches.
fn parse_dm_command(input: &str) -> Option<&str> {
    let rest = input.strip_prefix("/dm ")?.trim_start();
    let username = rest.strip_prefix('@').unwrap_or(rest).trim();
    if username.is_empty() {
        return None;
    }
    Some(username)
}

/// Parse `/leave` from the composer text.
fn parse_leave_command(input: &str) -> bool {
    input.trim() == "/leave"
}

/// Parse `/public <slug>` or `/private <slug>` style commands.
fn parse_room_command<'a>(input: &'a str, command: &str) -> Option<&'a str> {
    let rest = input.strip_prefix(&format!("{command} "))?.trim_start();
    let slug = rest.strip_prefix('#').unwrap_or(rest).trim();
    if slug.is_empty() {
        return None;
    }
    Some(slug)
}

/// Parse `/create-room <slug>` from the composer text (admin only).
fn parse_create_room_command(input: &str) -> Option<&str> {
    let rest = input.strip_prefix("/create-room ")?.trim_start();
    let slug = rest.strip_prefix('#').unwrap_or(rest).trim();
    if slug.is_empty() {
        return None;
    }
    Some(slug)
}

/// Parse `/delete-room <slug>` from the composer text (admin only).
fn parse_delete_room_command(input: &str) -> Option<&str> {
    let rest = input.strip_prefix("/delete-room ")?.trim_start();
    let slug = rest.strip_prefix('#').unwrap_or(rest).trim();
    if slug.is_empty() {
        return None;
    }
    Some(slug)
}

fn parse_subcommand<'a>(input: &'a str, command: &str) -> Option<Option<&'a str>> {
    let rest = input.strip_prefix(command)?;
    let rest = match rest.chars().next() {
        None => return Some(None),
        Some(c) if c.is_whitespace() => rest.trim(),
        Some(_) => return None,
    };
    if rest.is_empty() {
        Some(None)
    } else {
        Some(Some(rest))
    }
}

/// Parse `/fill-room <slug>` from the composer text (admin only).
fn parse_fill_room_command(input: &str) -> Option<&str> {
    let rest = input.strip_prefix("/fill-room ")?.trim_start();
    let slug = rest.strip_prefix('#').unwrap_or(rest).trim();
    if slug.is_empty() {
        return None;
    }
    Some(slug)
}

fn room_slug_for(rooms: &[(ChatRoom, Vec<ChatMessage>)], room_id: Uuid) -> Option<String> {
    rooms
        .iter()
        .find(|(room, _)| room.id == room_id)
        .and_then(|(room, _)| room.slug.clone())
}

fn unknown_slash_command(input: &str) -> Option<&str> {
    let trimmed = input.trim();
    if trimmed.is_empty() || trimmed.contains('\n') || !trimmed.starts_with('/') {
        return None;
    }

    let command = trimmed.split_whitespace().next()?;
    if command.len() <= 1 || command == "//" {
        return None;
    }

    Some(command)
}

fn online_username_set(active_users: Option<&ActiveUsers>) -> HashSet<String> {
    let Some(active_users) = active_users else {
        return HashSet::new();
    };
    let guard = active_users.lock_recover();
    guard
        .values()
        .map(|u| u.username.to_ascii_lowercase())
        .collect()
}

pub(crate) fn rank_mention_matches(
    all_usernames: &[String],
    query_lower: &str,
    online_set: impl FnOnce() -> HashSet<String>,
) -> Vec<MentionMatch> {
    // Lowercase each candidate once and keep it paired with the original
    // display name; reused for the prefix filter, the online lookup, and the
    // alphabetical tie-breaker.
    let mut filtered: Vec<(String, String)> = all_usernames
        .iter()
        .filter_map(|name| {
            let lower = name.to_ascii_lowercase();
            lower
                .starts_with(query_lower)
                .then(|| (lower, name.clone()))
        })
        .collect();
    if filtered.is_empty() {
        return Vec::new();
    }

    let online = online_set();
    let mut matches: Vec<(String, MentionMatch)> = filtered
        .drain(..)
        .map(|(lower, name)| {
            let is_online = online.contains(&lower);
            (
                lower,
                MentionMatch {
                    name,
                    online: is_online,
                },
            )
        })
        .collect();
    matches.sort_by(|(a_lower, a), (b_lower, b)| {
        b.online.cmp(&a.online).then_with(|| a_lower.cmp(b_lower))
    });
    matches.into_iter().map(|(_, m)| m).collect()
}

fn format_active_user_lines(active_users: Option<&ActiveUsers>) -> Vec<String> {
    let Some(active_users) = active_users else {
        return vec!["Active user list unavailable".to_string()];
    };

    let guard = active_users.lock_recover();
    if guard.is_empty() {
        return vec!["No active users".to_string()];
    }

    let mut users: Vec<&ActiveUser> = guard.values().collect();
    users.sort_by_key(|user| user.username.to_ascii_lowercase());
    users
        .into_iter()
        .map(|user| {
            if user.connection_count > 1 {
                format!("@{} ({} sessions)", user.username, user.connection_count)
            } else {
                format!("@{}", user.username)
            }
        })
        .collect()
}

fn annotate_staff_user_lines(
    lines: Vec<String>,
    session_registry: Option<&SessionRegistry>,
    paired_client_registry: Option<&PairedClientRegistry>,
) -> Vec<String> {
    let Some(session_registry) = session_registry else {
        return lines;
    };

    let sessions_by_username = live_sessions_by_username(session_registry);
    if sessions_by_username.is_empty() {
        return lines;
    }

    let mut annotated = Vec::with_capacity(lines.len());
    let mut in_staff_list = false;
    for line in lines {
        if line.starts_with("All Users (") {
            in_staff_list = true;
            annotated.push(line);
            continue;
        }
        if !in_staff_list {
            annotated.push(line);
            continue;
        }
        let Some(username) = staff_overlay_username(&line) else {
            annotated.push(line);
            continue;
        };

        let Some(sessions) = sessions_by_username.get(&username) else {
            annotated.push(line);
            continue;
        };

        let mut header = line;
        let session_count = sessions.len();
        header.push_str(&format!(
            " · online now · {} live {}",
            session_count,
            if session_count == 1 {
                "session"
            } else {
                "sessions"
            }
        ));
        annotated.push(header);
        annotated.extend(sessions.iter().map(|session| {
            format!(
                "  {}",
                format_live_session_line(session, paired_client_registry)
            )
        }));
    }
    annotated
}

fn format_control_center_user_list_lines(
    users: &[StaffUserRecord],
    session_registry: Option<&SessionRegistry>,
    selected_user_id: Option<Uuid>,
) -> Vec<String> {
    if users.is_empty() {
        return vec!["Loading users...".to_string()];
    }

    users
        .iter()
        .map(|user| {
            let marker = if Some(user.user_id) == selected_user_id {
                ">"
            } else {
                " "
            };
            let sessions = session_registry
                .map(|registry| registry.sessions_for_user(user.user_id))
                .unwrap_or_default();
            let mut summary = control_center_user_statuses(user);
            if !sessions.is_empty() {
                summary.push("online now".to_string());
                summary.push(format!(
                    "{} live {}",
                    sessions.len(),
                    if sessions.len() == 1 {
                        "session"
                    } else {
                        "sessions"
                    }
                ));
            }
            let label = match control_center_user_role_label(user) {
                Some(role_label) => {
                    format!("{} {role_label}", control_center_user_label(user))
                }
                None => control_center_user_label(user),
            };
            if summary.is_empty() {
                format!("{marker} {label}")
            } else {
                format!("{marker} {label} · {}", summary.join(" · "))
            }
        })
        .collect()
}

fn control_center_selected_user(
    users: &[StaffUserRecord],
    selected_user_id: Option<Uuid>,
) -> Option<&StaffUserRecord> {
    selected_user_id
        .and_then(|user_id| users.iter().find(|user| user.user_id == user_id))
        .or_else(|| users.first())
}

fn format_control_center_user_detail_lines(
    users: &[StaffUserRecord],
    selected_user: Option<&StaffUserRecord>,
    live_session_count: usize,
    can_admin_disconnect: bool,
) -> Vec<String> {
    if users.is_empty() {
        return vec![
            "Loading staff user directory...".to_string(),
            String::new(),
            "Staff user details will populate here once the snapshot arrives.".to_string(),
        ];
    }

    let Some(user) = selected_user else {
        return vec!["No user selected".to_string()];
    };

    let mut lines = vec![
        control_center_user_label(user),
        String::new(),
        format!(
            "role: {}",
            if user.is_admin {
                "administrator"
            } else if user.is_moderator {
                "moderator"
            } else {
                "member"
            }
        ),
        format!("live sessions: {}", live_session_count),
        format!(
            "server ban: {}",
            if user.active_server_ban.is_some() {
                "active"
            } else {
                "clear"
            }
        ),
    ];
    if let Some(ban) = &user.active_server_ban {
        let actor = ban
            .actor_username
            .as_ref()
            .map(|u| format!("@{u}"))
            .unwrap_or_else(|| "unknown".to_string());
        let reason = if ban.reason.trim().is_empty() {
            "(no reason recorded)".to_string()
        } else {
            ban.reason.clone()
        };
        let expires = match ban.expires_at {
            None => "permanent".to_string(),
            Some(expires_at) => format_relative_future(expires_at, chrono::Utc::now()),
        };
        lines.push(format!("  reason: {reason}"));
        lines.push(format!("  banned by: {actor}"));
        lines.push(format!(
            "  banned at: {}",
            ban.created.format("%Y-%m-%d %H:%M UTC")
        ));
        lines.push(format!("  expires: {expires}"));
    }
    lines.push(format!(
        "status: {}",
        if live_session_count == 0 {
            "offline"
        } else {
            "online now"
        }
    ));
    lines.push(String::new());
    lines.push(if can_admin_disconnect {
        "Actions next: disconnect all sessions, ban or unban from the user list, or target one session from the live-session pane.".to_string()
    } else {
        "Admin actions unavailable in moderator view.".to_string()
    });
    lines
}

fn format_relative_future(
    when: chrono::DateTime<chrono::Utc>,
    now: chrono::DateTime<chrono::Utc>,
) -> String {
    let delta = when.signed_duration_since(now);
    if delta.num_seconds() <= 0 {
        return "expired".to_string();
    }
    let days = delta.num_days();
    if days >= 1 {
        let hours = delta.num_hours() - days * 24;
        if hours > 0 {
            return format!("in {days}d {hours}h");
        }
        return format!("in {days}d");
    }
    let hours = delta.num_hours();
    if hours >= 1 {
        let minutes = delta.num_minutes() - hours * 60;
        if minutes > 0 {
            return format!("in {hours}h {minutes}m");
        }
        return format!("in {hours}h");
    }
    let minutes = delta.num_minutes();
    if minutes >= 1 {
        return format!("in {minutes}m");
    }
    "in <1m".to_string()
}

fn format_control_center_user_session_lines(
    sessions: &[LiveSessionSnapshot],
    paired_client_registry: Option<&PairedClientRegistry>,
    selected_session_id: Option<Uuid>,
    can_admin_disconnect: bool,
) -> Vec<String> {
    if sessions.is_empty() {
        return vec![
            "No live sessions".to_string(),
            String::new(),
            if can_admin_disconnect {
                "Select a user with active sessions to target one session.".to_string()
            } else {
                "Live sessions will appear here when the selected user is online.".to_string()
            },
        ];
    }

    let mut lines = vec!["Live Session Detail".to_string(), String::new()];
    lines.extend(sessions.iter().map(|session| {
        let marker = if Some(session.session_id) == selected_session_id {
            ">"
        } else {
            " "
        };
        format!(
            "{marker} {}",
            format_live_session_line(session, paired_client_registry)
        )
    }));
    if can_admin_disconnect {
        lines.push(String::new());
        lines
            .push("Actions: x disconnect selected session · b ban user · u unban user".to_string());
    }
    lines
}

fn format_control_center_staff_list_lines(
    users: &[StaffUserRecord],
    selected_staff_id: Option<Uuid>,
) -> Vec<String> {
    let staff: Vec<&StaffUserRecord> = users
        .iter()
        .filter(|user| user.is_admin || user.is_moderator)
        .collect();
    if staff.is_empty() {
        return vec!["No moderators or admins".to_string()];
    }
    staff
        .iter()
        .map(|user| {
            let marker = if Some(user.user_id) == selected_staff_id {
                ">"
            } else {
                " "
            };
            let role = if user.is_admin { "a" } else { "m" };
            format!("{marker} {} {}", control_center_user_label(user), role)
        })
        .collect()
}

fn format_control_center_staff_detail_lines(
    users: &[StaffUserRecord],
    selected_staff_id: Option<Uuid>,
) -> Vec<String> {
    let staff: Vec<&StaffUserRecord> = users
        .iter()
        .filter(|user| user.is_admin || user.is_moderator)
        .collect();
    if staff.is_empty() {
        return vec!["No staff to display".to_string()];
    }
    let selected = selected_staff_id
        .and_then(|id| staff.iter().find(|user| user.user_id == id).copied())
        .or_else(|| staff.first().copied());
    let Some(user) = selected else {
        return vec!["No staffer selected".to_string()];
    };
    vec![
        control_center_user_label(user),
        String::new(),
        format!(
            "role: {}",
            if user.is_admin {
                "administrator"
            } else {
                "moderator"
            }
        ),
    ]
}

fn control_center_user_label(user: &StaffUserRecord) -> String {
    if user.username.trim().is_empty() {
        "<unnamed>".to_string()
    } else {
        format!("@{}", user.username)
    }
}

fn format_audit_actor(entry: &AuditLogEntry) -> String {
    entry
        .actor_username
        .as_deref()
        .map(|name| format!("@{name}"))
        .unwrap_or_else(|| "<unknown>".to_string())
}

fn format_audit_target(entry: &AuditLogEntry) -> String {
    match (entry.target_id, entry.target_username.as_deref()) {
        (None, _) => "—".to_string(),
        (Some(_), Some(name)) => format!("@{name}"),
        (Some(_), None) => "@<unknown>".to_string(),
    }
}

/// Parsed audit-log filter built from the Audit tab's text input.
///
/// Tokens are whitespace-separated. A token of the form `key:value` is
/// structured (supported keys: `actor`, `target`, `action`, `kind`,
/// `since`, `until`); any bare token becomes a free-text fragment matched
/// case-insensitively against actor / target / action / kind. An empty
/// filter matches every entry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AuditFilter {
    actor: Option<String>,
    target: Option<String>,
    action: Option<String>,
    kind: Option<String>,
    since: Option<chrono::DateTime<chrono::Utc>>,
    until: Option<chrono::DateTime<chrono::Utc>>,
    free_text: Vec<String>,
}

impl AuditFilter {
    pub fn is_empty(&self) -> bool {
        self.actor.is_none()
            && self.target.is_none()
            && self.action.is_none()
            && self.kind.is_none()
            && self.since.is_none()
            && self.until.is_none()
            && self.free_text.is_empty()
    }
}

pub fn parse_audit_filter(input: &str) -> AuditFilter {
    let mut out = AuditFilter::default();
    for raw in input.split_whitespace() {
        if let Some((key, value)) = raw.split_once(':') {
            let value = value.trim();
            if value.is_empty() {
                continue;
            }
            match key.to_ascii_lowercase().as_str() {
                "actor" => out.actor = Some(strip_at(value).to_ascii_lowercase()),
                "target" => out.target = Some(strip_at(value).to_ascii_lowercase()),
                "action" => out.action = Some(value.to_ascii_lowercase()),
                "kind" => out.kind = Some(value.to_ascii_lowercase()),
                "since" => {
                    if let Some(dt) = parse_filter_date_start(value) {
                        out.since = Some(dt);
                    }
                }
                "until" => {
                    if let Some(dt) = parse_filter_date_end(value) {
                        out.until = Some(dt);
                    }
                }
                _ => out.free_text.push(raw.to_ascii_lowercase()),
            }
        } else {
            out.free_text.push(raw.to_ascii_lowercase());
        }
    }
    out
}

fn strip_at(value: &str) -> &str {
    value.strip_prefix('@').unwrap_or(value)
}

fn parse_filter_date_start(value: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    use chrono::{NaiveDate, NaiveTime, TimeZone, Utc};
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()?;
    let dt = date.and_time(NaiveTime::from_hms_opt(0, 0, 0)?);
    Utc.from_local_datetime(&dt).single()
}

fn parse_filter_date_end(value: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    use chrono::{Days, NaiveDate, NaiveTime, TimeZone, Utc};
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()?;
    // until: is exclusive end-of-day so that until:2026-04-20 includes the 20th.
    let next = date.checked_add_days(Days::new(1))?;
    let dt = next.and_time(NaiveTime::from_hms_opt(0, 0, 0)?);
    Utc.from_local_datetime(&dt).single()
}

fn audit_entry_matches(entry: &AuditLogEntry, filter: &AuditFilter) -> bool {
    if filter.is_empty() {
        return true;
    }
    if let Some(want) = filter.actor.as_deref() {
        let actor = entry
            .actor_username
            .as_deref()
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();
        if actor != want {
            return false;
        }
    }
    if let Some(want) = filter.target.as_deref() {
        let target = entry
            .target_username
            .as_deref()
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();
        if target != want {
            return false;
        }
    }
    if let Some(want) = filter.action.as_deref()
        && !entry.action.to_ascii_lowercase().contains(want)
    {
        return false;
    }
    if let Some(want) = filter.kind.as_deref()
        && entry.target_kind.to_ascii_lowercase() != want
    {
        return false;
    }
    if let Some(since) = filter.since
        && entry.created < since
    {
        return false;
    }
    if let Some(until) = filter.until
        && entry.created >= until
    {
        return false;
    }
    if !filter.free_text.is_empty() {
        let haystack = audit_entry_haystack(entry);
        for needle in &filter.free_text {
            if !haystack.contains(needle) {
                return false;
            }
        }
    }
    true
}

fn audit_entry_haystack(entry: &AuditLogEntry) -> String {
    let mut buf = String::new();
    buf.push_str(&entry.action.to_ascii_lowercase());
    buf.push(' ');
    buf.push_str(&entry.target_kind.to_ascii_lowercase());
    if let Some(name) = entry.actor_username.as_deref() {
        buf.push(' ');
        buf.push_str(&name.to_ascii_lowercase());
    }
    if let Some(name) = entry.target_username.as_deref() {
        buf.push(' ');
        buf.push_str(&name.to_ascii_lowercase());
    }
    buf
}

fn format_control_center_audit_list_lines(
    entries: &[&AuditLogEntry],
    selected_audit_id: Option<Uuid>,
) -> Vec<String> {
    if entries.is_empty() {
        return vec!["No audit entries".to_string()];
    }
    entries
        .iter()
        .map(|&entry| {
            let marker = if Some(entry.id) == selected_audit_id {
                ">"
            } else {
                " "
            };
            format!(
                "{marker} {when}  {action:<22} {target} by {actor}",
                when = entry.created.format("%Y-%m-%d %H:%M"),
                action = entry.action,
                target = format_audit_target(entry),
                actor = format_audit_actor(entry),
            )
        })
        .collect()
}

fn format_control_center_audit_detail_lines(
    entries: &[&AuditLogEntry],
    selected_audit_id: Option<Uuid>,
) -> Vec<String> {
    if entries.is_empty() {
        return vec!["No audit entries".to_string()];
    }
    let selected = selected_audit_id
        .and_then(|id| entries.iter().find(|entry| entry.id == id).copied())
        .or_else(|| entries.first().copied());
    let Some(entry) = selected else {
        return vec!["No entry selected".to_string()];
    };
    let mut lines = vec![
        format!("action      : {}", entry.action),
        format!("actor       : {}", format_audit_actor(entry)),
        format!("target      : {}", format_audit_target(entry)),
        format!("target_kind : {}", entry.target_kind),
        format!(
            "when        : {}",
            entry.created.format("%Y-%m-%d %H:%M:%S UTC")
        ),
        String::new(),
        "metadata:".to_string(),
    ];
    if let Some(map) = entry.metadata.as_object() {
        if map.is_empty() {
            lines.push("  (none)".to_string());
        } else {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            for key in keys {
                let value = &map[key];
                let rendered = match value {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Null => "null".to_string(),
                    other => other.to_string(),
                };
                lines.push(format!("  {key}: {rendered}"));
            }
        }
    } else {
        lines.push(format!("  {}", entry.metadata));
    }
    lines
}

fn control_center_user_role_label(user: &StaffUserRecord) -> Option<String> {
    let mut roles = Vec::new();
    if user.is_admin {
        roles.push("admin");
    }
    if user.is_moderator {
        roles.push("mod");
    }
    (!roles.is_empty()).then(|| format!("[{}]", roles.join(", ")))
}

fn control_center_user_statuses(user: &StaffUserRecord) -> Vec<String> {
    let mut flags = Vec::new();
    if user.active_server_ban.is_some() {
        flags.push("banned".to_string());
    }
    flags
}

fn control_center_selected_room(
    rooms: &[StaffRoomRecord],
    selected_room_id: Option<Uuid>,
) -> Option<&StaffRoomRecord> {
    selected_room_id
        .and_then(|room_id| rooms.iter().find(|room| room.room_id == room_id))
        .or_else(|| rooms.first())
}

fn format_control_center_room_list_lines(
    rooms: &[StaffRoomRecord],
    selected_room_id: Option<Uuid>,
) -> Vec<String> {
    if rooms.is_empty() {
        return vec!["Loading rooms...".to_string()];
    }

    rooms
        .iter()
        .map(|room| {
            let marker = if Some(room.room_id) == selected_room_id {
                ">"
            } else {
                " "
            };
            let mut summary = vec![room.kind.clone(), room.visibility.clone()];
            summary.push(format!(
                "{} {}",
                room.member_count,
                if room.member_count == 1 {
                    "member"
                } else {
                    "members"
                }
            ));
            if room.permanent {
                summary.push("permanent".to_string());
            }
            if room.auto_join {
                summary.push("auto".to_string());
            }
            if room.active_ban_count > 0 {
                summary.push(format!("{} banned", room.active_ban_count));
            }
            format!(
                "{marker} {} · {}",
                control_center_room_label(room),
                summary.join(" · ")
            )
        })
        .collect()
}

fn format_control_center_room_detail_lines(
    rooms: &[StaffRoomRecord],
    selected_room: Option<&StaffRoomRecord>,
) -> Vec<String> {
    if rooms.is_empty() {
        return vec![
            "Loading room directory...".to_string(),
            String::new(),
            "Staff room inspection will populate here once the snapshot arrives.".to_string(),
        ];
    }

    let Some(room) = selected_room else {
        return vec!["No room selected".to_string()];
    };

    let mut lines = vec![
        control_center_room_label(room),
        String::new(),
        format!("kind: {}", room.kind),
        format!("visibility: {}", room.visibility),
        format!(
            "members: {} {}",
            room.member_count,
            if room.member_count == 1 {
                "user"
            } else {
                "users"
            }
        ),
        format!(
            "active room bans: {}",
            if room.active_ban_count == 0 {
                "none".to_string()
            } else {
                room.active_ban_count.to_string()
            }
        ),
        format!("permanent: {}", yes_no(room.permanent)),
        format!("auto-join: {}", yes_no(room.auto_join)),
    ];

    if let Some(language_code) = &room.language_code {
        lines.push(format!("language: {}", language_code));
    }

    lines.push(String::new());
    lines.push("Actions next: kick, ban, unban, rename, visibility, delete.".to_string());
    lines
}

fn control_center_room_label(room: &StaffRoomRecord) -> String {
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

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

fn live_sessions_by_username(
    session_registry: &SessionRegistry,
) -> HashMap<String, Vec<LiveSessionSnapshot>> {
    let mut by_username: HashMap<String, Vec<LiveSessionSnapshot>> = HashMap::new();
    for snapshot in session_registry.snapshot_all() {
        by_username
            .entry(snapshot.username.to_ascii_lowercase())
            .or_default()
            .push(snapshot);
    }
    for sessions in by_username.values_mut() {
        sessions.sort_by_key(|session| session.connected_at);
    }
    by_username
}

fn staff_overlay_username(line: &str) -> Option<String> {
    let first = line.split_whitespace().next()?;
    first
        .strip_prefix('@')
        .map(|username| username.to_ascii_lowercase())
}

fn format_live_session_line(
    session: &LiveSessionSnapshot,
    paired_client_registry: Option<&PairedClientRegistry>,
) -> String {
    let mut details = vec![
        format!("session {}", short_session_id(session.session_id)),
        format!(
            "{} connected",
            format_elapsed_compact(session.connected_at.elapsed())
        ),
    ];
    if let Some(state) =
        paired_client_registry.and_then(|registry| registry.snapshot(&session.token))
    {
        details.push(format_paired_client_state(&state));
    }
    details.join(" · ")
}

fn short_session_id(session_id: Uuid) -> String {
    session_id.to_string().chars().take(8).collect()
}

fn format_elapsed_compact(elapsed: std::time::Duration) -> String {
    let secs = elapsed.as_secs();
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 60 * 60 {
        format!("{}m", secs / 60)
    } else if secs < 60 * 60 * 24 {
        format!("{}h", secs / (60 * 60))
    } else {
        format!("{}d", secs / (60 * 60 * 24))
    }
}

fn format_paired_client_state(state: &crate::session::ClientAudioState) -> String {
    let mut details = Vec::new();
    details.push(match state.client_kind {
        crate::session::ClientKind::Browser => "paired browser".to_string(),
        crate::session::ClientKind::Cli => "paired cli".to_string(),
        crate::session::ClientKind::Unknown => "paired client".to_string(),
    });
    if state.client_kind == crate::session::ClientKind::Cli {
        let transport = match (state.ssh_mode, state.platform) {
            (crate::session::ClientSshMode::Native, crate::session::ClientPlatform::Macos) => {
                Some("native/macos")
            }
            (crate::session::ClientSshMode::Native, crate::session::ClientPlatform::Linux) => {
                Some("native/linux")
            }
            (crate::session::ClientSshMode::Native, crate::session::ClientPlatform::Android) => {
                Some("native/android")
            }
            (crate::session::ClientSshMode::Native, crate::session::ClientPlatform::Windows) => {
                Some("native/windows")
            }
            (crate::session::ClientSshMode::Old, crate::session::ClientPlatform::Macos) => {
                Some("old/macos")
            }
            (crate::session::ClientSshMode::Old, crate::session::ClientPlatform::Linux) => {
                Some("old/linux")
            }
            (crate::session::ClientSshMode::Old, crate::session::ClientPlatform::Android) => {
                Some("old/android")
            }
            (crate::session::ClientSshMode::Old, crate::session::ClientPlatform::Windows) => {
                Some("old/windows")
            }
            _ => None,
        };
        if let Some(transport) = transport {
            details.push(transport.to_string());
        }
    }
    if state.muted {
        details.push("muted".to_string());
    } else {
        details.push(format!("vol {}%", state.volume_percent));
    }
    details.join(" · ")
}

fn admin_help_lines() -> Vec<String> {
    vec![
        "/admin help".to_string(),
        "/admin users".to_string(),
        "/admin rooms".to_string(),
        "/admin mods".to_string(),
        "/admin room".to_string(),
        "/admin room rename #new".to_string(),
        "/admin room public".to_string(),
        "/admin room private".to_string(),
        "/admin room delete".to_string(),
        String::new(),
        "Moderator commands:".to_string(),
        "/mod users".to_string(),
        "/mod rooms".to_string(),
        "/mod room".to_string(),
        "/mod room kick @user".to_string(),
        "/mod room ban @user".to_string(),
        "/mod room unban @user".to_string(),
    ]
}

enum ParsedAdminRoomCommand<'a> {
    Show,
    Rename { new_slug: Option<&'a str> },
    SetVisibility { visibility: &'static str },
    Delete,
}

fn parse_admin_room_subcommand(input: &str) -> Option<ParsedAdminRoomCommand<'_>> {
    let rest = input.strip_prefix("room")?;
    let rest = match rest.chars().next() {
        None => return Some(ParsedAdminRoomCommand::Show),
        Some(c) if c.is_whitespace() => rest.trim(),
        Some(_) => return None,
    };
    if rest.is_empty() {
        return Some(ParsedAdminRoomCommand::Show);
    }

    if let Some(slug) = rest.strip_prefix("rename") {
        let slug = slug.trim();
        let slug = (!slug.is_empty())
            .then(|| slug.strip_prefix('#').unwrap_or(slug).trim())
            .filter(|slug| !slug.is_empty());
        return Some(ParsedAdminRoomCommand::Rename { new_slug: slug });
    }

    match rest {
        "public" => Some(ParsedAdminRoomCommand::SetVisibility {
            visibility: "public",
        }),
        "private" => Some(ParsedAdminRoomCommand::SetVisibility {
            visibility: "private",
        }),
        "delete" => Some(ParsedAdminRoomCommand::Delete),
        _ => None,
    }
}

fn parse_admin_room_command(input: &str) -> Option<ParsedAdminRoomCommand<'_>> {
    parse_subcommand(input, "/admin")?.and_then(parse_admin_room_subcommand)
}

enum ParsedRoomModerationCommand<'a> {
    Show,
    Action {
        action: RoomModerationAction,
        target_username: Option<&'a str>,
    },
}

fn parse_room_moderation_subcommand(input: &str) -> Option<ParsedRoomModerationCommand<'_>> {
    let rest = input.strip_prefix("room")?;
    let rest = match rest.chars().next() {
        None => return Some(ParsedRoomModerationCommand::Show),
        Some(c) if c.is_whitespace() => rest.trim(),
        Some(_) => return None,
    };
    if rest.is_empty() {
        return Some(ParsedRoomModerationCommand::Show);
    }

    let (verb, trailing) = rest
        .split_once(char::is_whitespace)
        .map(|(verb, tail)| (verb, Some(tail.trim())))
        .unwrap_or((rest, None));
    let action = match verb {
        "kick" => RoomModerationAction::Kick,
        "ban" => RoomModerationAction::Ban,
        "unban" => RoomModerationAction::Unban,
        _ => return None,
    };
    let target_username = trailing
        .filter(|tail| !tail.is_empty())
        .map(|tail| tail.strip_prefix('@').unwrap_or(tail).trim())
        .filter(|tail| !tail.is_empty());
    Some(ParsedRoomModerationCommand::Action {
        action,
        target_username,
    })
}

fn parse_room_moderation_command(input: &str) -> Option<ParsedRoomModerationCommand<'_>> {
    parse_subcommand(input, "/mod")?.and_then(parse_room_moderation_subcommand)
}

fn mod_room_lines(room_slug: &str) -> Vec<String> {
    vec![
        format!("#{}", room_slug),
        String::new(),
        "/mod room kick @user".to_string(),
        "/mod room ban @user".to_string(),
        "/mod room unban @user".to_string(),
    ]
}

fn admin_room_lines(room_slug: &str) -> Vec<String> {
    vec![
        format!("#{}", room_slug),
        String::new(),
        "/admin room rename #new".to_string(),
        "/admin room public".to_string(),
        "/admin room private".to_string(),
        "/admin room delete".to_string(),
    ]
}

fn admin_room_success_message(
    old_slug: &str,
    new_slug: Option<&str>,
    visibility: Option<&str>,
    deleted: bool,
) -> String {
    if deleted {
        return format!("Deleted #{}", old_slug);
    }
    if let Some(new_slug) = new_slug {
        return format!("Renamed #{} to #{}", old_slug, new_slug);
    }
    if let Some(visibility) = visibility {
        return format!("Made #{} {}", old_slug, visibility);
    }
    format!("Updated #{}", old_slug)
}

fn wrapped_index(current: isize, delta: isize, len: usize) -> usize {
    (current + delta).rem_euclid(len as isize) as usize
}

fn adjacent_composer_room(
    order: &[RoomSlot],
    current_room_id: Option<Uuid>,
    delta: isize,
) -> Option<Uuid> {
    let rooms: Vec<Uuid> = order
        .iter()
        .filter_map(|slot| match slot {
            RoomSlot::Room(room_id) => Some(*room_id),
            RoomSlot::News | RoomSlot::Notifications | RoomSlot::Discover | RoomSlot::Showcase => {
                None
            }
        })
        .collect();
    if rooms.is_empty() {
        return None;
    }

    let current = current_room_id
        .and_then(|room_id| rooms.iter().position(|candidate| *candidate == room_id))
        .unwrap_or(0) as isize;
    Some(rooms[wrapped_index(current, delta, rooms.len())])
}

fn resolve_room_jump_target(targets: &[(u8, RoomSlot)], byte: u8) -> Option<RoomSlot> {
    let byte = byte.to_ascii_lowercase();
    targets
        .iter()
        .find_map(|(key, slot)| (*key == byte).then_some(*slot))
}

/// Parse `/<command>` or `/<command> [@]username`. Returns:
/// - `None` if `input` is not the given command,
/// - `Some(None)` for the bare command (caller treats as "list"),
/// - `Some(Some(username))` for the targeted form.
fn parse_user_command<'a>(input: &'a str, command: &str) -> Option<Option<&'a str>> {
    let rest = input.strip_prefix(command)?;
    let rest = match rest.chars().next() {
        None => return Some(None),
        Some(c) if c.is_whitespace() => rest.trim(),
        Some(_) => return None,
    };
    if rest.is_empty() {
        return Some(None);
    }
    let username = rest.strip_prefix('@').unwrap_or(rest).trim();
    Some((!username.is_empty()).then_some(username))
}

fn short_user_id(user_id: Uuid) -> String {
    let id = user_id.to_string();
    id[..id.len().min(8)].to_string()
}

/// Given a message list containing `current`, return the id of the message
/// that should take over the selection when `current` is deleted: prefer the
/// next index (older message, since the list is ordered newest-first), fall
/// back to the previous index if `current` was the last item, or `None` if
/// `current` is not in the list.
fn adjacent_message_id(msgs: &[ChatMessage], current: Uuid) -> Option<Uuid> {
    let idx = msgs.iter().position(|m| m.id == current)?;
    msgs.get(idx + 1)
        .map(|m| m.id)
        .or_else(|| idx.checked_sub(1).and_then(|i| msgs.get(i).map(|m| m.id)))
}

fn reply_preview_text(body: &str) -> String {
    if let Some(title) = news_reply_preview_text(body) {
        return title;
    }

    let body_without_reply_quote = match body.split_once('\n') {
        Some((first_line, rest))
            if first_line.trim().starts_with("> ") && !rest.trim().is_empty() =>
        {
            rest
        }
        _ => body,
    };

    let first_content_line = body_without_reply_quote
        .lines()
        .find_map(|line| {
            let trimmed = line.trim();
            (!trimmed.is_empty()).then_some(trimmed)
        })
        .unwrap_or("");
    let preview = strip_markdown_preview_markers(
        first_content_line
            .strip_prefix("> ")
            .unwrap_or(first_content_line)
            .trim(),
    );
    let preview: String = preview.chars().take(48).collect();
    if preview.chars().count() == 48 {
        format!("{}...", preview.trim_end())
    } else {
        preview
    }
}

pub(crate) fn new_chat_textarea() -> TextArea<'static> {
    composer::new_themed_textarea("Type a message...", WrapMode::Word, false)
}

fn news_reply_preview_text(body: &str) -> Option<String> {
    let trimmed = body.trim_start();
    if !trimmed.starts_with(NEWS_MARKER) {
        return None;
    }

    let raw = trimmed[NEWS_MARKER.len()..].trim_start();
    let title = raw
        .split(" || ")
        .next()
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .unwrap_or("news update");

    let preview: String = title.chars().take(48).collect();
    Some(if preview.chars().count() == 48 {
        format!("{}...", preview.trim_end())
    } else {
        preview
    })
}

fn strip_markdown_preview_markers(text: &str) -> String {
    let mut text = text.trim();

    if let Some(rest) = text.strip_prefix("> ") {
        text = rest.trim();
    }
    if let Some(rest) = text.strip_prefix("- ") {
        text = rest.trim();
    }

    let heading_level = text.chars().take_while(|ch| *ch == '#').count();
    if (1..=3).contains(&heading_level)
        && let Some(rest) = text[heading_level..].strip_prefix(' ')
    {
        text = rest.trim();
    }

    let digits = text.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0
        && let Some(rest) = text[digits..].strip_prefix(". ")
    {
        text = rest.trim();
    }

    let mut out = String::new();
    let mut idx = 0;
    while idx < text.len() {
        let rest = &text[idx..];

        if rest.starts_with('[')
            && let Some(bracket_pos) = rest[1..].find(']')
            && bracket_pos > 0
            && let Some(paren_inner) = rest[1 + bracket_pos + 1..].strip_prefix('(')
            && let Some(close_paren) = paren_inner.find(')')
            && close_paren > 0
        {
            out.push_str(&rest[1..1 + bracket_pos]);
            idx += 1 + bracket_pos + 2 + close_paren + 1;
            continue;
        }

        let mut stripped_marker = false;
        for marker in ["***", "**", "~~", "`", "*"] {
            if rest.starts_with(marker) {
                idx += marker.len();
                stripped_marker = true;
                break;
            }
        }
        if stripped_marker {
            continue;
        }

        let Some(ch) = rest.chars().next() else {
            break;
        };
        out.push(ch);
        idx += ch.len_utf8();
    }

    out.split_whitespace().collect::<Vec<_>>().join(" ")
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::common::theme;

    fn names(matches: &[MentionMatch]) -> Vec<&str> {
        matches.iter().map(|m| m.name.as_str()).collect()
    }

    fn online(names: &[&str]) -> HashSet<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    #[test]
    fn rank_mention_matches_orders_online_before_offline() {
        let all = vec![
            "alice".to_string(),
            "bob".to_string(),
            "carol".to_string(),
            "dave".to_string(),
        ];
        let ranked = rank_mention_matches(&all, "", || online(&["bob", "dave"]));
        assert_eq!(names(&ranked), vec!["bob", "dave", "alice", "carol"]);
        assert!(ranked[0].online && ranked[1].online);
        assert!(!ranked[2].online && !ranked[3].online);
    }

    #[test]
    fn rank_mention_matches_prefix_filter_groups_online_first() {
        // "@a" with two online and one offline 'a'-prefixed users:
        // online 'a' names come first (alphabetically), then offline.
        let all = vec![
            "alice".to_string(),
            "alex".to_string(),
            "albert".to_string(),
            "bob".to_string(),
        ];
        let ranked = rank_mention_matches(&all, "a", || online(&["alice", "alex"]));
        assert_eq!(names(&ranked), vec!["alex", "alice", "albert"]);
        assert!(ranked[0].online && ranked[1].online);
        assert!(!ranked[2].online);
    }

    #[test]
    fn rank_mention_matches_applies_prefix_filter() {
        let all = vec!["alice".to_string(), "albert".to_string(), "bob".to_string()];
        let ranked = rank_mention_matches(&all, "al", || online(&["bob"]));
        assert_eq!(names(&ranked), vec!["albert", "alice"]);
    }

    #[test]
    fn rank_mention_matches_prefix_is_case_insensitive() {
        let all = vec!["Alice".to_string(), "alBert".to_string()];
        let ranked = rank_mention_matches(&all, "al", HashSet::new);
        assert_eq!(names(&ranked), vec!["alBert", "Alice"]);
    }

    #[test]
    fn rank_mention_matches_falls_back_to_alpha_when_no_online_info() {
        let all = vec!["zed".to_string(), "alice".to_string(), "bob".to_string()];
        let ranked = rank_mention_matches(&all, "", HashSet::new);
        assert_eq!(names(&ranked), vec!["alice", "bob", "zed"]);
        assert!(ranked.iter().all(|m| !m.online));
    }

    #[test]
    fn rank_mention_matches_skips_online_set_when_prefix_excludes_all() {
        // When the query filters everyone out, the online-set supplier must
        // not be invoked — it's the expensive path (locks ActiveUsers).
        let all = vec!["alice".to_string(), "bob".to_string()];
        let ranked = rank_mention_matches(&all, "zz", || {
            panic!("online_set should not be built when prefix filter is empty")
        });
        assert!(ranked.is_empty());
    }

    #[test]
    fn online_username_set_returns_empty_for_none() {
        assert!(online_username_set(None).is_empty());
    }

    #[test]
    fn online_username_set_lowercases_active_usernames() {
        use crate::state::ActiveUser;
        use std::sync::{Arc, Mutex};
        use std::time::Instant;

        let mut users: HashMap<Uuid, ActiveUser> = HashMap::new();
        users.insert(
            Uuid::now_v7(),
            ActiveUser {
                username: "Alice".to_string(),
                connection_count: 1,
                last_login_at: Instant::now(),
            },
        );
        users.insert(
            Uuid::now_v7(),
            ActiveUser {
                username: "BOB".to_string(),
                connection_count: 2,
                last_login_at: Instant::now(),
            },
        );
        let active: ActiveUsers = Arc::new(Mutex::new(users));

        let set = online_username_set(Some(&active));
        assert_eq!(set, online(&["alice", "bob"]));
    }

    #[test]
    fn reply_preview_text_uses_message_body_for_nested_replies() {
        let preview = reply_preview_text("> @mat: original message preview\nyou like tetris?");
        assert_eq!(preview, "you like tetris?");
    }

    #[test]
    fn reply_preview_text_uses_news_title_for_news_messages() {
        let preview = reply_preview_text(
            "---NEWS--- Rust 1.95 Released || summary || https://example.com || ascii",
        );
        assert_eq!(preview, "Rust 1.95 Released");
    }

    #[test]
    fn reply_preview_text_strips_markdown_markers() {
        let preview = reply_preview_text("**bold** `@graybeard` [docs](https://late.sh)");
        assert_eq!(preview, "bold @graybeard docs");
    }

    #[test]
    fn news_marker_detection_matches_announcement_messages() {
        assert!(news_reply_preview_text("---NEWS--- title || summary || url || ascii").is_some());
        assert!(news_reply_preview_text("regular chat message").is_none());
    }

    // --- parse_dm_command ---

    #[test]
    fn parse_dm_with_at() {
        assert_eq!(parse_dm_command("/dm @alice"), Some("alice"));
    }

    #[test]
    fn parse_dm_without_at() {
        assert_eq!(parse_dm_command("/dm bob"), Some("bob"));
    }

    #[test]
    fn parse_dm_empty_username() {
        assert_eq!(parse_dm_command("/dm "), None);
        assert_eq!(parse_dm_command("/dm @"), None);
    }

    #[test]
    fn parse_dm_not_dm_command() {
        assert_eq!(parse_dm_command("hello world"), None);
        assert_eq!(parse_dm_command("/dms alice"), None);
    }

    #[test]
    fn parse_dm_trims_whitespace() {
        assert_eq!(parse_dm_command("/dm  @alice  "), Some("alice"));
    }

    #[test]
    fn new_chat_textarea_uses_theme_text_color() {
        let textarea = new_chat_textarea();
        assert_eq!(textarea.style().fg, Some(theme::TEXT()));
        assert_eq!(textarea.cursor_line_style().fg, Some(theme::TEXT()));
        assert_eq!(textarea.cursor_style().fg, Some(theme::TEXT()));
        assert_eq!(textarea.cursor_style().bg, None);
    }

    #[test]
    fn composer_cursor_visible_uses_explicit_theme_colors() {
        let mut textarea = new_chat_textarea();
        composer::set_themed_textarea_cursor_visible(&mut textarea, true);
        assert_eq!(textarea.cursor_style().fg, Some(theme::BG_CANVAS()));
        assert_eq!(textarea.cursor_style().bg, Some(theme::TEXT()));
    }

    #[test]
    fn composer_cursor_hidden_restores_plain_text_color() {
        let mut textarea = new_chat_textarea();
        composer::set_themed_textarea_cursor_visible(&mut textarea, true);
        composer::set_themed_textarea_cursor_visible(&mut textarea, false);
        assert_eq!(textarea.cursor_style().fg, Some(theme::TEXT()));
        assert_eq!(textarea.cursor_style().bg, None);
    }

    #[test]
    fn common_textarea_theme_refreshes_existing_chat_textarea_colors() {
        theme::set_current_by_id("late");
        let mut textarea = new_chat_textarea();
        let late_text = textarea.style().fg;

        theme::set_current_by_id("contrast");
        composer::apply_themed_textarea_style(&mut textarea, true);

        assert_ne!(textarea.style().fg, late_text);
        assert_eq!(textarea.style().fg, Some(theme::TEXT()));
        assert_eq!(textarea.cursor_line_style().fg, Some(theme::TEXT()));
        assert_eq!(textarea.cursor_style().fg, Some(theme::BG_CANVAS()));
        assert_eq!(textarea.cursor_style().bg, Some(theme::TEXT()));

        theme::set_current_by_id("late");
    }

    #[test]
    fn wrapped_index_wraps_forward() {
        assert_eq!(wrapped_index(2, 1, 3), 0);
        assert_eq!(wrapped_index(1, 5, 3), 0);
    }

    #[test]
    fn wrapped_index_wraps_backward() {
        assert_eq!(wrapped_index(0, -1, 3), 2);
        assert_eq!(wrapped_index(1, -5, 3), 2);
    }

    #[test]
    fn adjacent_composer_room_skips_virtual_slots() {
        let room_a = Uuid::from_u128(1);
        let room_b = Uuid::from_u128(2);
        let room_c = Uuid::from_u128(3);
        let order = vec![
            RoomSlot::Room(room_a),
            RoomSlot::News,
            RoomSlot::Notifications,
            RoomSlot::Showcase,
            RoomSlot::Discover,
            RoomSlot::Room(room_b),
            RoomSlot::Room(room_c),
        ];

        assert_eq!(
            adjacent_composer_room(&order, Some(room_a), 1),
            Some(room_b)
        );
        assert_eq!(
            adjacent_composer_room(&order, Some(room_b), -1),
            Some(room_a)
        );
        assert_eq!(
            adjacent_composer_room(&order, Some(room_c), 1),
            Some(room_a)
        );
    }

    #[test]
    fn adjacent_composer_room_returns_none_without_real_rooms() {
        let order = vec![
            RoomSlot::News,
            RoomSlot::Notifications,
            RoomSlot::Showcase,
            RoomSlot::Discover,
        ];
        assert_eq!(adjacent_composer_room(&order, None, 1), None);
    }

    #[test]
    fn room_slug_for_uses_explicit_room_id() {
        let general_id = Uuid::from_u128(11);
        let announcements_id = Uuid::from_u128(12);
        let rooms = vec![
            (
                ChatRoom {
                    id: general_id,
                    created: chrono::Utc::now(),
                    updated: chrono::Utc::now(),
                    kind: "general".to_string(),
                    visibility: "public".to_string(),
                    auto_join: true,
                    permanent: true,
                    slug: Some("general".to_string()),
                    language_code: None,
                    dm_user_a: None,
                    dm_user_b: None,
                },
                vec![],
            ),
            (
                ChatRoom {
                    id: announcements_id,
                    created: chrono::Utc::now(),
                    updated: chrono::Utc::now(),
                    kind: "topic".to_string(),
                    visibility: "public".to_string(),
                    auto_join: true,
                    permanent: true,
                    slug: Some("announcements".to_string()),
                    language_code: None,
                    dm_user_a: None,
                    dm_user_b: None,
                },
                vec![],
            ),
        ];

        assert_eq!(
            room_slug_for(&rooms, general_id),
            Some("general".to_string())
        );
        assert_eq!(
            room_slug_for(&rooms, announcements_id),
            Some("announcements".to_string())
        );
    }

    #[test]
    fn resolve_room_jump_target_is_case_insensitive() {
        let room_id = Uuid::from_u128(7);
        let targets = [
            (b'a', RoomSlot::Room(room_id)),
            (b's', RoomSlot::News),
            (b'd', RoomSlot::Notifications),
            (b'f', RoomSlot::Showcase),
            (b'g', RoomSlot::Discover),
        ];

        assert_eq!(
            resolve_room_jump_target(&targets, b'A'),
            Some(RoomSlot::Room(room_id))
        );
        assert_eq!(
            resolve_room_jump_target(&targets, b's'),
            Some(RoomSlot::News)
        );
        assert_eq!(
            resolve_room_jump_target(&targets, b'D'),
            Some(RoomSlot::Notifications)
        );
        assert_eq!(
            resolve_room_jump_target(&targets, b'f'),
            Some(RoomSlot::Showcase)
        );
        assert_eq!(
            resolve_room_jump_target(&targets, b'G'),
            Some(RoomSlot::Discover)
        );
        assert_eq!(resolve_room_jump_target(&targets, b'x'), None);
    }

    #[test]
    fn parse_user_command_with_username() {
        assert_eq!(
            parse_user_command("/ignore @alice", "/ignore"),
            Some(Some("alice"))
        );
        assert_eq!(
            parse_user_command("/unignore bob", "/unignore"),
            Some(Some("bob"))
        );
    }

    #[test]
    fn parse_user_command_lists_when_username_missing() {
        assert_eq!(parse_user_command("/ignore", "/ignore"), Some(None));
        assert_eq!(parse_user_command("/ignore   ", "/ignore"), Some(None));
        assert_eq!(parse_user_command("/ignore @", "/ignore"), Some(None));
        assert_eq!(parse_user_command("/unignore", "/unignore"), Some(None));
    }

    #[test]
    fn parse_user_command_rejects_non_matches() {
        assert_eq!(parse_user_command("ignore alice", "/ignore"), None);
        assert_eq!(parse_user_command("/ignored alice", "/ignore"), None);
        assert_eq!(parse_user_command("/unignored alice", "/unignore"), None);
    }

    #[test]
    fn parse_public_room_with_hash() {
        assert_eq!(
            parse_room_command("/public #lobby", "/public"),
            Some("lobby")
        );
    }

    #[test]
    fn parse_public_room_without_hash() {
        assert_eq!(
            parse_room_command("/public lobby", "/public"),
            Some("lobby")
        );
    }

    #[test]
    fn parse_private_room_with_hash() {
        assert_eq!(
            parse_room_command("/private #hideout", "/private"),
            Some("hideout")
        );
    }

    #[test]
    fn parse_private_room_empty() {
        assert_eq!(parse_room_command("/private ", "/private"), None);
        assert_eq!(parse_room_command("/private #", "/private"), None);
    }

    #[test]
    fn parse_private_room_not_command() {
        assert_eq!(parse_room_command("hello", "/private"), None);
        assert_eq!(parse_room_command("/privates foo", "/private"), None);
    }

    #[test]
    fn parse_create_room_with_hash() {
        assert_eq!(
            parse_create_room_command("/create-room #announcements"),
            Some("announcements")
        );
    }

    #[test]
    fn parse_create_room_without_hash() {
        assert_eq!(
            parse_create_room_command("/create-room announcements"),
            Some("announcements")
        );
    }

    #[test]
    fn parse_create_room_empty() {
        assert_eq!(parse_create_room_command("/create-room "), None);
        assert_eq!(parse_create_room_command("/create-room #"), None);
    }

    #[test]
    fn parse_create_room_not_command() {
        assert_eq!(parse_create_room_command("hello"), None);
        assert_eq!(parse_create_room_command("/create-rooms foo"), None);
    }

    #[test]
    fn parse_delete_room_with_hash() {
        assert_eq!(
            parse_delete_room_command("/delete-room #announcements"),
            Some("announcements")
        );
    }

    #[test]
    fn parse_delete_room_without_hash() {
        assert_eq!(
            parse_delete_room_command("/delete-room announcements"),
            Some("announcements")
        );
    }

    #[test]
    fn parse_delete_room_empty() {
        assert_eq!(parse_delete_room_command("/delete-room "), None);
    }

    #[test]
    fn parse_delete_room_not_command() {
        assert_eq!(parse_delete_room_command("hello"), None);
    }

    #[test]
    fn parse_subcommand_reads_bare_and_named_forms() {
        assert_eq!(parse_subcommand("/admin", "/admin"), Some(None));
        assert_eq!(
            parse_subcommand("/admin help", "/admin"),
            Some(Some("help"))
        );
        assert_eq!(parse_subcommand("/mod rooms", "/mod"), Some(Some("rooms")));
    }

    #[test]
    fn parse_subcommand_rejects_non_matches() {
        assert_eq!(parse_subcommand("/admins help", "/admin"), None);
        assert_eq!(parse_subcommand("admin help", "/admin"), None);
    }

    #[test]
    fn parse_admin_room_command_reads_show_and_action_forms() {
        assert!(matches!(
            parse_admin_room_command("/admin room"),
            Some(ParsedAdminRoomCommand::Show)
        ));
        assert!(matches!(
            parse_admin_room_command("/admin room rename #suite"),
            Some(ParsedAdminRoomCommand::Rename {
                new_slug: Some("suite"),
            })
        ));
        assert!(matches!(
            parse_admin_room_command("/admin room public"),
            Some(ParsedAdminRoomCommand::SetVisibility {
                visibility: "public",
            })
        ));
        assert!(matches!(
            parse_admin_room_command("/admin room delete"),
            Some(ParsedAdminRoomCommand::Delete)
        ));
    }

    #[test]
    fn parse_admin_room_command_handles_missing_target_and_rejects_non_matches() {
        assert!(matches!(
            parse_admin_room_command("/admin room rename"),
            Some(ParsedAdminRoomCommand::Rename { new_slug: None })
        ));
        assert!(parse_admin_room_command("/admin rooms").is_none());
        assert!(parse_admin_room_command("/mod room delete").is_none());
    }

    #[test]
    fn parse_room_moderation_command_reads_show_and_targeted_forms() {
        assert!(matches!(
            parse_room_moderation_command("/mod room"),
            Some(ParsedRoomModerationCommand::Show)
        ));
        assert!(matches!(
            parse_room_moderation_command("/mod room kick @alice"),
            Some(ParsedRoomModerationCommand::Action {
                action: RoomModerationAction::Kick,
                target_username: Some("alice"),
            })
        ));
        assert!(matches!(
            parse_room_moderation_command("/mod room unban bob"),
            Some(ParsedRoomModerationCommand::Action {
                action: RoomModerationAction::Unban,
                target_username: Some("bob"),
            })
        ));
    }

    #[test]
    fn parse_room_moderation_command_handles_missing_target_and_rejects_non_matches() {
        assert!(matches!(
            parse_room_moderation_command("/mod room ban"),
            Some(ParsedRoomModerationCommand::Action {
                action: RoomModerationAction::Ban,
                target_username: None,
            })
        ));
        assert!(parse_room_moderation_command("/mod rooms").is_none());
        assert!(parse_room_moderation_command("/admin room kick @alice").is_none());
    }

    #[test]
    fn parse_fill_room_with_hash() {
        assert_eq!(
            parse_fill_room_command("/fill-room #announcements"),
            Some("announcements")
        );
    }

    #[test]
    fn parse_fill_room_without_hash() {
        assert_eq!(
            parse_fill_room_command("/fill-room announcements"),
            Some("announcements")
        );
    }

    #[test]
    fn parse_fill_room_empty() {
        assert_eq!(parse_fill_room_command("/fill-room "), None);
        assert_eq!(parse_fill_room_command("/fill-room #"), None);
    }

    #[test]
    fn parse_fill_room_not_command() {
        assert_eq!(parse_fill_room_command("hello"), None);
        assert_eq!(parse_fill_room_command("/fill-rooms foo"), None);
    }

    #[test]
    fn unknown_slash_command_detects_typo() {
        assert_eq!(unknown_slash_command("/lsit"), Some("/lsit"));
        assert_eq!(unknown_slash_command("/lsit #general"), Some("/lsit"));
    }

    #[test]
    fn unknown_slash_command_ignores_regular_messages_and_multiline_text() {
        assert_eq!(unknown_slash_command("hello"), None);
        assert_eq!(unknown_slash_command("// not a command"), None);
        assert_eq!(unknown_slash_command("/bin/ls\nstill talking"), None);
    }

    #[test]
    fn format_active_user_lines_sorts_and_shows_session_counts() {
        let active_users = std::sync::Arc::new(std::sync::Mutex::new(HashMap::from([
            (
                Uuid::now_v7(),
                ActiveUser {
                    username: "zoe".to_string(),
                    connection_count: 2,
                    last_login_at: std::time::Instant::now(),
                },
            ),
            (
                Uuid::now_v7(),
                ActiveUser {
                    username: "alice".to_string(),
                    connection_count: 1,
                    last_login_at: std::time::Instant::now(),
                },
            ),
        ])));

        assert_eq!(
            format_active_user_lines(Some(&active_users)),
            vec!["@alice".to_string(), "@zoe (2 sessions)".to_string()]
        );
    }

    #[test]
    fn format_active_user_lines_handles_missing_registry() {
        assert_eq!(
            format_active_user_lines(None),
            vec!["Active user list unavailable".to_string()]
        );
    }

    #[test]
    fn annotate_staff_user_lines_adds_live_session_details_after_staff_header() {
        let registry = crate::session::SessionRegistry::new();
        let user_id = Uuid::now_v7();
        let session_id = Uuid::now_v7();
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        registry.register(crate::session::SessionRegistration {
            session_id,
            token: "tok-admin".to_string(),
            user_id,
            username: "admin_staff".to_string(),
            tx,
        });

        let lines = annotate_staff_user_lines(
            vec![
                "Online Now".to_string(),
                "  @admin_staff".to_string(),
                String::new(),
                "All Users (2)".to_string(),
                "@admin_staff [admin]".to_string(),
                "@plain_staff".to_string(),
            ],
            Some(&registry),
            None,
        );

        assert_eq!(lines[1], "  @admin_staff");
        assert_eq!(lines[3], "All Users (2)");
        assert!(
            lines[4].starts_with("@admin_staff [admin] · online now · 1 live session"),
            "expected live session annotation, got {:?}",
            lines[4]
        );
        assert!(
            lines[5].contains(&short_session_id(session_id)),
            "expected session id detail, got {:?}",
            lines[5]
        );
    }

    // --- adjacent_message_id (delete-and-advance) ---

    fn make_msg(id: Uuid) -> ChatMessage {
        ChatMessage {
            id,
            created: chrono::Utc::now(),
            updated: chrono::Utc::now(),
            room_id: Uuid::from_u128(999),
            user_id: Uuid::from_u128(999),
            body: String::new(),
        }
    }

    #[test]
    fn adjacent_message_id_returns_none_for_empty_list() {
        assert_eq!(adjacent_message_id(&[], Uuid::from_u128(1)), None);
    }

    #[test]
    fn adjacent_message_id_returns_none_when_not_in_list() {
        let msgs = vec![make_msg(Uuid::from_u128(1))];
        assert_eq!(adjacent_message_id(&msgs, Uuid::from_u128(99)), None);
    }

    #[test]
    fn adjacent_message_id_prefers_next_index_older_message() {
        // List is newest-first: [0]=newest, [1]=middle, [2]=oldest.
        // Deleting the middle should land on the oldest (idx+1).
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);
        let c = Uuid::from_u128(3);
        let msgs = vec![make_msg(a), make_msg(b), make_msg(c)];
        assert_eq!(adjacent_message_id(&msgs, b), Some(c));
    }

    #[test]
    fn adjacent_message_id_falls_back_to_previous_for_last_item() {
        // Deleting the oldest (last index) should land on the previous-older
        // message (idx-1), i.e., the next-oldest remaining.
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);
        let c = Uuid::from_u128(3);
        let msgs = vec![make_msg(a), make_msg(b), make_msg(c)];
        assert_eq!(adjacent_message_id(&msgs, c), Some(b));
    }

    #[test]
    fn adjacent_message_id_returns_none_for_sole_item() {
        let a = Uuid::from_u128(1);
        let msgs = vec![make_msg(a)];
        assert_eq!(adjacent_message_id(&msgs, a), None);
    }

    // --- dm_sort_key (regression: nav order must match UI order) ---

    fn make_dm(user_a: Uuid, user_b: Uuid) -> ChatRoom {
        ChatRoom {
            id: Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)),
            created: chrono::Utc::now(),
            updated: chrono::Utc::now(),
            kind: "dm".to_string(),
            visibility: "dm".to_string(),
            auto_join: false,
            permanent: false,
            slug: None,
            language_code: None,
            dm_user_a: Some(user_a),
            dm_user_b: Some(user_b),
        }
    }

    #[test]
    fn dm_sort_key_resolves_other_users_name() {
        let me = Uuid::from_u128(1);
        let alice = Uuid::from_u128(2);
        let bob = Uuid::from_u128(3);

        let mut usernames = HashMap::new();
        usernames.insert(me, "me".to_string());
        usernames.insert(alice, "alice".to_string());
        usernames.insert(bob, "bob".to_string());

        let room = make_dm(me, alice);
        assert_eq!(dm_sort_key(&room, me, &usernames), "@alice");

        // Works regardless of which slot I'm in
        let room = make_dm(bob, me);
        assert_eq!(dm_sort_key(&room, me, &usernames), "@bob");
    }

    #[test]
    fn dm_sort_key_orders_alphabetically_by_display_name() {
        let me = Uuid::from_u128(1);
        let alice = Uuid::from_u128(2);
        let charlie = Uuid::from_u128(3);
        let bob = Uuid::from_u128(4);

        let mut usernames = HashMap::new();
        usernames.insert(alice, "alice".to_string());
        usernames.insert(charlie, "charlie".to_string());
        usernames.insert(bob, "bob".to_string());

        let mut dms = [make_dm(me, charlie), make_dm(me, alice), make_dm(bob, me)];
        dms.sort_by_key(|r| dm_sort_key(r, me, &usernames));

        let names: Vec<_> = dms.iter().map(|r| dm_sort_key(r, me, &usernames)).collect();
        assert_eq!(names, vec!["@alice", "@bob", "@charlie"]);
    }
}

#[cfg(test)]
mod audit_filter_tests {
    use super::*;
    use chrono::TimeZone;
    use serde_json::json;

    fn entry(
        action: &str,
        target_kind: &str,
        actor: Option<&str>,
        target: Option<&str>,
        created: chrono::DateTime<chrono::Utc>,
    ) -> AuditLogEntry {
        AuditLogEntry {
            id: Uuid::from_u128(1),
            created,
            action: action.to_string(),
            target_kind: target_kind.to_string(),
            target_id: target.map(|_| Uuid::from_u128(2)),
            actor_user_id: Uuid::from_u128(3),
            actor_username: actor.map(str::to_string),
            target_username: target.map(str::to_string),
            metadata: json!({}),
        }
    }

    #[test]
    fn empty_filter_matches_all() {
        let f = parse_audit_filter("");
        assert!(f.is_empty());
        let e = entry(
            "ban_user",
            "user",
            Some("alice"),
            Some("troll"),
            chrono::Utc::now(),
        );
        assert!(audit_entry_matches(&e, &f));
    }

    #[test]
    fn actor_and_target_strip_at_and_ignore_case() {
        let f = parse_audit_filter("actor:@Alice target:troll");
        let hit = entry(
            "ban_user",
            "user",
            Some("alice"),
            Some("Troll"),
            chrono::Utc::now(),
        );
        assert!(audit_entry_matches(&hit, &f));
        let miss = entry(
            "ban_user",
            "user",
            Some("bob"),
            Some("Troll"),
            chrono::Utc::now(),
        );
        assert!(!audit_entry_matches(&miss, &f));
    }

    #[test]
    fn action_is_substring_match() {
        let f = parse_audit_filter("action:ban");
        let permaban = entry(
            "ban_user_permanent",
            "user",
            Some("a"),
            Some("t"),
            chrono::Utc::now(),
        );
        let kick = entry(
            "room_kick",
            "room",
            Some("a"),
            Some("t"),
            chrono::Utc::now(),
        );
        assert!(audit_entry_matches(&permaban, &f));
        assert!(!audit_entry_matches(&kick, &f));
    }

    #[test]
    fn since_until_bracket_creation_time() {
        let f = parse_audit_filter("since:2026-04-20 until:2026-04-22");
        let on_20 = entry(
            "x",
            "user",
            Some("a"),
            Some("t"),
            chrono::Utc.with_ymd_and_hms(2026, 4, 20, 12, 0, 0).unwrap(),
        );
        let on_22 = entry(
            "x",
            "user",
            Some("a"),
            Some("t"),
            chrono::Utc.with_ymd_and_hms(2026, 4, 22, 23, 0, 0).unwrap(),
        );
        let on_23 = entry(
            "x",
            "user",
            Some("a"),
            Some("t"),
            chrono::Utc.with_ymd_and_hms(2026, 4, 23, 0, 0, 0).unwrap(),
        );
        let before = entry(
            "x",
            "user",
            Some("a"),
            Some("t"),
            chrono::Utc
                .with_ymd_and_hms(2026, 4, 19, 23, 59, 59)
                .unwrap(),
        );
        assert!(audit_entry_matches(&on_20, &f));
        assert!(audit_entry_matches(&on_22, &f));
        assert!(!audit_entry_matches(&on_23, &f));
        assert!(!audit_entry_matches(&before, &f));
    }

    #[test]
    fn free_text_searches_actor_target_action_kind() {
        let f = parse_audit_filter("troll");
        let hit = entry(
            "delete_message",
            "message",
            Some("alice"),
            Some("troll"),
            chrono::Utc::now(),
        );
        let miss = entry(
            "delete_message",
            "message",
            Some("alice"),
            Some("evan"),
            chrono::Utc::now(),
        );
        assert!(audit_entry_matches(&hit, &f));
        assert!(!audit_entry_matches(&miss, &f));
    }

    #[test]
    fn unknown_keys_become_free_text() {
        let f = parse_audit_filter("color:blue");
        let hit = entry(
            "color:blue",
            "user",
            Some("a"),
            Some("t"),
            chrono::Utc::now(),
        );
        let miss = entry("ban", "user", Some("a"), Some("t"), chrono::Utc::now());
        assert!(audit_entry_matches(&hit, &f));
        assert!(!audit_entry_matches(&miss, &f));
    }

    #[test]
    fn malformed_dates_are_ignored() {
        let f = parse_audit_filter("since:not-a-date");
        assert!(f.since.is_none());
    }
}
