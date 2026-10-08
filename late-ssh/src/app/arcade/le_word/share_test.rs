use chrono::NaiveDate;
use late_core::models::le_word::LeWordLanguage;

use crate::app::arcade::le_word::state::{LetterScore, score_guess};
use crate::app::arcade::share::{Glyph, Row, ShareCard, ShareFormat, render};

use super::card;

fn day(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

#[test]
fn won_card_is_the_guess_grid_with_the_solve_count() {
    let scores = vec![
        score_guess("adieu", "shade"),
        score_guess("shame", "shade"),
        score_guess("shade", "shade"),
    ];
    let card = card(day(2026, 6, 20), LeWordLanguage::English, &scores, true);
    assert_eq!(
        card,
        ShareCard {
            title: "late.sh Le Word (English) #71 · 3/6".to_string(),
            rows: vec![
                Row::Glyphs(vec![
                    Glyph::Yellow,
                    Glyph::Yellow,
                    Glyph::Dark,
                    Glyph::Yellow,
                    Glyph::Dark,
                ]),
                Row::Glyphs(vec![
                    Glyph::Green,
                    Glyph::Green,
                    Glyph::Green,
                    Glyph::Dark,
                    Glyph::Green,
                ]),
                Row::Glyphs(vec![Glyph::Green; 5]),
            ],
        }
    );
    assert_eq!(
        render(&card, ShareFormat::Emoji),
        "late.sh Le Word (English) #71 · 3/6\n🟨🟨⬛🟨⬛\n🟩🟩🟩⬛🟩\n🟩🟩🟩🟩🟩\nssh late.sh"
    );
}

#[test]
fn lost_card_reads_x_out_of_six() {
    let scores = vec![[LetterScore::Absent; 5]; 6];
    let card = card(day(2026, 6, 18), LeWordLanguage::English, &scores, false);
    assert_eq!(card.title, "late.sh Le Word (English) #69 · X/6");
    assert_eq!(card.rows.len(), 6);
}

#[test]
fn polish_share_card_identifies_the_language_without_revealing_the_answer() {
    let scores = vec![score_guess("żółty", "żółty")];
    let result = card(day(2026, 6, 20), LeWordLanguage::Polish, &scores, true);
    assert_eq!(result.title, "late.sh Le Word (Polski) #71 · 1/6");
    assert!(!render(&result, ShareFormat::Ascii).contains("żółty"));
}
