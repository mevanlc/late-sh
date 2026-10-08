use crate::{
    models::le_word::{DailyWin, DailyWord, Game, GameParams, LeWordLanguage},
    test_utils::{create_test_user, test_db},
};
use chrono::NaiveDate;

#[tokio::test]
async fn daily_word_records_one_global_answer_per_date() {
    let test_db = test_db().await;
    let mut client = test_db.db.get().await.expect("client");
    let tx = client.transaction().await.expect("transaction");
    let today = NaiveDate::from_ymd_opt(2026, 6, 17).unwrap();

    let inserted = DailyWord::insert_for_date(&*tx, today, "hunch", LeWordLanguage::English)
        .await
        .expect("insert daily word");
    assert_eq!(inserted.answer_word, "hunch");

    let again = DailyWord::insert_for_date(&*tx, today, "glass", LeWordLanguage::English)
        .await
        .expect("same date keeps existing answer");
    assert_eq!(again.answer_word, "hunch");

    let found = DailyWord::find_by_date(&*tx, today, LeWordLanguage::English)
        .await
        .expect("find daily word")
        .expect("daily word exists");
    assert_eq!(found.answer_word, "hunch");
    tx.commit().await.expect("commit");
}

#[tokio::test]
async fn game_progress_and_daily_win_persist() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("client");
    let user = create_test_user(&test_db.db, "le-word-player").await;
    let today = NaiveDate::from_ymd_opt(2026, 6, 17).unwrap();

    Game::upsert(
        &**client,
        GameParams {
            user_id: user.id,
            puzzle_date: today,
            language: "en".to_string(),
            answer_word: "hunch".to_string(),
            guesses: serde_json::json!(["glass", "hunch"]),
            current_guess: String::new(),
            is_game_over: true,
            won: true,
        },
    )
    .await
    .expect("save game");

    let game = Game::find_by_user_id_for_date(&**client, user.id, today)
        .await
        .expect("load game")
        .expect("game exists");
    assert_eq!(game.answer_word, "hunch");
    assert!(game.won);

    assert!(
        !DailyWin::has_won_today(&**client, user.id, today)
            .await
            .expect("initial win check")
    );

    DailyWin::record_win(&**client, user.id, today, 2)
        .await
        .expect("record win");
    let win = DailyWin::record_win(&**client, user.id, today, 4)
        .await
        .expect("record worse win");
    assert_eq!(win.score, 2);
    assert!(
        DailyWin::has_won_today(&**client, user.id, today)
            .await
            .expect("win check")
    );
}

#[tokio::test]
async fn daily_answers_are_unique_per_language_and_polish_letters_are_valid() {
    let db = test_db().await;
    let mut client = db.db.get().await.unwrap();
    let tx = client.transaction().await.unwrap();
    let date = NaiveDate::from_ymd_opt(2026, 10, 8).unwrap();
    let en = DailyWord::insert_for_date(&*tx, date, "radio", LeWordLanguage::English)
        .await
        .unwrap();
    let pl = DailyWord::insert_for_date(&*tx, date, "radio", LeWordLanguage::Polish)
        .await
        .unwrap();
    assert_ne!(en.id, pl.id);
    let again = DailyWord::insert_for_date(&*tx, date, "żółty", LeWordLanguage::Polish)
        .await
        .unwrap();
    assert_eq!(again.answer_word, "radio");
    let tomorrow = date.succ_opt().unwrap();
    let accented = DailyWord::insert_for_date(&*tx, tomorrow, "żółty", LeWordLanguage::Polish)
        .await
        .unwrap();
    assert_eq!(accented.answer_word.chars().count(), 5);
    let used = DailyWord::used_answer_words(&*tx, LeWordLanguage::English)
        .await
        .unwrap();
    assert!(!used.contains(&"żółty".to_string()));
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn language_migration_preserves_existing_english_progress() {
    let db = test_db().await;
    let user = create_test_user(&db.db, "migration-player").await;
    let mut client = db.db.get().await.unwrap();
    let tx = client.transaction().await.unwrap();
    tx.batch_execute("CREATE SCHEMA le_word_migration_probe; SET LOCAL search_path TO le_word_migration_probe, public;").await.unwrap();
    tx.batch_execute(include_str!("../../migrations/087_create_le_word.sql"))
        .await
        .unwrap();
    let date = NaiveDate::from_ymd_opt(2026, 10, 8).unwrap();
    tx.execute(
        "INSERT INTO le_word_daily_words (puzzle_date, answer_word) VALUES ($1, 'glass')",
        &[&date],
    )
    .await
    .unwrap();
    tx.execute("INSERT INTO le_word_games (user_id, puzzle_date, answer_word, guesses) VALUES ($1, $2, 'glass', '[\"hunch\"]'::jsonb)", &[&user.id, &date]).await.unwrap();
    tx.batch_execute(include_str!("../../migrations/229_le_word_languages.sql"))
        .await
        .unwrap();
    let word = DailyWord::find_by_date(&*tx, date, LeWordLanguage::English)
        .await
        .unwrap()
        .unwrap();
    let game = Game::find_by_user_id_for_date(&*tx, user.id, date)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(word.answer_word, "glass");
    assert_eq!(game.language, "en");
    assert_eq!(game.guesses, serde_json::json!(["hunch"]));
    tx.rollback().await.unwrap();
}
