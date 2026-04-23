#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Permissions {
    is_admin: bool,
    is_moderator: bool,
}

impl Permissions {
    pub const fn new(is_admin: bool, is_moderator: bool) -> Self {
        Self {
            is_admin,
            is_moderator,
        }
    }

    pub const fn is_admin(self) -> bool {
        self.is_admin
    }

    pub const fn is_moderator(self) -> bool {
        self.is_moderator
    }

    pub const fn can_moderate(self) -> bool {
        self.is_admin || self.is_moderator
    }

    pub const fn can_access_admin_surface(self) -> bool {
        self.is_admin
    }

    pub const fn can_access_mod_surface(self) -> bool {
        self.can_moderate()
    }

    pub const fn can_manage_permanent_rooms(self) -> bool {
        self.is_admin
    }

    pub const fn can_post_announcements(self) -> bool {
        self.is_admin
    }

    pub const fn can_edit_message(self, is_owner: bool) -> bool {
        is_owner || self.can_moderate()
    }

    pub const fn can_delete_message(self, is_owner: bool) -> bool {
        is_owner || self.can_moderate()
    }

    pub const fn can_delete_article(self, is_owner: bool) -> bool {
        is_owner || self.can_moderate()
    }
}

#[cfg(test)]
mod tests {
    use super::Permissions;

    #[test]
    fn moderator_can_moderate_without_admin_privileges() {
        let permissions = Permissions::new(false, true);
        assert!(permissions.can_moderate());
        assert!(!permissions.can_access_admin_surface());
        assert!(permissions.can_access_mod_surface());
        assert!(!permissions.can_manage_permanent_rooms());
        assert!(!permissions.can_post_announcements());
    }

    #[test]
    fn admin_can_moderate_and_manage_admin_surfaces() {
        let permissions = Permissions::new(true, false);
        assert!(permissions.can_moderate());
        assert!(permissions.can_access_admin_surface());
        assert!(permissions.can_access_mod_surface());
        assert!(permissions.can_manage_permanent_rooms());
        assert!(permissions.can_post_announcements());
    }

    #[test]
    fn ownership_still_allows_regular_user_message_actions() {
        let permissions = Permissions::default();
        assert!(permissions.can_edit_message(true));
        assert!(permissions.can_delete_message(true));
        assert!(permissions.can_delete_article(true));
        assert!(!permissions.can_edit_message(false));
        assert!(!permissions.can_delete_message(false));
        assert!(!permissions.can_delete_article(false));
    }
}
