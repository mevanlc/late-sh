use crate::app::arcade::le_word::state::*;

#[test]
fn score_guess_handles_duplicate_letters() {
    assert_eq!(
        score_guess("allee", "apple"),
        [
            LetterScore::Correct,
            LetterScore::Present,
            LetterScore::Absent,
            LetterScore::Absent,
            LetterScore::Correct,
        ]
    );
    assert_eq!(
        score_guess("sassy", "abyss"),
        [
            LetterScore::Present,
            LetterScore::Present,
            LetterScore::Absent,
            LetterScore::Correct,
            LetterScore::Present,
        ]
    );
}

#[test]
fn score_guess_matches_shade_screenshot_case() {
    assert_eq!(
        score_guess("wormy", "shade"),
        [
            LetterScore::Absent,
            LetterScore::Absent,
            LetterScore::Absent,
            LetterScore::Absent,
            LetterScore::Absent,
        ]
    );
    assert_eq!(
        score_guess("adieu", "shade"),
        [
            LetterScore::Present,
            LetterScore::Present,
            LetterScore::Absent,
            LetterScore::Present,
            LetterScore::Absent,
        ]
    );
    assert_eq!(
        score_guess("adeem", "shade"),
        [
            LetterScore::Present,
            LetterScore::Present,
            LetterScore::Present,
            LetterScore::Absent,
            LetterScore::Absent,
        ]
    );
    assert_eq!(
        score_guess("house", "shade"),
        [
            LetterScore::Present,
            LetterScore::Absent,
            LetterScore::Absent,
            LetterScore::Present,
            LetterScore::Correct,
        ]
    );
}

#[test]
fn score_letter_from_guesses_keeps_best_keyboard_hint() {
    let guesses = vec!["allee".to_string(), "sassy".to_string()];

    assert_eq!(
        score_letter_from_guesses(&guesses, "apple", 'a'),
        Some(LetterScore::Correct)
    );
    assert_eq!(
        score_letter_from_guesses(&guesses, "apple", 'l'),
        Some(LetterScore::Present)
    );
    assert_eq!(
        score_letter_from_guesses(&guesses, "apple", 's'),
        Some(LetterScore::Absent)
    );
    assert_eq!(score_letter_from_guesses(&guesses, "apple", 'z'), None);
}

fn yesterdays_state() -> (State, chrono::NaiveDate, chrono::NaiveDate) {
    use crate::app::arcade::le_word::svc::LeWordService;
    use late_core::db::{Db, DbConfig};
    use late_core::models::le_word::DailyWord;
    use uuid::Uuid;

    let (activity_feed, _) = tokio::sync::broadcast::channel(1);
    let svc = LeWordService::new(
        Db::new(&DbConfig::default()).expect("test db pool"),
        activity_feed,
    );
    let today = svc.today();
    let yesterday = today.pred_opt().expect("yesterday");
    let now = chrono::Utc::now();
    let state = State::new(
        Uuid::now_v7(),
        svc,
        Some(DailyWord {
            id: Uuid::now_v7(),
            created: now,
            updated: now,
            puzzle_date: yesterday,
            language: "en".to_string(),
            answer_word: "crane".to_string(),
        }),
        None,
        LeWordLanguage::English,
    );
    (state, today, yesterday)
}

fn todays_word(today: chrono::NaiveDate) -> late_core::models::le_word::DailyWord {
    let now = chrono::Utc::now();
    late_core::models::le_word::DailyWord {
        id: uuid::Uuid::now_v7(),
        created: now,
        updated: now,
        puzzle_date: today,
        language: "en".to_string(),
        answer_word: "slate".to_string(),
    }
}

/// Yesterday's word stayed on the board of a session that never reconnected,
/// so guesses went on being scored against it after the day rolled over.
#[test]
fn rolling_over_the_day_clears_yesterdays_word() {
    let (mut state, today, yesterday) = yesterdays_state();
    state.guesses.push("slate".to_string());
    assert_eq!(state.puzzle_date, yesterday);

    assert!(state.ensure_current_daily(), "the day should roll over");
    assert!(state.guesses.is_empty(), "yesterday's guesses are still up");
    assert!(
        !state.daily_word_loaded,
        "the board must not accept guesses until today's word lands"
    );

    // A fetch is in flight: no duplicate spawn on the next tick.
    assert!(!state.ensure_current_daily());

    // The word lands; only now does the round own today's date.
    let (tx, rx) = tokio::sync::oneshot::channel();
    state.word_reload_rx = Some(rx);
    tx.send(Ok((todays_word(today), None)))
        .expect("deliver word");
    assert!(state.poll_word_reload());
    assert_eq!(state.puzzle_date, today);
    assert!(state.daily_word_loaded);

    // Same day again: nothing to do.
    assert!(!state.ensure_current_daily());
}

/// A failed rollover fetch used to disable Le Word for the rest of the
/// session: the date had already advanced, so the rollover never re-ran and
/// input stayed gated on `daily_word_loaded`. The date now only advances when
/// the word lands, and a failure retries after a backoff.
#[test]
fn failed_word_fetch_retries_after_backoff() {
    let (mut state, today, yesterday) = yesterdays_state();

    // Without a runtime the fetch sender drops, standing in for a DB error.
    assert!(state.ensure_current_daily());
    assert!(state.poll_word_reload(), "the dead fetch should be noticed");
    assert_eq!(
        state.puzzle_date, yesterday,
        "a failure must not bank today"
    );
    assert!(!state.daily_word_loaded);

    // Inside the backoff window: no hammering.
    assert!(!state.ensure_current_daily());

    // Once the backoff passes, the rollover tries again...
    state.word_reload_backoff_until = Some(std::time::Instant::now());
    assert!(state.ensure_current_daily(), "the fetch should retry");

    // ...and a successful retry brings the board back for good.
    let (tx, rx) = tokio::sync::oneshot::channel();
    state.word_reload_rx = Some(rx);
    tx.send(Ok((todays_word(today), None)))
        .expect("deliver word");
    assert!(state.poll_word_reload());
    assert_eq!(state.puzzle_date, today);
    assert!(state.daily_word_loaded, "the retried word should install");
    assert!(!state.ensure_current_daily());
}

#[test]
fn polish_scoring_counts_letters_and_treats_accents_as_distinct() {
    use LetterScore::{Absent, Correct, Present};
    assert_eq!(score_guess("żółty", "żółty"), [Correct; 5]);
    assert_eq!(
        score_guess("zolty", "żółty"),
        [Absent, Absent, Absent, Correct, Correct]
    );
    assert_eq!(
        score_guess("żżżżż", "żółty"),
        [Correct, Absent, Absent, Absent, Absent]
    );
    assert_eq!(
        score_guess("żżółt", "żółty"),
        [Correct, Absent, Present, Present, Present]
    );
    let guesses = vec!["źółty".to_string(), "żółty".to_string()];
    assert_eq!(
        score_letter_from_guesses(&guesses, "żółty", 'ż'),
        Some(Correct)
    );
    assert_eq!(
        score_letter_from_guesses(&guesses, "żółty", 'ź'),
        Some(Absent)
    );
    assert_eq!(score_letter_from_guesses(&guesses, "żółty", 'z'), None);
}

#[test]
fn failed_submission_keeps_draft_and_blocks_edits_while_pending() {
    let (mut state, today, _) = yesterdays_state();
    state.install_round(todays_word(today), None);
    state.current_guess = "glass".to_string();
    assert!(state.submit_guess());
    assert!(state.submission_pending());
    assert!(!state.push_letter('a'));
    assert!(!state.pop_letter());
    assert!(!state.choose_language(LeWordLanguage::Polish));
    assert!(state.poll_submission());
    assert!(!state.submission_pending());
    assert_eq!(state.current_guess, "glass");
    assert!(!state.language_locked());
}

#[test]
fn rapid_language_switch_drops_obsolete_loads() {
    let (mut state, today, _) = yesterdays_state();
    state.install_round(todays_word(today), None);
    state.current_guess = "gl".to_string();
    assert!(state.choose_language(LeWordLanguage::Polish));
    assert!(state.current_guess.is_empty());
    let (obsolete_tx, obsolete_rx) = oneshot::channel();
    state.word_reload_rx = Some(obsolete_rx);
    assert!(state.choose_language(LeWordLanguage::English));
    assert!(obsolete_tx.send(Ok((todays_word(today), None))).is_err());
    let (tx, rx) = oneshot::channel();
    state.word_reload_rx = Some(rx);
    tx.send(Ok((todays_word(today), None))).unwrap();
    assert!(state.poll_word_reload());
    assert_eq!(state.language, LeWordLanguage::English);
}

#[test]
fn saved_attempt_language_does_not_replace_next_days_preference() {
    let (previous, today, yesterday) = yesterdays_state();
    let mut word = todays_word(today);
    word.puzzle_date = yesterday;
    let mut state = State::new(
        previous.user_id,
        previous.svc,
        Some(word),
        None,
        LeWordLanguage::Polish,
    );
    state.guesses.push("glass".to_string());
    assert_eq!(state.language, LeWordLanguage::English);
    assert!(state.language_locked());
    assert!(state.ensure_current_daily());
    assert_eq!(state.language, LeWordLanguage::Polish);
    assert!(!state.language_locked());
}
