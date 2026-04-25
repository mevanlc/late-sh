use crate::app::{input::ParsedInput, state::App};

use super::state::Field;

pub fn handle_input(app: &mut App, event: ParsedInput) {
    if app.viz_config_modal_state.is_editing() {
        handle_edit_input(app, event);
        return;
    }

    if is_close_event(&event) {
        close(app);
        return;
    }

    match event {
        ParsedInput::Char('\t') | ParsedInput::Byte(b'\t') => {
            app.viz_config_modal_state.focus_next();
            return;
        }
        ParsedInput::BackTab => {
            app.viz_config_modal_state.focus_prev();
            return;
        }
        ParsedInput::Byte(b'\r') | ParsedInput::Byte(b'\n') => {
            app.viz_config_modal_state.begin_edit();
            return;
        }
        _ => {}
    }

    // Up/Down = large step (existing per-field deltas).
    // Left/Right = small step (0.01 across all numeric fields).
    // +/- mirror the large-step direction for non-arrow keyboards.
    let (delta_sign, large): (i32, bool) = match event {
        ParsedInput::Arrow(b'A') => (1, true),
        ParsedInput::Arrow(b'B') => (-1, true),
        ParsedInput::Arrow(b'C') => (1, false),
        ParsedInput::Arrow(b'D') => (-1, false),
        ParsedInput::Char('+' | '=') | ParsedInput::Byte(b'+' | b'=') => (1, true),
        ParsedInput::Char('-' | '_') | ParsedInput::Byte(b'-' | b'_') => (-1, true),
        _ => return,
    };

    let sign = delta_sign as f32;
    let small_step = 0.01_f32;

    match app.viz_config_modal_state.focused() {
        Field::Mode => {
            if delta_sign > 0 {
                app.visualizer.mode_next();
            } else {
                app.visualizer.mode_prev();
            }
        }
        Field::Gain => {
            let step = if large { 0.25 } else { small_step };
            app.visualizer.adjust_gain(sign * step);
        }
        Field::Attack => {
            let step = if large { 0.05 } else { small_step };
            app.visualizer.adjust_attack(sign * step);
        }
        Field::Release => {
            let step = if large { 0.05 } else { small_step };
            app.visualizer.adjust_release(sign * step);
        }
        Field::Tilt => {
            app.visualizer.toggle_tilt();
        }
    }
}

fn handle_edit_input(app: &mut App, event: ParsedInput) {
    match event {
        ParsedInput::Byte(b'\r') | ParsedInput::Byte(b'\n') => {
            commit_edit(app);
        }
        // Esc cancels the edit but leaves the modal open.
        ParsedInput::Byte(0x1B) => {
            app.viz_config_modal_state.cancel_edit();
        }
        ParsedInput::Byte(0x08) | ParsedInput::Byte(0x7F) => {
            app.viz_config_modal_state.pop_edit_char();
        }
        ParsedInput::Char(c) if is_number_input_char(c) => {
            app.viz_config_modal_state.push_edit_char(c);
        }
        ParsedInput::Byte(b) if is_number_input_byte(b) => {
            app.viz_config_modal_state.push_edit_char(b as char);
        }
        _ => {}
    }
}

fn is_number_input_char(c: char) -> bool {
    c.is_ascii_digit() || c == '.' || c == '-' || c == '+'
}

fn is_number_input_byte(b: u8) -> bool {
    b.is_ascii_digit() || b == b'.' || b == b'-' || b == b'+'
}

fn commit_edit(app: &mut App) {
    let snapshot = app
        .viz_config_modal_state
        .editing()
        .map(|e| (e.field, e.buffer.clone()));
    if let Some((field, buffer)) = snapshot
        && let Ok(value) = buffer.trim().parse::<f32>()
    {
        match field {
            Field::Gain => app.visualizer.set_gain(value),
            Field::Attack => app.visualizer.set_attack(value),
            Field::Release => app.visualizer.set_release(value),
            Field::Mode | Field::Tilt => {}
        }
    }
    app.viz_config_modal_state.cancel_edit();
}

pub fn handle_escape(app: &mut App) {
    if app.viz_config_modal_state.is_editing() {
        app.viz_config_modal_state.cancel_edit();
    } else {
        close(app);
    }
}

fn is_close_event(event: &ParsedInput) -> bool {
    matches!(event, ParsedInput::Byte(0x1B))
}

fn close(app: &mut App) {
    app.show_viz_config_modal = false;
}
