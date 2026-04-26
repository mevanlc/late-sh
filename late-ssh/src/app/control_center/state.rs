use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Users,
    Rooms,
    Staff,
}

impl Tab {
    pub const fn label(self) -> &'static str {
        match self {
            Tab::Users => "Users",
            Tab::Rooms => "Rooms",
            Tab::Staff => "Staff",
        }
    }
}

const TAB_COUNT: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Focus {
    Tabs,
    #[default]
    UserList,
    UserSessions,
    RoomList,
    StaffList,
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
    DisconnectUser {
        user_id: Uuid,
    },
    DisconnectUserSession {
        user_id: Uuid,
        session_id: Uuid,
    },
    BanUser {
        user_id: Uuid,
        reason: String,
        expires_at: Option<DateTime<Utc>>,
    },
    UnbanUser {
        user_id: Uuid,
    },
    GrantModerator {
        user_id: Uuid,
    },
    GrantAdmin {
        user_id: Uuid,
    },
    RevokeModerator {
        user_id: Uuid,
    },
    SetRoomVisibility {
        room_id: Uuid,
        visibility: String,
    },
    DeleteRoom {
        room_id: Uuid,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BanPromptField {
    Reason,
    Duration,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BanPrompt {
    pub user_id: Uuid,
    pub username: String,
    pub reason: String,
    pub duration: String,
    pub focus: BanPromptField,
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
    selected_user_session_id: Option<Uuid>,
    selected_room_id: Option<Uuid>,
    selected_staff_id: Option<Uuid>,
    prompt: Option<Prompt>,
    ban_prompt: Option<BanPrompt>,
    pending_confirm_action: Option<PendingConfirmAction>,
}

impl State {
    pub fn selected_tab(&self) -> Tab {
        match self.selected_tab {
            1 => Tab::Rooms,
            2 => Tab::Staff,
            _ => Tab::Users,
        }
    }

    pub fn focus(&self) -> Focus {
        self.focus
    }

    pub fn focus_next(&mut self, has_user_sessions: bool) {
        self.focus = match (self.selected_tab(), self.focus, has_user_sessions) {
            (Tab::Users, Focus::UserList, true) => Focus::UserSessions,
            (Tab::Users, Focus::UserList, false) => Focus::Tabs,
            (Tab::Users, Focus::UserSessions, _) => Focus::Tabs,
            (Tab::Users, Focus::Tabs, _) => Focus::UserList,
            (Tab::Users, Focus::RoomList | Focus::StaffList, _) => Focus::UserList,
            (Tab::Rooms, Focus::RoomList, _) => Focus::Tabs,
            (Tab::Rooms, Focus::Tabs, _) => Focus::RoomList,
            (Tab::Rooms, Focus::UserList | Focus::UserSessions | Focus::StaffList, _) => {
                Focus::RoomList
            }
            (Tab::Staff, Focus::StaffList, _) => Focus::Tabs,
            (Tab::Staff, Focus::Tabs, _) => Focus::StaffList,
            (Tab::Staff, Focus::UserList | Focus::UserSessions | Focus::RoomList, _) => {
                Focus::StaffList
            }
        };
    }

    pub fn focus_prev(&mut self, has_user_sessions: bool) {
        self.focus = match (self.selected_tab(), self.focus, has_user_sessions) {
            (Tab::Users, Focus::UserList, _) => Focus::Tabs,
            (Tab::Users, Focus::UserSessions, _) => Focus::UserList,
            (Tab::Users, Focus::Tabs, true) => Focus::UserSessions,
            (Tab::Users, Focus::Tabs, false) => Focus::UserList,
            (Tab::Users, Focus::RoomList | Focus::StaffList, _) => Focus::Tabs,
            (Tab::Rooms, Focus::RoomList, _) => Focus::Tabs,
            (Tab::Rooms, Focus::Tabs, _) => Focus::RoomList,
            (Tab::Rooms, Focus::UserList | Focus::UserSessions | Focus::StaffList, _) => {
                Focus::Tabs
            }
            (Tab::Staff, Focus::StaffList, _) => Focus::Tabs,
            (Tab::Staff, Focus::Tabs, _) => Focus::StaffList,
            (Tab::Staff, Focus::UserList | Focus::UserSessions | Focus::RoomList, _) => Focus::Tabs,
        };
    }

    pub fn normalize_focus(&mut self, has_user_sessions: bool) {
        self.focus = match (self.selected_tab(), self.focus, has_user_sessions) {
            (Tab::Users, Focus::RoomList | Focus::StaffList, _) => Focus::UserList,
            (Tab::Users, Focus::UserSessions, false) => Focus::UserList,
            (Tab::Users, focus, _) => focus,
            (Tab::Rooms, Focus::UserList | Focus::UserSessions | Focus::StaffList, _) => {
                Focus::RoomList
            }
            (Tab::Rooms, focus, _) => focus,
            (Tab::Staff, Focus::UserList | Focus::UserSessions | Focus::RoomList, _) => {
                Focus::StaffList
            }
            (Tab::Staff, focus, _) => focus,
        };
    }

    pub fn focus_user_list(&mut self) {
        self.focus = Focus::UserList;
    }

    pub fn focus_room_list(&mut self) {
        self.focus = Focus::RoomList;
    }

    pub fn focus_staff_list(&mut self) {
        self.focus = Focus::StaffList;
    }

    fn clear_pending_state(&mut self) {
        self.prompt = None;
        self.ban_prompt = None;
        self.pending_confirm_action = None;
    }

    pub fn next_tab(&mut self) {
        self.selected_tab = (self.selected_tab + 1) % TAB_COUNT;
        self.normalize_focus(false);
        self.clear_pending_state();
    }

    pub fn prev_tab(&mut self) {
        self.selected_tab = (self.selected_tab + TAB_COUNT - 1) % TAB_COUNT;
        self.normalize_focus(false);
        self.clear_pending_state();
    }

    pub fn selected_room_id(&self) -> Option<Uuid> {
        self.selected_room_id
    }

    pub fn selected_user_id(&self) -> Option<Uuid> {
        self.selected_user_id
    }

    pub fn selected_user_session_id(&self) -> Option<Uuid> {
        self.selected_user_session_id
    }

    pub fn sync_user_ids(&mut self, user_ids: &[Uuid]) {
        if user_ids.is_empty() {
            self.selected_user_id = None;
            self.selected_user_session_id = None;
            self.clear_pending_state();
            return;
        }
        if self
            .selected_user_id
            .is_some_and(|user_id| user_ids.contains(&user_id))
        {
            return;
        }
        self.selected_user_id = user_ids.first().copied();
        self.selected_user_session_id = None;
        self.clear_pending_state();
    }

    pub fn move_user_selection(&mut self, user_ids: &[Uuid], delta: isize) -> bool {
        if user_ids.is_empty() {
            self.selected_user_id = None;
            self.selected_user_session_id = None;
            self.clear_pending_state();
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
        self.selected_user_session_id = None;
        self.clear_pending_state();
        changed
    }

    pub fn sync_user_session_ids(&mut self, session_ids: &[Uuid]) {
        if session_ids.is_empty() {
            self.selected_user_session_id = None;
            if self.focus == Focus::UserSessions {
                self.focus = Focus::UserList;
            }
            self.pending_confirm_action = None;
            return;
        }
        if self
            .selected_user_session_id
            .is_some_and(|session_id| session_ids.contains(&session_id))
        {
            return;
        }
        self.selected_user_session_id = session_ids.first().copied();
        self.pending_confirm_action = None;
    }

    pub fn move_user_session_selection(&mut self, session_ids: &[Uuid], delta: isize) -> bool {
        if session_ids.is_empty() {
            self.selected_user_session_id = None;
            if self.focus == Focus::UserSessions {
                self.focus = Focus::UserList;
            }
            self.pending_confirm_action = None;
            return false;
        }

        let current_index = self
            .selected_user_session_id
            .and_then(|session_id| {
                session_ids
                    .iter()
                    .position(|candidate| *candidate == session_id)
            })
            .unwrap_or(0);
        let next_index =
            ((current_index as isize + delta).rem_euclid(session_ids.len() as isize)) as usize;
        let next_session_id = session_ids[next_index];
        let changed = self.selected_user_session_id != Some(next_session_id);
        self.selected_user_session_id = Some(next_session_id);
        self.pending_confirm_action = None;
        changed
    }

    pub fn sync_room_ids(&mut self, room_ids: &[Uuid]) {
        if room_ids.is_empty() {
            self.selected_room_id = None;
            self.clear_pending_state();
            return;
        }
        if self
            .selected_room_id
            .is_some_and(|room_id| room_ids.contains(&room_id))
        {
            return;
        }
        self.selected_room_id = room_ids.first().copied();
        self.clear_pending_state();
    }

    pub fn selected_staff_id(&self) -> Option<Uuid> {
        self.selected_staff_id
    }

    pub fn sync_staff_ids(&mut self, staff_ids: &[Uuid]) {
        if staff_ids.is_empty() {
            self.selected_staff_id = None;
            return;
        }
        if self
            .selected_staff_id
            .is_some_and(|id| staff_ids.contains(&id))
        {
            return;
        }
        self.selected_staff_id = staff_ids.first().copied();
    }

    pub fn move_staff_selection(&mut self, staff_ids: &[Uuid], delta: isize) -> bool {
        if staff_ids.is_empty() {
            self.selected_staff_id = None;
            return false;
        }

        let current_index = self
            .selected_staff_id
            .and_then(|id| staff_ids.iter().position(|candidate| *candidate == id))
            .unwrap_or(0);
        let next_index =
            ((current_index as isize + delta).rem_euclid(staff_ids.len() as isize)) as usize;
        let next_id = staff_ids[next_index];
        let changed = self.selected_staff_id != Some(next_id);
        self.selected_staff_id = Some(next_id);
        changed
    }

    pub fn move_room_selection(&mut self, room_ids: &[Uuid], delta: isize) -> bool {
        if room_ids.is_empty() {
            self.selected_room_id = None;
            self.clear_pending_state();
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
        self.clear_pending_state();
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

    pub fn begin_ban_prompt(&mut self, user_id: Uuid, username: String) -> bool {
        self.prompt = None;
        self.pending_confirm_action = None;
        self.ban_prompt = Some(BanPrompt {
            user_id,
            username,
            reason: String::new(),
            duration: String::new(),
            focus: BanPromptField::Reason,
        });
        true
    }

    pub fn ban_prompt(&self) -> Option<&BanPrompt> {
        self.ban_prompt.as_ref()
    }

    pub fn is_ban_prompt_open(&self) -> bool {
        self.ban_prompt.is_some()
    }

    pub fn cancel_ban_prompt(&mut self) -> bool {
        self.ban_prompt.take().is_some()
    }

    pub fn ban_prompt_push(&mut self, ch: char) {
        if let Some(prompt) = &mut self.ban_prompt {
            ban_prompt_focused_field_mut(prompt).push(ch);
        }
    }

    pub fn ban_prompt_backspace(&mut self) {
        if let Some(prompt) = &mut self.ban_prompt {
            ban_prompt_focused_field_mut(prompt).pop();
        }
    }

    pub fn ban_prompt_delete_word_left(&mut self) {
        let Some(prompt) = &mut self.ban_prompt else {
            return;
        };
        let field = ban_prompt_focused_field_mut(prompt);
        while field.ends_with(char::is_whitespace) {
            field.pop();
        }
        while field.chars().last().is_some_and(|ch| !ch.is_whitespace()) {
            field.pop();
        }
    }

    pub fn ban_prompt_focus_next(&mut self) {
        if let Some(prompt) = &mut self.ban_prompt {
            prompt.focus = match prompt.focus {
                BanPromptField::Reason => BanPromptField::Duration,
                BanPromptField::Duration => BanPromptField::Reason,
            };
        }
    }

    pub fn take_ban_prompt(&mut self) -> Option<BanPrompt> {
        self.ban_prompt.take()
    }
}

fn ban_prompt_focused_field_mut(prompt: &mut BanPrompt) -> &mut String {
    match prompt.focus {
        BanPromptField::Reason => &mut prompt.reason,
        BanPromptField::Duration => &mut prompt.duration,
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum BanDurationParseError {
    Empty,
    MissingUnit,
    InvalidNumber,
    InvalidUnit,
    NonPositive,
    TooLarge,
}

impl std::fmt::Display for BanDurationParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = match self {
            Self::Empty => "duration is empty",
            Self::MissingUnit => "duration needs a unit (s/m/h/d)",
            Self::InvalidNumber => "duration number is invalid",
            Self::InvalidUnit => "duration unit must be s, m, h, or d",
            Self::NonPositive => "duration must be positive",
            Self::TooLarge => "duration is too large",
        };
        f.write_str(msg)
    }
}

/// Parse a ban duration of the form `<N><s|m|h|d>`.
/// An empty/whitespace-only input returns `Ok(None)` (permanent ban).
pub fn parse_ban_duration(input: &str) -> Result<Option<Duration>, BanDurationParseError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let bytes = trimmed.as_bytes();
    let unit = *bytes.last().ok_or(BanDurationParseError::Empty)?;
    if unit.is_ascii_digit() {
        return Err(BanDurationParseError::MissingUnit);
    }
    let number_str = &trimmed[..trimmed.len() - 1];
    if number_str.is_empty() {
        return Err(BanDurationParseError::InvalidNumber);
    }
    let n: i64 = number_str
        .parse()
        .map_err(|_| BanDurationParseError::InvalidNumber)?;
    if n <= 0 {
        return Err(BanDurationParseError::NonPositive);
    }
    let duration = match unit.to_ascii_lowercase() {
        b's' => Duration::try_seconds(n),
        b'm' => Duration::try_minutes(n),
        b'h' => Duration::try_hours(n),
        b'd' => Duration::try_days(n),
        _ => return Err(BanDurationParseError::InvalidUnit),
    }
    .ok_or(BanDurationParseError::TooLarge)?;
    Ok(Some(duration))
}

pub fn ban_expires_at(duration: Option<Duration>) -> Option<DateTime<Utc>> {
    duration.and_then(|d| Utc::now().checked_add_signed(d))
}

#[cfg(test)]
mod ban_duration_tests {
    use super::*;

    #[test]
    fn empty_is_permanent() {
        assert_eq!(parse_ban_duration(""), Ok(None));
        assert_eq!(parse_ban_duration("   "), Ok(None));
    }

    #[test]
    fn parses_each_unit() {
        assert_eq!(
            parse_ban_duration("30s"),
            Ok(Some(Duration::try_seconds(30).unwrap()))
        );
        assert_eq!(
            parse_ban_duration("15m"),
            Ok(Some(Duration::try_minutes(15).unwrap()))
        );
        assert_eq!(
            parse_ban_duration("24h"),
            Ok(Some(Duration::try_hours(24).unwrap()))
        );
        assert_eq!(
            parse_ban_duration("7d"),
            Ok(Some(Duration::try_days(7).unwrap()))
        );
    }

    #[test]
    fn unit_is_case_insensitive() {
        assert_eq!(
            parse_ban_duration("12H"),
            Ok(Some(Duration::try_hours(12).unwrap()))
        );
    }

    #[test]
    fn rejects_missing_unit() {
        assert_eq!(
            parse_ban_duration("42"),
            Err(BanDurationParseError::MissingUnit)
        );
    }

    #[test]
    fn rejects_invalid_unit() {
        assert_eq!(
            parse_ban_duration("10y"),
            Err(BanDurationParseError::InvalidUnit)
        );
    }

    #[test]
    fn rejects_invalid_number() {
        assert_eq!(
            parse_ban_duration("abch"),
            Err(BanDurationParseError::InvalidNumber)
        );
        assert_eq!(
            parse_ban_duration("h"),
            Err(BanDurationParseError::InvalidNumber)
        );
    }

    #[test]
    fn rejects_non_positive() {
        assert_eq!(
            parse_ban_duration("0h"),
            Err(BanDurationParseError::NonPositive)
        );
        assert_eq!(
            parse_ban_duration("-3h"),
            Err(BanDurationParseError::NonPositive)
        );
    }
}
