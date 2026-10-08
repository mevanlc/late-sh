ALTER TABLE le_word_daily_words
    ADD COLUMN language TEXT NOT NULL DEFAULT 'en' CHECK (language IN ('en', 'pl')),
    DROP CONSTRAINT le_word_daily_words_puzzle_date_key,
    DROP CONSTRAINT le_word_daily_words_answer_word_key,
    DROP CONSTRAINT le_word_daily_words_answer_word_check,
    ADD UNIQUE (language, puzzle_date),
    ADD UNIQUE (language, answer_word),
    ADD CHECK (char_length(answer_word) = 5 AND (
        (language = 'en' AND answer_word ~ '^[a-z]+$') OR
        (language = 'pl' AND answer_word ~ '^[a-ząćęłńóśźż]+$')
    ));

ALTER TABLE le_word_games
    ADD COLUMN language TEXT NOT NULL DEFAULT 'en' CHECK (language IN ('en', 'pl')),
    DROP CONSTRAINT le_word_games_answer_word_check,
    ADD CHECK (char_length(answer_word) = 5 AND (
        (language = 'en' AND answer_word ~ '^[a-z]+$') OR
        (language = 'pl' AND answer_word ~ '^[a-ząćęłńóśźż]+$')
    ));

-- The game and win keys stay (user_id, puzzle_date): one attempt across languages.
