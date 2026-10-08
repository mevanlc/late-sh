use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::{Context, Result, ensure};
use chrono::NaiveDate;
use late_core::MutexRecover;
use late_core::db::Db;
use late_core::models::le_word::{DailyWin, DailyWord, Game, GameParams, LeWordLanguage};
use late_core::models::leaderboard::DailyPuzzle;
use late_core::models::profile::fetch_username;
use late_core::models::user::User;
use rand::seq::SliceRandom;
use tokio::sync::{broadcast, watch};
use unicode_normalization::UnicodeNormalization;
use uuid::Uuid;

use super::state::{MAX_GUESSES, WORD_LEN};
use crate::app::activity::event::{ActivityEvent, ActivityGame};
use crate::metrics::{self, ArcadeDifficulty, ArcadeFinish, ArcadeMode};

const ANSWER_POOL: &str = include_str!("../../../../assets/le_word/answer_pool.txt");
const VALID_EXTRA: &str = include_str!("../../../../assets/le_word/valid_extra.txt");
const POLISH_ANSWERS: &str = include_str!("../../../../assets/le_word/pl/answer_pool.txt");
const POLISH_GUESSES: &str = include_str!("../../../../assets/le_word/pl/valid_extra.txt");

struct Dictionary {
    answers: Vec<&'static str>,
    guesses: HashSet<&'static str>,
}
static ENGLISH: OnceLock<Dictionary> = OnceLock::new();
static POLISH: OnceLock<Dictionary> = OnceLock::new();

pub struct Submission {
    pub game: Game,
    pub accepted: bool,
}

#[derive(Clone)]
pub struct LeWordService {
    db: Db,
    activity_feed: broadcast::Sender<ActivityEvent>,
    preference_writes: Arc<Mutex<HashMap<Uuid, watch::Sender<LeWordLanguage>>>>,
}

impl LeWordService {
    pub fn new(db: Db, activity_feed: broadcast::Sender<ActivityEvent>) -> Self {
        Self {
            db,
            activity_feed,
            preference_writes: Arc::default(),
        }
    }

    pub fn today(&self) -> NaiveDate {
        chrono::Utc::now().date_naive()
    }

    pub fn is_valid_guess(&self, language: LeWordLanguage, guess: &str) -> bool {
        dictionary(language).guesses.contains(guess)
    }

    pub async fn ensure_daily_word(&self, language: LeWordLanguage) -> Result<DailyWord> {
        self.ensure_word_for_date(self.today(), language).await
    }

    async fn ensure_word_for_date(
        &self,
        puzzle_date: NaiveDate,
        language: LeWordLanguage,
    ) -> Result<DailyWord> {
        let mut client = self.db.get().await?;
        if let Some(word) = DailyWord::find_by_date(&**client, puzzle_date, language).await? {
            return Ok(word);
        }
        let tx = client.transaction().await?;
        DailyWord::lock_daily_creation(&*tx).await?;
        let word = match DailyWord::find_by_date(&*tx, puzzle_date, language).await? {
            Some(word) => word,
            None => {
                let used = DailyWord::used_answer_words(&*tx, language).await?;
                let used = used.iter().map(String::as_str).collect();
                let answer = choose_unused_answer(language, &used)?;
                DailyWord::insert_for_date(&*tx, puzzle_date, answer, language).await?
            }
        };
        tx.commit().await?;
        Ok(word)
    }

    pub async fn load_game(&self, user_id: Uuid, puzzle_date: NaiveDate) -> Result<Option<Game>> {
        let client = self.db.get().await?;
        Game::find_by_user_id_for_date(&**client, user_id, puzzle_date).await
    }

    /// A saved attempt owns its language; an unstarted day uses the account preference.
    pub async fn load_round(
        &self,
        user_id: Uuid,
        puzzle_date: NaiveDate,
        preferred: LeWordLanguage,
    ) -> Result<(DailyWord, Option<Game>)> {
        let game = self.load_game(user_id, puzzle_date).await?;
        let language = game
            .as_ref()
            .filter(|game| {
                game.is_game_over || game.guesses.as_array().is_some_and(|g| !g.is_empty())
            })
            .and_then(|game| LeWordLanguage::from_key(&game.language))
            .unwrap_or(preferred);
        Ok((
            self.ensure_word_for_date(puzzle_date, language).await?,
            game,
        ))
    }

    pub async fn load_initial_round(&self, user_id: Uuid) -> Result<(DailyWord, Option<Game>)> {
        let client = self.db.get().await?;
        let user = User::get(&client, user_id)
            .await?
            .context("user not found")?;
        let preferred = LeWordLanguage::from_settings(&user.settings);
        drop(client);
        self.load_round(user_id, self.today(), preferred).await
    }

    pub async fn has_won_today(&self, user_id: Uuid) -> Result<bool> {
        let client = self.db.get().await?;
        DailyWin::has_won_today(&**client, user_id, self.today()).await
    }

    /// One writer per account keeps rapid language choices in input order.
    pub fn save_preference_task(&self, user_id: Uuid, language: LeWordLanguage) {
        let mut writers = self.preference_writes.lock_recover();
        if let Some(writer) = writers.get(&user_id) {
            writer.send_replace(language);
            return;
        }
        let (writer, mut pending) = watch::channel(language);
        writers.insert(user_id, writer);
        drop(writers);
        let svc = self.clone();
        tokio::spawn(async move {
            loop {
                let language = *pending.borrow_and_update();
                match svc.db.get().await {
                    Ok(client) => {
                        if let Err(error) = language.save_preference(&client, user_id).await {
                            tracing::error!(?error, "failed to save Le Word language");
                        }
                    }
                    Err(error) => tracing::error!(?error, "failed to save Le Word language"),
                }
                // Retire under the enqueue lock so the last choice cannot get lost.
                let mut writers = svc.preference_writes.lock_recover();
                if !pending.has_changed().unwrap_or(false) {
                    writers.remove(&user_id);
                    break;
                }
            }
        });
    }

    /// Commit a guess against the current board, never overwriting another session.
    pub async fn submit(
        &self,
        user_id: Uuid,
        puzzle_date: NaiveDate,
        language: LeWordLanguage,
        guess: String,
        expected_guesses: usize,
    ) -> Result<Submission> {
        let guess = normalize_word(&guess);
        ensure!(self.is_valid_guess(language, &guess), "Not in word list.");
        let word = self.ensure_word_for_date(puzzle_date, language).await?;
        let mut client = self.db.get().await?;
        let tx = client.transaction().await?;
        Game::lock_submission(&*tx, user_id, puzzle_date).await?;
        let saved = Game::find_by_user_id_for_date(&*tx, user_id, puzzle_date).await?;
        let mut guesses: Vec<String> = saved
            .as_ref()
            .map(|game| serde_json::from_value(game.guesses.clone()))
            .transpose()?
            .unwrap_or_default();
        if let Some(game) = saved
            && (game.is_game_over
                || guesses.len() != expected_guesses
                || (!guesses.is_empty() && game.language != language.key()))
        {
            tx.commit().await?;
            return Ok(Submission {
                game,
                accepted: false,
            });
        }
        ensure!(
            guesses.len() == expected_guesses && guesses.len() < MAX_GUESSES,
            "Daily board changed. Reopen Le Word."
        );
        ensure!(
            !DailyWin::has_won_today(&*tx, user_id, puzzle_date).await?,
            "Today's Le Word is already completed."
        );
        guesses.push(guess.clone());
        let won = guess == word.answer_word;
        let is_game_over = won || guesses.len() == MAX_GUESSES;
        let game = Game::upsert(
            &*tx,
            GameParams {
                user_id,
                puzzle_date,
                language: language.key().to_string(),
                answer_word: word.answer_word,
                guesses: serde_json::to_value(&guesses)?,
                current_guess: String::new(),
                is_game_over,
                won,
            },
        )
        .await?;
        if won {
            DailyWin::record_win(&*tx, user_id, puzzle_date, guesses.len() as i32).await?;
        }
        tx.commit().await?;
        if is_game_over {
            metrics::record_arcade_finish(
                DailyPuzzle::LeWord,
                ArcadeMode::Daily,
                ArcadeDifficulty::Single,
                if won {
                    ArcadeFinish::Won
                } else {
                    ArcadeFinish::Lost
                },
            );
        }
        if won {
            let username = fetch_username(&client, user_id).await;
            let _ = self.activity_feed.send(ActivityEvent::game_won_at(
                user_id,
                username,
                ActivityGame::LeWord,
                Some("daily".to_string()),
                Some(guesses.len() as i32),
                ActivityEvent::occurred_on_utc_date(puzzle_date),
            ));
        }
        Ok(Submission {
            game,
            accepted: true,
        })
    }
}

pub fn normalize_word(word: &str) -> String {
    word.chars().flat_map(char::to_lowercase).nfc().collect()
}

fn dictionary(language: LeWordLanguage) -> &'static Dictionary {
    let (cell, answers, extra) = match language {
        LeWordLanguage::English => (&ENGLISH, ANSWER_POOL, VALID_EXTRA),
        LeWordLanguage::Polish => (&POLISH, POLISH_ANSWERS, POLISH_GUESSES),
    };
    cell.get_or_init(|| {
        let answers = parse_words(answers, language);
        let mut guesses = answers.iter().copied().collect::<HashSet<_>>();
        guesses.extend(parse_words(extra, language));
        Dictionary { answers, guesses }
    })
}

fn parse_words(source: &'static str, language: LeWordLanguage) -> Vec<&'static str> {
    source
        .lines()
        .map(str::trim)
        .filter(|word| {
            word.chars().count() == WORD_LEN && word.chars().all(|ch| language.accepts(ch))
        })
        .collect()
}

fn choose_unused_answer(language: LeWordLanguage, used: &HashSet<&str>) -> Result<&'static str> {
    // Sampling the actual remainder also handles removed/retired pool entries.
    dictionary(language)
        .answers
        .iter()
        .copied()
        .filter(|word| !used.contains(word))
        .collect::<Vec<_>>()
        .choose(&mut rand::thread_rng())
        .copied()
        .context("Le Word answer pool has no unused words left")
}

#[cfg(test)]
#[path = "svc_test.rs"]
mod svc_test;
