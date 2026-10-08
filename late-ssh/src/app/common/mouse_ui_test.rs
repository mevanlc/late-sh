use super::*;
use ratatui_textarea::WrapMode;

fn caret_map(
    text: &str,
    width: u16,
    height: u16,
    wrap: WrapMode,
    end: bool,
) -> (MouseState<(usize, usize), ()>, Buffer) {
    let area = Rect::new(0, 0, width, height);
    let mut buffer = Buffer::empty(area);
    let mut input = TextArea::from([text]);
    input.set_wrap_mode(wrap);
    if end {
        input.move_cursor(CursorMove::End);
    }
    (&input).render(area, &mut buffer);
    let mouse = MouseState::default();
    mouse.begin((width, height));
    text_hits(&input, area, &mouse, |row, col| (row, col));
    (mouse, buffer)
}

#[test]
fn caret_targets_follow_wide_characters_and_soft_wrapping() {
    let (mouse, buffer) = caret_map("ab界cdefgh", 6, 3, WrapMode::Glyph, false);
    assert_eq!(buffer[(4, 0)].symbol(), "c");
    assert_eq!(mouse.target(4, 0, (6, 3)), Some((0, 3)));
    assert_eq!(
        mouse.target(3, 0, (6, 3)),
        Some((0, 2)),
        "second cell of a wide glyph"
    );
    assert_eq!(buffer[(0, 1)].symbol(), "e");
    assert_eq!(mouse.target(0, 1, (6, 3)), Some((0, 5)));
    assert_eq!(
        mouse.target(5, 1, (6, 3)),
        Some((0, 9)),
        "blank tail lands at the end"
    );
}

#[test]
fn caret_targets_follow_horizontal_scroll() {
    let (mouse, buffer) = caret_map("abcdefghijklmnop", 6, 1, WrapMode::None, true);
    let visible = buffer[(0, 0)].symbol().chars().next().unwrap();
    let col = "abcdefghijklmnop".find(visible).unwrap();
    assert!(col > 0);
    assert_eq!(mouse.target(0, 0, (6, 1)), Some((0, col)));
}

#[test]
fn wrapped_buttons_and_content_publish_only_visible_targets() {
    let area = Rect::new(0, 0, 12, 4);
    let mut buffer = Buffer::empty(area);
    let mouse = MouseState::<usize, ()>::default();
    mouse.begin((12, 4));
    buttons(
        &mut Surface {
            buffer: &mut buffer,
        },
        Rect::new(1, 1, 8, 2),
        &mouse,
        &[("[Save]", 1), ("[Close]", 2)],
    );
    assert_eq!(mouse.target(1, 2, (12, 4)), Some(2));
    assert_eq!(mouse.target(9, 2, (12, 4)), None);
}
