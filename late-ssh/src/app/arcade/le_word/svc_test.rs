use super::*;
use late_core::test_utils::{create_test_user, test_db};

#[test]
fn dictionaries_are_unique_normalized_five_letter_words_and_include_every_answer() {
    for (language, answers, extra) in [
        (LeWordLanguage::English, ANSWER_POOL, VALID_EXTRA),
        (LeWordLanguage::Polish, POLISH_ANSWERS, POLISH_GUESSES),
    ] {
        for source in [answers, extra] {
            let words: Vec<_> = source.lines().collect();
            assert_eq!(
                words.len(),
                words.iter().copied().collect::<HashSet<_>>().len()
            );
            for word in words {
                assert_eq!(word, normalize_word(word));
                assert_eq!(word.chars().count(), WORD_LEN, "{word}");
                assert!(word.chars().all(|ch| language.accepts(ch)), "{word}");
            }
        }
        let dictionary = dictionary(language);
        assert!(
            dictionary
                .answers
                .iter()
                .all(|answer| dictionary.guesses.contains(answer))
        );
    }
    assert_eq!(dictionary(LeWordLanguage::English).answers.len(), 2317);
    assert!(
        dictionary(LeWordLanguage::English)
            .guesses
            .contains("noire")
    );
    assert!(
        dictionary(LeWordLanguage::Polish)
            .answers
            .contains(&"szkło")
    );
    assert!(dictionary(LeWordLanguage::Polish).guesses.contains("żółty"));
    assert!(
        !dictionary(LeWordLanguage::English)
            .guesses
            .contains("żółty")
    );
}

#[test]
fn daily_selection_avoids_used_answers_and_handles_retired_words() {
    for language in LeWordLanguage::ALL {
        let answers = &dictionary(language).answers;
        let mut used: HashSet<_> = answers.iter().copied().collect();
        used.insert("retired");
        assert!(choose_unused_answer(language, &used).is_err());
        used.remove(answers[0]);
        assert_eq!(choose_unused_answer(language, &used).unwrap(), answers[0]);
    }
}

#[tokio::test]
async fn concurrent_first_guesses_lock_one_language_and_restore_the_same_board() {
    let db = test_db().await;
    let user = create_test_user(&db.db, "bilingual-player").await;
    let (tx, _) = broadcast::channel(8);
    let svc = LeWordService::new(db.db.clone(), tx);
    let (english, polish) = tokio::join!(
        svc.submit(
            user.id,
            svc.today(),
            LeWordLanguage::English,
            "glass".to_string(),
            0
        ),
        svc.submit(
            user.id,
            svc.today(),
            LeWordLanguage::Polish,
            "szkło".to_string(),
            0
        ),
    );
    let (english, polish) = (english.unwrap(), polish.unwrap());
    assert_ne!(english.accepted, polish.accepted);
    assert_eq!(english.game.language, polish.game.language);
    assert_eq!(english.game.guesses, polish.game.guesses);
    assert_eq!(english.game.guesses.as_array().unwrap().len(), 1);
    let language = LeWordLanguage::from_key(&english.game.language).unwrap();
    let other = if language == LeWordLanguage::English {
        LeWordLanguage::Polish
    } else {
        LeWordLanguage::English
    };
    let (word, saved) = svc.load_round(user.id, svc.today(), other).await.unwrap();
    assert_eq!(word.language, language.key());
    assert_eq!(saved.unwrap().guesses, english.game.guesses);
}

#[tokio::test]
async fn stale_submissions_do_not_overwrite_progress_or_repeat_win_events() {
    let db = test_db().await;
    let user = create_test_user(&db.db, "polish-winner").await;
    let client = db.db.get().await.unwrap();
    DailyWord::insert_for_date(
        &**client,
        chrono::Utc::now().date_naive(),
        "żółty",
        LeWordLanguage::Polish,
    )
    .await
    .unwrap();
    drop(client);
    let (tx, mut events) = broadcast::channel(8);
    let svc = LeWordService::new(db.db.clone(), tx);
    let first = svc
        .submit(
            user.id,
            svc.today(),
            LeWordLanguage::Polish,
            "szkło".to_string(),
            0,
        )
        .await
        .unwrap();
    assert!(first.accepted);
    let stale = svc
        .submit(
            user.id,
            svc.today(),
            LeWordLanguage::Polish,
            "radio".to_string(),
            0,
        )
        .await
        .unwrap();
    assert!(!stale.accepted);
    assert_eq!(stale.game.guesses, first.game.guesses);
    let win = svc
        .submit(
            user.id,
            svc.today(),
            LeWordLanguage::Polish,
            "ŻÓŁTY".to_string(),
            1,
        )
        .await
        .unwrap();
    assert!(win.accepted && win.game.won);
    assert!(events.try_recv().is_ok());
    let repeat = svc
        .submit(
            user.id,
            svc.today(),
            LeWordLanguage::English,
            "glass".to_string(),
            0,
        )
        .await
        .unwrap();
    assert!(!repeat.accepted);
    assert_eq!(repeat.game.language, "pl");
    assert!(events.try_recv().is_err());
    let client = db.db.get().await.unwrap();
    let wins: i64 = client
        .query_one(
            "SELECT COUNT(*) FROM le_word_daily_wins WHERE user_id = $1",
            &[&user.id],
        )
        .await
        .unwrap()
        .get(0);
    let total: i64 = client
        .query_one(
            "SELECT wins FROM daily_win_totals WHERE game = 'le_word' AND user_id = $1",
            &[&user.id],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!((wins, total), (1, 1));
}

#[tokio::test]
async fn rejected_words_do_not_lock_the_daily_and_polish_losses_do() {
    let db = test_db().await;
    let user = create_test_user(&db.db, "daily-loss").await;
    let client = db.db.get().await.unwrap();
    DailyWord::insert_for_date(
        &**client,
        chrono::Utc::now().date_naive(),
        "żółty",
        LeWordLanguage::Polish,
    )
    .await
    .unwrap();
    drop(client);
    let (tx, _) = broadcast::channel(8);
    let svc = LeWordService::new(db.db.clone(), tx);
    assert!(
        svc.submit(
            user.id,
            svc.today(),
            LeWordLanguage::English,
            "zzzzz".to_string(),
            0
        )
        .await
        .is_err()
    );
    assert!(svc.load_game(user.id, svc.today()).await.unwrap().is_none());
    for expected in 0..6 {
        let result = svc
            .submit(
                user.id,
                svc.today(),
                LeWordLanguage::Polish,
                "szkło".to_string(),
                expected,
            )
            .await
            .unwrap();
        assert!(result.accepted);
    }
    let result = svc
        .submit(
            user.id,
            svc.today(),
            LeWordLanguage::English,
            "glass".to_string(),
            0,
        )
        .await
        .unwrap();
    assert!(!result.accepted);
    assert!(result.game.is_game_over && !result.game.won);
    assert_eq!(result.game.language, "pl");
}

#[tokio::test]
async fn preference_writes_keep_the_latest_choice_and_other_settings() {
    let db = test_db().await;
    let user = create_test_user(&db.db, "language-setting").await;
    let client = db.db.get().await.unwrap();
    client
        .execute(
            "UPDATE users SET settings = settings || $1::jsonb WHERE id = $2",
            &[
                &serde_json::json!({"theme": "greenery", "nested": {"keep": true}}),
                &user.id,
            ],
        )
        .await
        .unwrap();
    drop(client);
    let (tx, _) = broadcast::channel(8);
    let svc = LeWordService::new(db.db.clone(), tx);
    for i in 0..30 {
        svc.save_preference_task(
            user.id,
            if i % 2 == 0 {
                LeWordLanguage::English
            } else {
                LeWordLanguage::Polish
            },
        );
    }
    for _ in 0..100 {
        let client = db.db.get().await.unwrap();
        let loaded = User::get(&client, user.id).await.unwrap().unwrap();
        if LeWordLanguage::from_settings(&loaded.settings) == LeWordLanguage::Polish {
            // Give any obsolete writer enough opportunity to expose out-of-order saves.
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            let final_user = User::get(&client, user.id).await.unwrap().unwrap();
            assert_eq!(
                LeWordLanguage::from_settings(&final_user.settings),
                LeWordLanguage::Polish
            );
            assert_eq!(final_user.settings["theme"], "greenery");
            assert_eq!(final_user.settings["nested"]["keep"], true);
            assert!(svc.preference_writes.lock_recover().is_empty());
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("latest preference was not saved");
}
