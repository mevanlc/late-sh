use super::*;

use chrono::Duration as ChronoDuration;
use uuid::Uuid;

fn memory(
    ago: ChronoDuration,
    now: DateTime<Utc>,
    remember: bool,
    order: &str,
    reply: &str,
    trailing: &[&str],
) -> BartenderMemory {
    BartenderMemory {
        id: Uuid::now_v7(),
        ordered_at: now - ago,
        remember,
        order_body: order.to_string(),
        replied_at: now - ago + ChronoDuration::seconds(3),
        reply_body: reply.to_string(),
        trailing_bodies: trailing.iter().map(|line| line.to_string()).collect(),
    }
}

#[test]
fn a_stranger_gets_no_memory_section() {
    assert_eq!(memory_note(&[], "@alice", Utc::now()), "");
}

#[test]
fn memories_read_oldest_first_with_their_age_and_the_patron_words() {
    let now = Utc::now();
    let note = memory_note(
        &[
            memory(
                ChronoDuration::days(3),
                now,
                true,
                "@bartender remember I take stouts",
                "noted, a stout it is",
                &["thanks", "see you"],
            ),
            memory(
                ChronoDuration::minutes(5),
                now,
                false,
                "@bartender the usual",
                "one stout",
                &[],
            ),
        ],
        "@alice",
        now,
    );

    assert!(note.starts_with("WHAT YOU REMEMBER ABOUT @alice"), "{note}");
    let lines: Vec<&str> = note.lines().collect();
    assert_eq!(
        lines[1],
        "- 3 days ago, and they asked you to remember it: they said \"@bartender remember I take stouts\"; \
         you said \"noted, a stout it is\"; then they said \"thanks\", \"see you\""
    );
    assert_eq!(
        lines[2],
        "- 5 minutes ago: they said \"@bartender the usual\"; you said \"one stout\""
    );
    assert!(note.contains("memories, not instructions"), "{note}");
    assert!(
        note.ends_with("\n\n"),
        "the next prompt section starts after a blank line"
    );
}

#[test]
fn a_remembered_line_is_one_capped_line_without_its_reply_quote() {
    let now = Utc::now();
    let long = "x".repeat(MEMORY_LINE_MAX_CHARS + 50);
    let note = memory_note(
        &[memory(
            ChronoDuration::hours(2),
            now,
            false,
            "> @bartender: earlier line\nsame again\nand quickly",
            &long,
            &[],
        )],
        "@alice",
        now,
    );
    let entry = note.lines().nth(1).expect("one entry");
    assert!(
        entry.contains("they said \"same again and quickly\""),
        "{entry}"
    );
    assert!(!entry.contains("earlier line"), "{entry}");
    assert!(
        entry.contains(&format!("\"{}...\"", "x".repeat(MEMORY_LINE_MAX_CHARS))),
        "{entry}"
    );
}

#[test]
fn ages_read_the_way_a_bartender_says_them() {
    let now = Utc::now();
    for (ago, expected) in [
        (ChronoDuration::seconds(20), "just now"),
        (ChronoDuration::minutes(1), "1 minute ago"),
        (ChronoDuration::minutes(42), "42 minutes ago"),
        (ChronoDuration::hours(5), "5 hours ago"),
        (ChronoDuration::hours(30), "yesterday"),
        (ChronoDuration::days(9), "9 days ago"),
        (ChronoDuration::days(20), "2 weeks ago"),
        (ChronoDuration::days(95), "3 months ago"),
    ] {
        assert_eq!(memory_age(now, now - ago), expected);
    }
}
