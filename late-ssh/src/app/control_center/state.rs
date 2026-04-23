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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomActionPrompt {
    pub action: RoomAction,
    pub target_username: String,
}

#[derive(Debug, Default)]
pub struct State {
    selected_tab: usize,
    selected_room_id: Option<Uuid>,
    room_action_prompt: Option<RoomActionPrompt>,
}

impl State {
    pub fn selected_tab(&self) -> Tab {
        match self.selected_tab {
            1 => Tab::Rooms,
            _ => Tab::Users,
        }
    }

    pub fn next_tab(&mut self) {
        self.selected_tab = (self.selected_tab + 1) % 2;
        self.room_action_prompt = None;
    }

    pub fn prev_tab(&mut self) {
        self.selected_tab = if self.selected_tab == 0 { 1 } else { 0 };
        self.room_action_prompt = None;
    }

    pub fn selected_room_id(&self) -> Option<Uuid> {
        self.selected_room_id
    }

    pub fn sync_room_ids(&mut self, room_ids: &[Uuid]) {
        if room_ids.is_empty() {
            self.selected_room_id = None;
            self.room_action_prompt = None;
            return;
        }
        if self
            .selected_room_id
            .is_some_and(|room_id| room_ids.contains(&room_id))
        {
            return;
        }
        self.selected_room_id = room_ids.first().copied();
        self.room_action_prompt = None;
    }

    pub fn move_room_selection(&mut self, room_ids: &[Uuid], delta: isize) -> bool {
        if room_ids.is_empty() {
            self.selected_room_id = None;
            self.room_action_prompt = None;
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
        self.room_action_prompt = None;
        changed
    }

    pub fn begin_room_action(&mut self, action: RoomAction) -> bool {
        if self.selected_room_id.is_none() {
            return false;
        }
        self.room_action_prompt = Some(RoomActionPrompt {
            action,
            target_username: String::new(),
        });
        true
    }

    pub fn room_action_prompt(&self) -> Option<&RoomActionPrompt> {
        self.room_action_prompt.as_ref()
    }

    pub fn is_prompt_open(&self) -> bool {
        self.room_action_prompt.is_some()
    }

    pub fn cancel_prompt(&mut self) -> bool {
        self.room_action_prompt.take().is_some()
    }

    pub fn prompt_push(&mut self, ch: char) {
        if let Some(prompt) = &mut self.room_action_prompt {
            prompt.target_username.push(ch);
        }
    }

    pub fn prompt_backspace(&mut self) {
        if let Some(prompt) = &mut self.room_action_prompt {
            prompt.target_username.pop();
        }
    }

    pub fn prompt_delete_word_left(&mut self) {
        let Some(prompt) = &mut self.room_action_prompt else {
            return;
        };
        while prompt.target_username.ends_with(char::is_whitespace) {
            prompt.target_username.pop();
        }
        while prompt
            .target_username
            .chars()
            .last()
            .is_some_and(|ch| !ch.is_whitespace())
        {
            prompt.target_username.pop();
        }
    }

    pub fn submit_prompt(&mut self) -> Option<(Uuid, RoomAction, String)> {
        let room_id = self.selected_room_id?;
        let prompt = self.room_action_prompt.take()?;
        let target_username = prompt
            .target_username
            .trim()
            .trim_start_matches('@')
            .to_string();
        Some((room_id, prompt.action, target_username))
    }
}
