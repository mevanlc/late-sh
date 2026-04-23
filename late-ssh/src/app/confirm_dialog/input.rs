use crate::app::input::ParsedInput;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ConfirmDialogAction {
    Confirm,
    Cancel,
}

pub(crate) fn action_for(event: &ParsedInput) -> Option<ConfirmDialogAction> {
    match event {
        ParsedInput::Byte(b'\r' | b'\n') => Some(ConfirmDialogAction::Confirm),
        ParsedInput::Byte(0x1B) => Some(ConfirmDialogAction::Cancel),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_confirm_and_cancel_keys() {
        assert_eq!(
            action_for(&ParsedInput::Byte(b'\r')),
            Some(ConfirmDialogAction::Confirm)
        );
        assert_eq!(
            action_for(&ParsedInput::Byte(0x1B)),
            Some(ConfirmDialogAction::Cancel)
        );
    }

    #[test]
    fn leaves_name_entry_chars_for_typed_confirmation() {
        assert_eq!(action_for(&ParsedInput::Char('y')), None);
        assert_eq!(action_for(&ParsedInput::Char('n')), None);
        assert_eq!(action_for(&ParsedInput::Char('q')), None);
    }
}
