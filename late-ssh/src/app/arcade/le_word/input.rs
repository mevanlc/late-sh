use late_core::models::le_word::LeWordLanguage;
use ratatui::layout::Rect;

use super::state::State;
use super::ui::KeyboardKey;
use crate::app::input::{MouseButton, MouseEvent, MouseEventKind, ParsedInput};

pub fn handle_key(state: &mut State, byte: u8) -> bool {
    if state.show_language_picker {
        match byte {
            b'\r' | b'\n' => {
                state.choose_language(state.picker_language);
            }
            0x1B => state.show_language_picker = false,
            b'\t' => toggle_picker(state),
            _ => {}
        }
        return true;
    }
    if state.show_rules {
        match byte {
            b'!' | b'q' | b'Q' | 0x1B => state.close_rules(),
            _ => {}
        }
        return true;
    }
    if state.accent_pending && byte == 0x1B {
        state.accent_pending = false;
        state.message.clear();
        return true;
    }
    match byte {
        b'!' => {
            state.open_rules();
            true
        }
        b'\t' => state.open_language_picker(),
        b'\r' | b'\n' => {
            state.accent_pending = false;
            state.submit_guess()
        }
        0x08 | 0x7F => state.pop_letter(),
        _ if byte.is_ascii() => handle_char(state, byte as char),
        _ => false,
    }
}

pub fn handle_char(state: &mut State, ch: char) -> bool {
    if state.show_rules || state.show_language_picker {
        return true;
    }
    if state.accent_pending {
        state.accent_pending = false;
        if let Some(accent) = polish_accent(ch) {
            return state.push_letter(accent);
        }
        state.message = "Accents: ;a ;c ;e ;l ;n ;o ;s ;x ;z".to_string();
        return true;
    }
    if ch == ';' && state.language == LeWordLanguage::Polish {
        if state.daily_word_loaded && !state.is_game_over && !state.submission_pending() {
            state.accent_pending = true;
            state.message = "Accent: a c e l n o s x z (Esc cancels)".to_string();
        }
        return true;
    }
    state.push_letter(ch)
}

fn polish_accent(ch: char) -> Option<char> {
    Some(match ch.to_ascii_lowercase() {
        'a' => 'ą',
        'c' => 'ć',
        'e' => 'ę',
        'l' => 'ł',
        'n' => 'ń',
        'o' => 'ó',
        's' => 'ś',
        'x' => 'ź',
        'z' => 'ż',
        _ => return None,
    })
}

fn toggle_picker(state: &mut State) {
    state.picker_language = match state.picker_language {
        LeWordLanguage::English => LeWordLanguage::Polish,
        LeWordLanguage::Polish => LeWordLanguage::English,
    };
}

pub fn handle_arrow(state: &mut State, key: u8) -> bool {
    if state.show_language_picker && matches!(key, b'A' | b'B' | b'C' | b'D') {
        toggle_picker(state);
    }
    matches!(key, b'A' | b'B' | b'C' | b'D')
}

pub fn handle_event(state: &mut State, event: &ParsedInput) -> bool {
    match event {
        ParsedInput::Char(ch) if !ch.is_ascii() => {
            handle_char(state, *ch);
            true
        }
        ParsedInput::Paste(bytes) => {
            if state.show_rules || state.show_language_picker {
                return true;
            }
            if let Ok(text) = std::str::from_utf8(bytes) {
                // A paste fills the row; it never submits or consumes guesses.
                let candidate =
                    super::svc::normalize_word(&format!("{}{}", state.current_guess, text.trim()));
                if candidate.chars().count() <= super::state::WORD_LEN
                    && candidate.chars().all(|ch| state.language.accepts(ch))
                    && state.daily_word_loaded
                    && !state.is_game_over
                    && !state.submission_pending()
                {
                    state.current_guess = candidate;
                    state.accent_pending = false;
                    state.message.clear();
                }
            }
            true
        }
        _ => false,
    }
}

pub fn handle_mouse(
    state: &mut State,
    area: Rect,
    mouse: MouseEvent,
    show_bottom_bar: bool,
) -> bool {
    if state.show_rules {
        return true;
    }
    if mouse.kind != MouseEventKind::Down || mouse.button != Some(MouseButton::Left) {
        return false;
    }
    let (Some(x), Some(y)) = (mouse.x.checked_sub(1), mouse.y.checked_sub(1)) else {
        return false;
    };
    let content = crate::app::arcade::ui::game_content_area(area, true, show_bottom_bar);
    if state.show_language_picker {
        if let Some(language) = super::ui::language_picker_hit_test(content, x, y) {
            state.choose_language(language);
        }
        return true;
    }
    if super::ui::language_selector_hit_test(content, x, y) {
        return state.open_language_picker();
    }
    match super::ui::keyboard_hit_test(content, state.language, x, y) {
        Some(KeyboardKey::Letter(ch)) => {
            state.accent_pending = false;
            state.push_letter(ch)
        }
        Some(KeyboardKey::Backspace) => state.pop_letter(),
        Some(KeyboardKey::Enter) => {
            state.accent_pending = false;
            state.submit_guess()
        }
        None => false,
    }
}

#[cfg(test)]
#[path = "input_test.rs"]
pub(crate) mod input_test;
