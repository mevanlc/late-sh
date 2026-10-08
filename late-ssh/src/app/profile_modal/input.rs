use super::state::MouseTarget;
use crate::app::{
    input::{MouseButton, MouseEvent, MouseEventKind, ParsedInput},
    state::App,
};

pub(crate) fn handle_input(app: &mut App, event: ParsedInput) {
    if is_close_event(&event) {
        close(app);
        return;
    }

    match event {
        ParsedInput::Byte(b'g') | ParsedInput::Char('g') => {
            app.profile_modal_state.scroll_to_top();
        }
        ParsedInput::Byte(b'G') | ParsedInput::Char('G') => {
            app.profile_modal_state.scroll_to_bottom();
        }
        ParsedInput::Byte(b'j' | b'J')
        | ParsedInput::Char('j' | 'J')
        | ParsedInput::Arrow(b'B') => {
            app.profile_modal_state.scroll_by(1);
        }
        ParsedInput::Byte(b'k' | b'K')
        | ParsedInput::Char('k' | 'K')
        | ParsedInput::Arrow(b'A') => {
            app.profile_modal_state.scroll_by(-1);
        }
        ParsedInput::Mouse(mouse) => {
            if !app.interaction_mode.mouse_enabled() {
                return;
            }
            let (Some(x), Some(y)) = (mouse.x.checked_sub(1), mouse.y.checked_sub(1)) else {
                return;
            };
            match mouse.kind {
                MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                    if app
                        .profile_modal_state
                        .mouse
                        .over_pane(x, y, app.size)
                        .is_some()
                    {
                        app.profile_modal_state.scroll_by(
                            if mouse.kind == MouseEventKind::ScrollUp {
                                -3
                            } else {
                                3
                            },
                        );
                    }
                }
                MouseEventKind::Down if mouse.button == Some(MouseButton::Left) => {
                    match app.profile_modal_state.mouse.target(x, y, app.size) {
                        Some(MouseTarget::Close) => close(app),
                        Some(MouseTarget::Copy(url)) => {
                            app.pending_clipboard = Some(url);
                            app.banner = Some(crate::app::common::primitives::Banner::success(
                                "Link copied!",
                            ));
                        }
                        Some(MouseTarget::ScrollTo(offset)) => {
                            app.profile_modal_state.scroll_to(offset)
                        }
                        None if clicked_outside(app, &mouse) => close(app),
                        None => {}
                    }
                }
                _ => {}
            }
        }
        ParsedInput::PageDown => {
            let step = (app.size.1 / 2).max(1) as i16;
            app.profile_modal_state.scroll_by(step);
        }
        ParsedInput::PageUp => {
            let step = (app.size.1 / 2).max(1) as i16;
            app.profile_modal_state.scroll_by(-step);
        }
        _ => {}
    }
}

pub(crate) fn handle_escape(app: &mut App) {
    close(app);
}

/// True for a left-button press that lands outside the modal's popup rect
/// (from the last render). SGR mouse cells are 1-indexed; the popup rect is
/// in 0-indexed frame cells, so shift the click by one before testing.
fn clicked_outside(app: &App, mouse: &MouseEvent) -> bool {
    if mouse.button != Some(MouseButton::Left)
        || !app.profile_modal_state.mouse.is_current(app.size)
    {
        return false;
    }
    let (Some(x), Some(y)) = (mouse.x.checked_sub(1), mouse.y.checked_sub(1)) else {
        return false;
    };
    let popup = app.profile_modal_state.popup_area();
    // A zero-size rect means nothing was drawn yet: don't dismiss on it.
    if popup.width == 0 || popup.height == 0 {
        return false;
    }
    !(x >= popup.x
        && x < popup.x.saturating_add(popup.width)
        && y >= popup.y
        && y < popup.y.saturating_add(popup.height))
}

fn is_close_event(event: &ParsedInput) -> bool {
    matches!(
        event,
        ParsedInput::Byte(b'q' | b'Q' | 0x1B) | ParsedInput::Char('q' | 'Q')
    )
}

fn close(app: &mut App) {
    app.show_profile_modal = false;
    app.profile_modal_state.close();
}

#[cfg(test)]
#[path = "input_test.rs"]
mod input_test;
