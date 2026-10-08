use anyhow::{Result, ensure};
use chrono::NaiveDate;
use serde_json::Value;
use tokio_postgres::{Client, GenericClient};
use uuid::Uuid;

/// Languages supported by Le Word, independent of chat translation settings.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum LeWordLanguage {
    #[default]
    English,
    Polish,
}

impl LeWordLanguage {
    pub const ALL: [Self; 2] = [Self::English, Self::Polish];

    pub fn key(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::Polish => "pl",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::Polish => "Polski",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|language| language.key() == key)
    }

    pub fn accepts(self, ch: char) -> bool {
        ch.is_ascii_lowercase() || (self == Self::Polish && "ąćęłńóśźż".contains(ch))
    }

    pub fn from_settings(settings: &Value) -> Self {
        settings
            .get("le_word_language")
            .and_then(Value::as_str)
            .and_then(Self::from_key)
            .unwrap_or_default()
    }

    /// Merge only this preference, preserving profile and other settings.
    pub async fn save_preference(self, client: &Client, user_id: Uuid) -> Result<()> {
        let changed = client.execute(
            "UPDATE users SET settings = settings || jsonb_build_object('le_word_language', $1::text),
             updated = current_timestamp WHERE id = $2",
            &[&self.key(), &user_id],
        ).await?;
        ensure!(changed == 1, "user not found");
        Ok(())
    }
}

crate::model! {
    table = "le_word_daily_words";
    params = DailyWordParams;
    struct DailyWord {
        @data
        pub puzzle_date: NaiveDate,
        pub language: String,
        pub answer_word: String,
    }
}

crate::user_scoped_model! {
    table = "le_word_games";
    user_field = user_id;
    params = GameParams;
    struct Game {
        @data
        pub user_id: Uuid,
        pub puzzle_date: NaiveDate,
        pub language: String,
        pub answer_word: String,
        pub guesses: Value,
        pub current_guess: String,
        pub is_game_over: bool,
        pub won: bool,
    }
}

crate::user_scoped_model! {
    table = "le_word_daily_wins";
    user_field = user_id;
    params = DailyWinParams;
    struct DailyWin {
        @data
        pub user_id: Uuid,
        pub puzzle_date: NaiveDate,
        pub score: i32,
    }
}

impl DailyWord {
    pub async fn find_by_date(
        client: &impl GenericClient,
        puzzle_date: NaiveDate,
        language: LeWordLanguage,
    ) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT * FROM le_word_daily_words WHERE puzzle_date = $1 AND language = $2",
                &[&puzzle_date, &language.key()],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    /// Take the transaction-scoped advisory lock guarding daily-word creation,
    /// so concurrent `ensure_daily_word` callers serialize on the check-then-
    /// insert instead of racing. Released automatically when the transaction
    /// ends, so the caller must hold an open transaction.
    pub async fn lock_daily_creation(client: &impl GenericClient) -> Result<()> {
        client
            .query_one(
                "SELECT pg_advisory_xact_lock(hashtextextended('le_word_daily_word', 0))",
                &[],
            )
            .await?;
        Ok(())
    }

    pub async fn used_answer_words(
        client: &impl GenericClient,
        language: LeWordLanguage,
    ) -> Result<Vec<String>> {
        let rows = client
            .query(
                "SELECT answer_word FROM le_word_daily_words WHERE language = $1",
                &[&language.key()],
            )
            .await?;
        Ok(rows.into_iter().map(|row| row.get("answer_word")).collect())
    }

    pub async fn insert_for_date(
        client: &impl GenericClient,
        puzzle_date: NaiveDate,
        answer_word: &str,
        language: LeWordLanguage,
    ) -> Result<Self> {
        let row = client
            .query_one(
                "INSERT INTO le_word_daily_words (puzzle_date, answer_word, language)
                 VALUES ($1, $2, $3)
                 ON CONFLICT (language, puzzle_date) DO UPDATE SET answer_word = le_word_daily_words.answer_word
                 RETURNING *",
                &[&puzzle_date, &answer_word, &language.key()],
            )
            .await?;
        Ok(Self::from(row))
    }
}

impl Game {
    /// Also serializes creation when there is no game row to lock yet.
    pub async fn lock_submission(
        client: &impl GenericClient,
        user_id: Uuid,
        puzzle_date: NaiveDate,
    ) -> Result<()> {
        let key = format!("le_word_game:{user_id}:{puzzle_date}");
        client
            .query_one(
                "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
                &[&key],
            )
            .await?;
        Ok(())
    }

    pub async fn find_by_user_id_for_date(
        client: &impl GenericClient,
        user_id: Uuid,
        puzzle_date: NaiveDate,
    ) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT * FROM le_word_games WHERE user_id = $1 AND puzzle_date = $2",
                &[&user_id, &puzzle_date],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    pub async fn upsert(client: &impl GenericClient, params: GameParams) -> Result<Self> {
        let row = client
            .query_one(
                "INSERT INTO le_word_games
                   (user_id, puzzle_date, answer_word, guesses, current_guess, is_game_over, won, language)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                 ON CONFLICT (user_id, puzzle_date) DO UPDATE SET
                   language = $8,
                   answer_word = $3,
                   guesses = $4,
                   current_guess = $5,
                   is_game_over = $6,
                   won = $7,
                   updated = current_timestamp
                 WHERE le_word_games.language = $8 OR
                     (jsonb_array_length(le_word_games.guesses) = 0 AND NOT le_word_games.is_game_over)
                 RETURNING *",
                &[
                    &params.user_id,
                    &params.puzzle_date,
                    &params.answer_word,
                    &params.guesses,
                    &params.current_guess,
                    &params.is_game_over,
                    &params.won,
                    &params.language,
                ],
            )
            .await?;
        Ok(Self::from(row))
    }
}

impl DailyWin {
    pub async fn record_win(
        client: &impl GenericClient,
        user_id: Uuid,
        puzzle_date: NaiveDate,
        score: i32,
    ) -> Result<Self> {
        let row = client
            .query_one(
                &format!(
                    "WITH win AS (
                         INSERT INTO le_word_daily_wins (user_id, puzzle_date, score)
                         VALUES ($1, $2, $3)
                         ON CONFLICT (user_id, puzzle_date) DO UPDATE SET
                           score = LEAST(le_word_daily_wins.score, $3),
                           updated = current_timestamp
                         RETURNING *, (xmax = 0) AS fresh_win
                     ),
                     total AS (
                         {bump}
                     )
                     SELECT * FROM win",
                    bump = super::leaderboard::bump_daily_win_total_sql(
                        super::leaderboard::DailyPuzzle::LeWord
                    ),
                ),
                &[&user_id, &puzzle_date, &score],
            )
            .await?;
        Ok(Self::from(row))
    }

    pub async fn has_won_today(
        client: &impl GenericClient,
        user_id: Uuid,
        puzzle_date: NaiveDate,
    ) -> Result<bool> {
        let row = client
            .query_opt(
                "SELECT id FROM le_word_daily_wins WHERE user_id = $1 AND puzzle_date = $2",
                &[&user_id, &puzzle_date],
            )
            .await?;
        Ok(row.is_some())
    }
}
