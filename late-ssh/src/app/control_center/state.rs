use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Users,
    Rooms,
}

impl Tab {
    pub const fn label(self) -> &'static str {
        match self {
            Tab::Users => "Users",
            Tab::Rooms => "Rooms",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Focus {
    #[default]
    Tabs,
    ActivePane,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoomAction {
    Kick,
    Ban,
    Unban,
}

impl RoomAction {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Kick => "Kick",
            Self::Ban => "Ban",
            Self::Unban => "Unban",
        }
    }

    pub const fn prompt_noun(self) -> &'static str {
        match self {
            Self::Kick => "kick",
            Self::Ban => "ban",
            Self::Unban => "unban",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdminAction {
    Rename,
}

impl AdminAction {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Rename => "Rename",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptKind {
    RoomAction(RoomAction),
    AdminAction(AdminAction),
}

impl PromptKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::RoomAction(action) => action.label(),
            Self::AdminAction(action) => action.label(),
        }
    }

    pub const fn panel_title(self) -> &'static str {
        match self {
            Self::RoomAction(_) => "Moderate User",
            Self::AdminAction(_) => "Admin Action",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PendingConfirmAction {
    DisconnectUser { user_id: Uuid },
    SetRoomVisibility { room_id: Uuid, visibility: String },
    DeleteRoom { room_id: Uuid },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prompt {
    pub kind: PromptKind,
    pub value: String,
}

#[derive(Debug, Default)]
pub struct State {
    selected_tab: usize,
    focus: Focus,
    selected_user_id: Option<Uuid>,
    selected_room_id: Option<Uuid>,
    prompt: Option<Prompt>,
    pending_confirm_action: Option<PendingConfirmAction>,
}

impl State {
    pub fn selected_tab(&self) -> Tab {
        match self.selected_tab {
            1 => Tab::Rooms,
            _ => Tab::Users,
        }
    }

    pub fn focus(&self) -> Focus {
        self.focus
    }

    pub fn focus_next(&mut self) {
        self.focus = match self.focus {
            Focus::Tabs => Focus::ActivePane,
            Focus::ActivePane => Focus::Tabs,
        };
    }

    pub fn focus_prev(&mut self) {
        self.focus_next();
    }

    pub fn next_tab(&mut self) {
        self.selected_tab = (self.selected_tab + 1) % 2;
        self.prompt = None;
        self.pending_confirm_action = None;
    }

    pub fn prev_tab(&mut self) {
        self.selected_tab = if self.selected_tab == 0 { 1 } else { 0 };
        self.prompt = None;
        self.pending_confirm_action = None;
    }

    pub fn selected_room_id(&self) -> Option<Uuid> {
        self.selected_room_id
    }

    pub fn selected_user_id(&self) -> Option<Uuid> {
        self.selected_user_id
    }

    pub fn sync_user_ids(&mut self, user_ids: &[Uuid]) {
        if user_ids.is_empty() {
            self.selected_user_id = None;
            self.prompt = None;
            self.pending_confirm_action = None;
            return;
        }
        if self
            .selected_user_id
            .is_some_and(|user_id| user_ids.contains(&user_id))
        {
            return;
        }
        self.selected_user_id = user_ids.first().copied();
        self.prompt = None;
        self.pending_confirm_action = None;
    }

    pub fn move_user_selection(&mut self, user_ids: &[Uuid], delta: isize) -> bool {
        if user_ids.is_empty() {
            self.selected_user_id = None;
            self.prompt = None;
            self.pending_confirm_action = None;
            return false;
        }

        let current_index = self
            .selected_user_id
            .and_then(|user_id| user_ids.iter().position(|candidate| *candidate == user_id))
            .unwrap_or(0);
        let next_index =
            ((current_index as isize + delta).rem_euclid(user_ids.len() as isize)) as usize;
        let next_user_id = user_ids[next_index];
        let changed = self.selected_user_id != Some(next_user_id);
        self.selected_user_id = Some(next_user_id);
        self.prompt = None;
        self.pending_confirm_action = None;
        changed
    }

    pub fn sync_room_ids(&mut self, room_ids: &[Uuid]) {
        if room_ids.is_empty() {
            self.selected_room_id = None;
            self.prompt = None;
            self.pending_confirm_action = None;
            return;
        }
        if self
            .selected_room_id
            .is_some_and(|room_id| room_ids.contains(&room_id))
        {
            return;
        }
        self.selected_room_id = room_ids.first().copied();
        self.prompt = None;
        self.pending_confirm_action = None;
    }

    pub fn move_room_selection(&mut self, room_ids: &[Uuid], delta: isize) -> bool {
        if room_ids.is_empty() {
            self.selected_room_id = None;
            self.prompt = None;
            self.pending_confirm_action = None;
            return false;
        }

        let current_index = self
            .selected_room_id
            .and_then(|room_id| room_ids.iter().position(|candidate| *candidate == room_id))
            .unwrap_or(0);
        let next_index =
            ((current_index as isize + delta).rem_euclid(room_ids.len() as isize)) as usize;
        let next_room_id = room_ids[next_index];
        let changed = self.selected_room_id != Some(next_room_id);
        self.selected_room_id = Some(next_room_id);
        self.prompt = None;
        self.pending_confirm_action = None;
        changed
    }

    pub fn begin_room_action(&mut self, action: RoomAction) -> bool {
        if self.selected_room_id.is_none() {
            return false;
        }
        self.prompt = Some(Prompt {
            kind: PromptKind::RoomAction(action),
            value: String::new(),
        });
        true
    }

    pub fn begin_admin_action(&mut self, action: AdminAction) -> bool {
        if self.selected_room_id.is_none() {
            return false;
        }
        self.prompt = Some(Prompt {
            kind: PromptKind::AdminAction(action),
            value: String::new(),
        });
        true
    }

    pub fn prompt(&self) -> Option<&Prompt> {
        self.prompt.as_ref()
    }

    pub fn is_prompt_open(&self) -> bool {
        self.prompt.is_some()
    }

    pub fn cancel_prompt(&mut self) -> bool {
        self.prompt.take().is_some()
    }

    pub fn set_pending_confirm_action(&mut self, action: PendingConfirmAction) {
        self.pending_confirm_action = Some(action);
    }

    pub fn take_pending_confirm_action(&mut self) -> Option<PendingConfirmAction> {
        self.pending_confirm_action.take()
    }

    pub fn clear_pending_confirm_action(&mut self) {
        self.pending_confirm_action = None;
    }

    pub fn prompt_push(&mut self, ch: char) {
        if let Some(prompt) = &mut self.prompt {
            prompt.value.push(ch);
        }
    }

    pub fn prompt_backspace(&mut self) {
        if let Some(prompt) = &mut self.prompt {
            prompt.value.pop();
        }
    }

    pub fn prompt_delete_word_left(&mut self) {
        let Some(prompt) = &mut self.prompt else {
            return;
        };
        while prompt.value.ends_with(char::is_whitespace) {
            prompt.value.pop();
        }
        while prompt
            .value
            .chars()
            .last()
            .is_some_and(|ch| !ch.is_whitespace())
        {
            prompt.value.pop();
        }
    }

    pub fn submit_prompt(&mut self) -> Option<(Uuid, PromptKind, String)> {
        let room_id = self.selected_room_id?;
        let prompt = self.prompt.take()?;
        Some((room_id, prompt.kind, prompt.value.trim().to_string()))
    }
}
