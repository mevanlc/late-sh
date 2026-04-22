use crate::app::{
    input::{MouseButton, MouseEventKind, ParsedInput},
    state::App,
};

use super::ui::{info_hit, swatch_hit};

pub(crate) fn handle_key(app: &mut App, byte: u8) -> bool {
    let size = app.size;
    let is_interacting = app.artboard_interacting;
    let Some(state) = app.dartboard_state.as_mut() else {
        return false;
    };

    if state.is_help_open() || state.is_glyph_picker_open() {
        let action = super::input::handle_byte(state, size, byte);
        return handle_action(app, action);
    }

    if is_interacting {
        let action = super::input::handle_byte(state, size, byte);
        return handle_action(app, action);
    }

    match byte {
        b'i' | b'I' | b'\r' | b'\n' => {
            app.activate_artboard_interaction();
            true
        }
        0x10 => {
            let action = super::input::handle_byte(state, size, byte);
            handle_action(app, action)
        }
        _ => false,
    }
}

pub(crate) fn handle_arrow(app: &mut App, key: u8) -> bool {
    let size = app.size;
    let is_interacting = app.artboard_interacting;
    let Some(state) = app.dartboard_state.as_mut() else {
        return false;
    };

    if is_interacting || state.is_help_open() || state.is_glyph_picker_open() {
        return super::input::handle_arrow(state, size, key);
    }

    match key {
        b'A' => {
            state.move_up(size);
            true
        }
        b'B' => {
            state.move_down(size);
            true
        }
        b'C' => {
            state.move_right(size);
            true
        }
        b'D' => {
            state.move_left(size);
            true
        }
        _ => false,
    }
}

pub(crate) fn handle_event(app: &mut App, event: &ParsedInput) -> bool {
    let size = app.size;
    let is_interacting = app.artboard_interacting;
    let Some(state) = app.dartboard_state.as_mut() else {
        return false;
    };

    if is_interacting || state.is_help_open() || state.is_glyph_picker_open() {
        let action = super::input::handle_event(state, size, event);
        return handle_action(app, action);
    }

    match event {
        ParsedInput::PageUp => {
            state.move_page_up(size);
            true
        }
        ParsedInput::PageDown => {
            state.move_page_down(size);
            true
        }
        ParsedInput::Home => {
            state.move_home(size);
            true
        }
        ParsedInput::End => {
            state.move_end(size);
            true
        }
        ParsedInput::Mouse(mouse)
            if matches!(
                mouse.kind,
                MouseEventKind::ScrollUp
                    | MouseEventKind::ScrollDown
                    | MouseEventKind::ScrollLeft
                    | MouseEventKind::ScrollRight
            ) =>
        {
            let action = super::input::handle_event(state, size, event);
            handle_action(app, action)
        }
        ParsedInput::Mouse(mouse)
            if matches!(mouse.kind, MouseEventKind::Down)
                && matches!(mouse.button, Some(MouseButton::Left))
                && !mouse.modifiers.shift
                && !mouse.modifiers.alt
                && !mouse.modifiers.ctrl =>
        {
            if swatch_hit(size, state, mouse.x, mouse.y).is_some()
                || info_hit(size, state, mouse.x, mouse.y)
            {
                return true;
            }
            if !state.move_to_screen_point(size, mouse.x, mouse.y) {
                return false;
            }
            app.activate_artboard_interaction();
            true
        }
        _ => false,
    }
}

fn handle_action(app: &mut App, action: super::input::InputAction) -> bool {
    match action {
        super::input::InputAction::Ignored => false,
        super::input::InputAction::Handled => true,
        super::input::InputAction::Copy(text) => {
            app.pending_clipboard = Some(text);
            true
        }
        super::input::InputAction::Leave => {
            app.deactivate_artboard_interaction();
            true
        }
    }
}
