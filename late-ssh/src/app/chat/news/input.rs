use crate::app::state::App;

pub fn handle_composer_input(app: &mut App, byte: u8) {
    match byte {
        0x1B => {
            // Escape cancels composing and aborts any in-flight URL task.
            app.chat.news.stop_composing();
        }
        b'\r' | b'\n' => {
            app.chat.news.submit_composer();
        }
        0x7F | 0x08 => {
            app.chat.news.composer_pop();
        }
        b if (32..127).contains(&b) => {
            app.chat.news.composer_push(b as char);
        }
        _ => {}
    }
}

pub fn handle_arrow(app: &mut App, key: u8) -> bool {
    match key {
        b'A' => {
            app.chat.news.move_selection(-1);
            true
        }
        b'B' => {
            app.chat.news.move_selection(1);
            true
        }
        _ => false,
    }
}

pub fn handle_byte(app: &mut App, byte: u8) -> bool {
    match byte {
        b'i' | b'I' => {
            app.chat.news.start_composing();
            true
        }
        b'\r' | b'\n' => {
            if let Some(url) = app.chat.news.selected_url() {
                app.pending_clipboard = Some(url.to_owned());
                app.banner = Some(crate::app::common::primitives::Banner::success(
                    "Link copied!",
                ));
            }
            true
        }
        b'j' | b'J' => {
            app.chat.news.move_selection(1);
            true
        }
        b'k' | b'K' => {
            app.chat.news.move_selection(-1);
            true
        }
        b'd' | b'D' => {
            app.chat.news.delete_selected();
            true
        }
        _ => false,
    }
}
