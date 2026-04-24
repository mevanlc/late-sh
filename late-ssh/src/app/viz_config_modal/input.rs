use crate::app::{input::ParsedInput, state::App};

use super::state::Field;

pub fn handle_input(app: &mut App, event: ParsedInput) {
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
        _ => {}
    }

    // Up / Right = increment, Down / Left = decrement. +/- mirror them so
    // users on keyboards without arrow emphasis can still drive the values.
    let delta: i32 = match event {
        ParsedInput::Arrow(b'A') | ParsedInput::Arrow(b'C') => 1,
        ParsedInput::Arrow(b'B') | ParsedInput::Arrow(b'D') => -1,
        ParsedInput::Char('+' | '=') | ParsedInput::Byte(b'+' | b'=') => 1,
        ParsedInput::Char('-' | '_') | ParsedInput::Byte(b'-' | b'_') => -1,
        _ => return,
    };

    match app.viz_config_modal_state.focused() {
        Field::Mode => {
            if delta > 0 {
                app.visualizer.mode_next();
            } else {
                app.visualizer.mode_prev();
            }
        }
        Field::Gain => app.visualizer.adjust_gain(delta as f32 * 0.25),
        Field::Attack => app.visualizer.adjust_attack(delta as f32 * 0.05),
        Field::Release => app.visualizer.adjust_release(delta as f32 * 0.05),
        // Either direction flips the toggle; matches the "arrows adjust" model.
        Field::Tilt => {
            let _ = delta;
            app.visualizer.toggle_tilt();
        }
    }
}

pub fn handle_escape(app: &mut App) {
    close(app);
}

fn is_close_event(event: &ParsedInput) -> bool {
    matches!(event, ParsedInput::Byte(0x1B))
}

fn close(app: &mut App) {
    app.show_viz_config_modal = false;
}
