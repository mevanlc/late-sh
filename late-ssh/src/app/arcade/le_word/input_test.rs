use super::*;
use late_core::db::{Db, DbConfig};
use late_core::models::le_word::DailyWord;
use uuid::Uuid;

pub(crate) fn polish_state() -> State {
    let (tx, _) = tokio::sync::broadcast::channel(8);
    let svc = super::super::svc::LeWordService::new(Db::new(&DbConfig::default()).unwrap(), tx);
    let now = chrono::Utc::now();
    let word = DailyWord {
        id: Uuid::now_v7(),
        created: now,
        updated: now,
        puzzle_date: svc.today(),
        language: "pl".to_string(),
        answer_word: "żółty".to_string(),
    };
    State::new(
        Uuid::now_v7(),
        svc,
        Some(word),
        None,
        LeWordLanguage::Polish,
    )
}

#[test]
fn all_polish_accents_have_a_keyboard_only_entry_path() {
    let mut state = polish_state();
    for (key, expected) in "acelnosxz".chars().zip("ąćęłńóśźż".chars()) {
        assert!(handle_char(&mut state, ';'));
        assert!(handle_char(&mut state, key));
        assert_eq!(state.current_guess, expected.to_string());
        state.pop_letter();
    }
}

#[test]
fn accent_cancellation_and_invalid_sequences_do_not_change_the_guess() {
    let mut state = polish_state();
    state.push_letter('b');
    handle_char(&mut state, ';');
    handle_key(&mut state, 0x7f);
    assert_eq!(state.current_guess, "b");
    handle_char(&mut state, ';');
    handle_key(&mut state, 0x1b);
    assert_eq!(state.current_guess, "b");
    assert!(!state.accent_pending);
    handle_char(&mut state, ';');
    handle_char(&mut state, 'j');
    assert_eq!(state.current_guess, "b");
    assert!(!state.accent_pending);
}

#[test]
fn unicode_uppercase_and_decomposed_input_fill_five_tiles() {
    let mut state = polish_state();
    for ch in "ZO\u{301}ŁTY".chars() {
        assert!(handle_char(&mut state, ch));
    }
    assert_eq!(state.current_guess, "zółty");
    assert!(!state.push_letter('a'));
    state.pop_letter();
    assert_eq!(state.current_guess, "zółt");
}

#[test]
fn paste_normalizes_without_submitting_and_rejects_extra_words() {
    let mut state = polish_state();
    assert!(handle_event(
        &mut state,
        &ParsedInput::Paste("ŻÓŁTY".as_bytes().to_vec())
    ));
    assert_eq!(state.current_guess, "żółty");
    assert!(state.guesses.is_empty());
    assert!(!state.submission_pending());
    handle_event(&mut state, &ParsedInput::Paste(b" more".to_vec()));
    assert_eq!(state.current_guess, "żółty");
}

#[test]
fn picker_is_available_only_before_an_accepted_guess() {
    let mut state = polish_state();
    state.current_guess = "abcde".to_string();
    assert!(state.submit_guess());
    assert!(!state.language_locked());
    handle_key(&mut state, b'\t');
    assert!(state.show_language_picker);
    handle_arrow(&mut state, b'A');
    assert_eq!(state.picker_language, LeWordLanguage::English);
    handle_key(&mut state, 0x1b);
    assert!(!state.show_language_picker);
    assert_eq!(state.current_guess, "abcde");
    state.guesses.push("żółty".to_string());
    assert!(!state.choose_language(LeWordLanguage::English));
    assert_eq!(state.language, LeWordLanguage::Polish);
}
