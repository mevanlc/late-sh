use super::*;

#[test]
fn polish_key_hints_fit_an_eighty_column_terminal() {
    let state = super::super::input::input_test::polish_state();
    for width in [30, 54, 78] {
        let hints = key_hints(&state, width);
        assert!(hints.width() <= width, "{hints}");
        assert!(hints.to_string().contains("Esc"));
        assert!(hints.to_string().contains("; accent"));
    }
}

#[test]
fn result_panel_prefers_space_below_board() {
    let board_area = Rect::new(0, 0, 80, 40);
    let board = Rect::new(28, 10, 24, 13);
    let keyboard = Rect::new(20, 25, 39, 5);
    let panel = result_panel_area(board_area, board, Some(keyboard));
    assert!(panel.y > keyboard.y + keyboard.height);
    assert_eq!(panel.width, 28);
}

#[test]
fn full_and_compact_layouts_keep_every_polish_key_clickable() {
    for height in [14, 17, 24, 40] {
        let area = Rect::new(2, 1, 80, height);
        let layout = le_word_layout(area, LeWordLanguage::Polish);
        let keyboard = layout.keyboard.expect("all four rows fit");
        let keys = keyboard_key_rects(keyboard, LeWordLanguage::Polish);
        assert_eq!(keys.len(), 37);
        for key in keys {
            assert!(key.rect.x + key.rect.width <= area.right());
            assert!(key.rect.y < area.bottom());
            assert_eq!(
                keyboard_hit_test(area, LeWordLanguage::Polish, key.rect.x, key.rect.y),
                Some(key.key)
            );
        }
        assert!(keyboard.y >= layout.board.bottom());
        assert!(layout.board.y >= layout.header.bottom());
    }
}

#[test]
fn narrow_layout_hides_whole_keyboard_instead_of_losing_accents() {
    assert!(
        le_word_layout(Rect::new(0, 0, 30, 40), LeWordLanguage::Polish)
            .keyboard
            .is_none()
    );
}

#[test]
fn polish_tiles_and_keys_render_unicode_uppercase() {
    let mut state = super::super::input::input_test::polish_state();
    state.current_guess = "żółty".to_string();
    let text = board_lines(&state, false)[0].to_string();
    assert!(text.contains(" Ż   Ó   Ł   T   Y "));
    assert_eq!(key_label(KeyboardKey::Letter('ź')), "Ź");
}

#[test]
fn language_picker_draw_and_hit_test_agree() {
    let area = Rect::new(3, 4, 80, 30);
    let modal = language_picker_area(area);
    for (i, language) in LeWordLanguage::ALL.into_iter().enumerate() {
        assert_eq!(
            language_picker_hit_test(area, modal.x + 5, modal.y + 2 + i as u16),
            Some(language)
        );
    }
}
