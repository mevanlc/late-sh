use chrono::NaiveDate;
use late_core::models::le_word::{DailyWord, Game, LeWordLanguage};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use uuid::Uuid;

use super::svc::{LeWordService, Submission, normalize_word};

pub const DAILY_WIN_REWARD_CHIPS: i64 = 250;
pub const WORD_LEN: usize = 5;
pub const MAX_GUESSES: usize = 6;
pub const DAILY_DIFFICULTY_KEY: &str = "daily";
pub const LANGUAGE_RULE: &str =
    "One daily attempt across all languages. Your first guess locks your choice—choose wisely.";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum LetterScore {
    Correct,
    Present,
    Absent,
}

type LoadedRound = anyhow::Result<(DailyWord, Option<Game>)>;

pub struct State {
    pub user_id: Uuid,
    pub puzzle_date: NaiveDate,
    pub language: LeWordLanguage,
    preferred_language: LeWordLanguage,
    pub answer: String,
    pub daily_word_loaded: bool,
    pub guesses: Vec<String>,
    pub current_guess: String,
    pub is_game_over: bool,
    pub won: bool,
    pub show_rules: bool,
    pub show_language_picker: bool,
    pub picker_language: LeWordLanguage,
    pub accent_pending: bool,
    pub message: String,
    word_reload_rx: Option<oneshot::Receiver<LoadedRound>>,
    word_reload_backoff_until: Option<std::time::Instant>,
    submission_rx: Option<oneshot::Receiver<anyhow::Result<Submission>>>,
    pub svc: LeWordService,
}
const WORD_RELOAD_RETRY: std::time::Duration = std::time::Duration::from_secs(30);

impl State {
    pub fn new(
        user_id: Uuid,
        svc: LeWordService,
        daily_word: Option<DailyWord>,
        saved_game: Option<Game>,
        preferred_language: LeWordLanguage,
    ) -> Self {
        let language = daily_word
            .as_ref()
            .and_then(|word| LeWordLanguage::from_key(&word.language))
            .unwrap_or(preferred_language);
        let mut state = Self {
            user_id,
            puzzle_date: svc.today(),
            language,
            preferred_language,
            answer: String::new(),
            daily_word_loaded: false,
            guesses: Vec::new(),
            current_guess: String::new(),
            is_game_over: false,
            won: false,
            show_rules: false,
            show_language_picker: false,
            picker_language: language,
            accent_pending: false,
            message: "Le Word is unavailable. Retrying soon.".to_string(),
            word_reload_rx: None,
            word_reload_backoff_until: None,
            submission_rx: None,
            svc,
        };
        if let Some(word) = daily_word {
            state.install_round(word, saved_game);
        }
        state
    }

    fn install_round(&mut self, word: DailyWord, saved_game: Option<Game>) {
        self.puzzle_date = word.puzzle_date;
        self.language = LeWordLanguage::from_key(&word.language).unwrap_or_default();
        self.answer = word.answer_word;
        self.daily_word_loaded = true;
        self.guesses.clear();
        self.current_guess.clear();
        self.is_game_over = false;
        self.won = false;
        self.accent_pending = false;
        self.message = "Guess today's Le Word.".to_string();
        if let Some(game) = saved_game
            && game.puzzle_date == self.puzzle_date
            && game.language == self.language.key()
            && game.answer_word == self.answer
        {
            self.install_game(game);
        }
    }

    fn install_game(&mut self, game: Game) {
        self.language = LeWordLanguage::from_key(&game.language).unwrap_or_default();
        self.puzzle_date = game.puzzle_date;
        self.answer = game.answer_word;
        self.daily_word_loaded = true;
        self.guesses = serde_json::from_value(game.guesses).unwrap_or_default();
        self.current_guess = game.current_guess;
        self.is_game_over = game.is_game_over;
        self.won = game.won;
        self.accent_pending = false;
        self.message = if self.won {
            format!("Solved in {}.", self.guesses.len())
        } else if self.is_game_over {
            format!("The word was {}.", self.answer.to_uppercase())
        } else {
            "Try again.".to_string()
        };
    }

    pub fn language_locked(&self) -> bool {
        !self.guesses.is_empty() || self.is_game_over
    }
    pub fn submission_pending(&self) -> bool {
        self.submission_rx.is_some()
    }

    pub fn open_language_picker(&mut self) -> bool {
        if self.language_locked() || self.submission_pending() {
            self.message = "Your language is locked for this daily attempt.".to_string();
        } else {
            self.accent_pending = false;
            self.picker_language = self.language;
            self.show_language_picker = true;
        }
        true
    }

    pub fn choose_language(&mut self, language: LeWordLanguage) -> bool {
        if self.language_locked() || self.submission_pending() {
            return false;
        }
        self.show_language_picker = false;
        self.accent_pending = false;
        if self.language == language {
            return true;
        }
        self.language = language;
        self.preferred_language = language;
        self.current_guess.clear();
        self.word_reload_backoff_until = None;
        if tokio::runtime::Handle::try_current().is_ok() {
            self.svc.save_preference_task(self.user_id, language);
        }
        // Replacing the receiver makes results from an older selection unobservable.
        self.load_round(self.puzzle_date);
        true
    }

    fn load_round(&mut self, date: NaiveDate) {
        self.answer.clear();
        self.daily_word_loaded = false;
        self.guesses.clear();
        self.current_guess.clear();
        self.is_game_over = false;
        self.won = false;
        self.accent_pending = false;
        self.message = "Loading today's Le Word.".to_string();
        let (tx, rx) = oneshot::channel();
        self.word_reload_rx = Some(rx);
        let svc = self.svc.clone();
        let user_id = self.user_id;
        let language = self.language;
        if tokio::runtime::Handle::try_current().is_ok() {
            tokio::spawn(async move {
                let _ = tx.send(svc.load_round(user_id, date, language).await);
            });
        }
    }

    /// Retry unavailable words, and roll over only when the workspace permits it.
    pub fn ensure_current_daily(&mut self) -> bool {
        let today = self.svc.today();
        if (self.puzzle_date == today && self.daily_word_loaded)
            || self.word_reload_rx.is_some()
            || self.submission_pending()
        {
            return false;
        }
        if self
            .word_reload_backoff_until
            .is_some_and(|until| std::time::Instant::now() < until)
        {
            return false;
        }
        self.word_reload_backoff_until = None;
        if self.puzzle_date != today {
            self.language = self.preferred_language;
            self.show_language_picker = false;
        }
        // The date advances when the result lands, preserving failed-rollover retries.
        self.load_round(today);
        true
    }

    pub fn poll_word_reload(&mut self) -> bool {
        let Some(rx) = self.word_reload_rx.as_mut() else {
            // An unavailable board has no move to preserve, even while open.
            return if !self.daily_word_loaded {
                self.ensure_current_daily()
            } else {
                false
            };
        };
        match rx.try_recv() {
            Ok(Ok((word, game))) => {
                self.word_reload_rx = None;
                self.word_reload_backoff_until = None;
                self.install_round(word, game);
                true
            }
            Ok(Err(_)) | Err(oneshot::error::TryRecvError::Closed) => {
                self.word_reload_rx = None;
                self.word_reload_backoff_until =
                    Some(std::time::Instant::now() + WORD_RELOAD_RETRY);
                self.message = "Le Word is unavailable. Retrying soon.".to_string();
                true
            }
            Err(oneshot::error::TryRecvError::Empty) => false,
        }
    }

    pub fn poll_submission(&mut self) -> bool {
        let Some(rx) = self.submission_rx.as_mut() else {
            return false;
        };
        match rx.try_recv() {
            Ok(Ok(result)) => {
                self.submission_rx = None;
                self.install_game(result.game);
                if !result.accepted {
                    self.message = "Loaded your daily attempt from another session.".to_string();
                }
                true
            }
            Ok(Err(error)) => {
                tracing::error!(?error, "failed to submit Le Word guess");
                self.submission_rx = None;
                self.message =
                    "Could not submit. Your guess is kept; press Enter to retry.".to_string();
                true
            }
            Err(oneshot::error::TryRecvError::Closed) => {
                self.submission_rx = None;
                self.message =
                    "Could not submit. Your guess is kept; press Enter to retry.".to_string();
                true
            }
            Err(oneshot::error::TryRecvError::Empty) => false,
        }
    }

    pub fn has_unfinished_daily(&self) -> bool {
        self.daily_word_loaded
            && !self.guesses.is_empty()
            && !self.is_game_over
            && self.puzzle_date == self.svc.today()
    }
    pub fn guess_number(&self) -> usize {
        self.guesses
            .len()
            .saturating_add((!self.is_game_over) as usize)
    }

    pub fn submit_guess(&mut self) -> bool {
        if self.submission_pending() || self.is_game_over {
            return false;
        }
        if !self.daily_word_loaded {
            self.message = "Le Word is unavailable. Try again soon.".to_string();
            return true;
        }
        if self.current_guess.chars().count() != WORD_LEN {
            self.message = "Not enough letters.".to_string();
            return true;
        }
        if !self.svc.is_valid_guess(self.language, &self.current_guess) {
            self.message = "Not in word list.".to_string();
            return true;
        }
        self.accent_pending = false;
        let (tx, rx) = oneshot::channel();
        self.submission_rx = Some(rx);
        self.message = "Submitting guess…".to_string();
        let svc = self.svc.clone();
        let (user_id, date, language, guess, expected) = (
            self.user_id,
            self.puzzle_date,
            self.language,
            self.current_guess.clone(),
            self.guesses.len(),
        );
        if tokio::runtime::Handle::try_current().is_ok() {
            tokio::spawn(async move {
                let _ = tx.send(svc.submit(user_id, date, language, guess, expected).await);
            });
        }
        true
    }

    pub fn push_letter(&mut self, ch: char) -> bool {
        if !self.daily_word_loaded || self.is_game_over || self.submission_pending() {
            return false;
        }
        // Combining input can normalize the previous letter even in a full row.
        let candidate = normalize_word(&format!("{}{ch}", self.current_guess));
        if candidate.chars().count() > WORD_LEN
            || !candidate.chars().all(|ch| self.language.accepts(ch))
        {
            return false;
        }
        self.current_guess = candidate;
        self.message.clear();
        true
    }

    pub fn pop_letter(&mut self) -> bool {
        if self.accent_pending {
            self.accent_pending = false;
            self.message.clear();
            return true;
        }
        if !self.daily_word_loaded || self.is_game_over || self.submission_pending() {
            return false;
        }
        self.current_guess.pop().is_some()
    }
    pub fn scores_for_guess(&self, guess: &str) -> [LetterScore; WORD_LEN] {
        score_guess(guess, &self.answer)
    }
    pub fn score_for_keyboard_letter(&self, letter: char) -> Option<LetterScore> {
        score_letter_from_guesses(&self.guesses, &self.answer, letter)
    }
    pub fn open_rules(&mut self) {
        self.accent_pending = false;
        self.show_rules = true;
    }
    pub fn close_rules(&mut self) {
        self.show_rules = false;
    }
}

pub fn score_guess(guess: &str, answer: &str) -> [LetterScore; WORD_LEN] {
    let guess: Vec<char> = guess.chars().collect();
    let answer: Vec<char> = answer.chars().collect();
    let mut scores = [LetterScore::Absent; WORD_LEN];
    let mut remaining = std::collections::HashMap::<char, usize>::new();
    for (idx, score) in scores.iter_mut().enumerate() {
        if let Some(&letter) = answer.get(idx) {
            if guess.get(idx) == Some(&letter) {
                *score = LetterScore::Correct;
            } else {
                *remaining.entry(letter).or_default() += 1;
            }
        }
    }
    for (idx, score) in scores.iter_mut().enumerate() {
        if *score == LetterScore::Correct {
            continue;
        }
        if let Some(count) = guess.get(idx).and_then(|letter| remaining.get_mut(letter))
            && *count > 0
        {
            *score = LetterScore::Present;
            *count -= 1;
        }
    }
    scores
}

pub fn score_letter_from_guesses(
    guesses: &[String],
    answer: &str,
    letter: char,
) -> Option<LetterScore> {
    let letter = letter.to_lowercase().next()?;
    let mut best = None;
    for guess in guesses {
        let scores = score_guess(guess, answer);
        for (idx, ch) in guess.chars().enumerate().take(WORD_LEN) {
            if ch != letter {
                continue;
            }
            if best.is_none_or(|score| score_rank(scores[idx]) > score_rank(score)) {
                best = Some(scores[idx]);
            }
        }
    }
    best
}

fn score_rank(score: LetterScore) -> u8 {
    match score {
        LetterScore::Correct => 3,
        LetterScore::Present => 2,
        LetterScore::Absent => 1,
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
