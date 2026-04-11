use super::svc::Genre;
use crate::app::state::App;

pub fn handle_key(app: &mut App, byte: u8) -> bool {
    match byte {
        b'l' | b'L' => {
            app.vote.cast_task(Genre::Lofi);
            true
        }
        b'c' | b'C' => {
            app.vote.cast_task(Genre::Classic);
            true
        }
        b'a' | b'A' => {
            app.vote.cast_task(Genre::Ambient);
            true
        }
        // b'z' | b'Z' => {
        //     app.vote.cast_task(Genre::Jazz);
        //     true
        // }
        _ => false,
    }
}
