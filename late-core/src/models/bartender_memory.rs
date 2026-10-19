//! What @bartender remembers about a patron.
//!
//! One row per exchange he answered: the patron's order, his reply, and up
//! to [`TRAILING_MESSAGE_CAP`] more lines the patron said in that room from
//! the order until [`TRAILING_WINDOW_SECS`] after the reply landed. The
//! words are copied when the exchange is stored, so a memory lives by the
//! rules below and nothing else: pruning or deleting the chat log it came
//! from leaves it alone.
//!
//! A patron keeps at most [`MEMORY_CAP`]. An order that says "remember"
//! ([`mentions_remember`]) asks for its exchange to stick: the newest
//! [`REMEMBER_KEEP`] of those are kept ahead of everything else, and every
//! other slot goes to the newest of the rest ([`evictions`]). The cap on
//! the protected ones is what keeps a patron who says "remember" every time
//! from crowding out tonight.
//!
//! "forget me", said exactly ([`is_forget_request`]), wipes the lot and
//! stamps `bartender_memory_resets`, so an exchange still in flight when it
//! was said cannot land after the wipe.
//!
//! The two triggers are matched literally on the stored body, which is the
//! drunk-scrambled one, so `chat/slur.rs` protects [`memory_phrase_spans`]
//! the same way it protects the spending phrases.

use anyhow::Result;
use chrono::{DateTime, Utc};
use tokio_postgres::{Client, Transaction};
use uuid::Uuid;

use super::chat_message::ChatMessage;

/// Most exchanges one patron keeps.
pub const MEMORY_CAP: usize = 16;
/// Most "remember" exchanges kept ahead of the plain ones. The rest of the
/// cap always goes to recency.
pub const REMEMBER_KEEP: usize = 8;
/// Most lines kept from what the patron said after ordering.
pub const TRAILING_MESSAGE_CAP: i64 = 5;
/// How long after the reply lands the patron's lines still count.
pub const TRAILING_WINDOW_SECS: i64 = 5;

/// The whole of a wipe request, once the bartender's handle is set aside.
const FORGET_WORDS: [&str; 2] = ["forget", "me"];
/// The word that asks for an exchange to stick.
const REMEMBER_WORD: &str = "remember";
/// What `chat/slur.rs` drops into a drunk patron's line. It carries no
/// meaning, so the forget matcher looks straight past it.
const HICCUP: &str = "*hic*";

/// One stored exchange, newest first in the slice handed to [`evictions`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemorySlot {
    pub id: Uuid,
    pub remember: bool,
}

/// Which of a patron's exchanges go, given all of them newest first: the
/// newest [`REMEMBER_KEEP`] "remember" ones stay, then the newest of the
/// rest fill what is left of [`MEMORY_CAP`]. Older "remember" ones beyond
/// the protected few compete on recency like any other.
pub fn evictions(newest_first: &[MemorySlot]) -> Vec<Uuid> {
    let mut protected_left = REMEMBER_KEEP;
    let protected: Vec<bool> = newest_first
        .iter()
        .map(|slot| {
            let protect = slot.remember && protected_left > 0;
            if protect {
                protected_left -= 1;
            }
            protect
        })
        .collect();
    let mut open = MEMORY_CAP - (REMEMBER_KEEP - protected_left);
    newest_first
        .iter()
        .zip(protected)
        .filter_map(|(slot, protect)| match (protect, open) {
            (true, _) => None,
            (false, 0) => Some(slot.id),
            (false, _) => {
                open -= 1;
                None
            }
        })
        .collect()
}

/// Whether `text` (the message with its reply quote stripped) is exactly
/// "forget me": any case, any spacing, the bartender's handle allowed once
/// at either end (with a `,` or `:` after it), a hiccup ignored. Anything
/// more is conversation, so "please forget me" or "forget me?" goes to the
/// model like any other line.
pub fn is_forget_request(text: &str, bartender: &str) -> bool {
    let handle = bartender.trim().trim_start_matches('@');
    let mut words: Vec<&str> = text
        .split_whitespace()
        .filter(|word| *word != HICCUP)
        .collect();
    if words.first().is_some_and(|word| is_handle(word, handle)) {
        words.remove(0);
    } else if words.last().is_some_and(|word| is_handle(word, handle)) {
        words.pop();
    }
    words.len() == FORGET_WORDS.len()
        && words
            .iter()
            .zip(FORGET_WORDS)
            .all(|(word, expected)| word.eq_ignore_ascii_case(expected))
}

fn is_handle(word: &str, handle: &str) -> bool {
    word.trim_end_matches([',', ':'])
        .strip_prefix('@')
        .is_some_and(|name| name.eq_ignore_ascii_case(handle))
}

/// Whether an order asks for its exchange to stick: "remember" as a whole
/// word, any case. "remembered" and "remembering" are talk about the past,
/// not a request.
pub fn mentions_remember(text: &str) -> bool {
    !word_spans(text, REMEMBER_WORD).is_empty()
}

/// Byte ranges in `text` that drunk text must leave alone: every whole-word
/// "remember" and every "forget me", so the bar still reads what the
/// patron meant.
pub fn memory_phrase_spans(text: &str) -> Vec<(usize, usize)> {
    let mut spans = word_spans(text, REMEMBER_WORD);
    for (start, end) in word_spans(text, FORGET_WORDS[0]) {
        let rest = &text[end..];
        let gap = rest.len() - rest.trim_start().len();
        if gap == 0 {
            continue;
        }
        let me_start = end + gap;
        if let Some(&(_, me_end)) = word_spans(&text[me_start..], FORGET_WORDS[1])
            .first()
            .filter(|(at, _)| *at == 0)
        {
            spans.push((start, me_start + me_end));
        }
    }
    spans.sort_unstable();
    spans
}

/// Every whole-word, case-insensitive occurrence of the ASCII `word`.
fn word_spans(text: &str, word: &str) -> Vec<(usize, usize)> {
    // ASCII lowercasing keeps every byte where it was, so offsets found in
    // the lowered copy index the original.
    let lowered = text.to_ascii_lowercase();
    lowered
        .match_indices(word)
        .map(|(start, _)| (start, start + word.len()))
        .filter(|&(start, end)| {
            !text[..start].chars().next_back().is_some_and(is_word_char)
                && !text[end..].chars().next().is_some_and(is_word_char)
        })
        .collect()
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Whether [`BartenderMemory::record`] kept the exchange.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recorded {
    /// Stored, and this many older exchanges made room for it.
    Stored { evicted: usize },
    /// The patron said "forget me" after ordering this one.
    Forgotten,
    /// This order's exchange was already stored.
    AlreadyStored,
}

/// One remembered exchange as the bartender's prompt reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BartenderMemory {
    pub id: Uuid,
    pub ordered_at: DateTime<Utc>,
    pub remember: bool,
    pub order_body: String,
    pub replied_at: DateTime<Utc>,
    pub reply_body: String,
    /// What the patron said after ordering, oldest first.
    pub trailing_bodies: Vec<String>,
}

impl BartenderMemory {
    /// Store the exchange of `order` (the patron's message) and `reply`
    /// (the bartender's answer), with the patron's lines in the same room
    /// up to [`TRAILING_WINDOW_SECS`] after the reply. Call it once that
    /// window has closed. Every word is copied here, so the memory owes
    /// nothing to the chat log afterwards.
    ///
    /// One transaction under the patron's lock, shared with
    /// [`Self::forget`]: the reset check, the insert and the eviction all
    /// read the same rows, so two exchanges settling together cannot both
    /// see fifteen and leave seventeen.
    pub async fn record(
        client: &mut Client,
        order: &ChatMessage,
        reply: &ChatMessage,
        remember: bool,
    ) -> Result<Recorded> {
        let user_id = order.user_id;
        let tx = client.transaction().await?;
        lock_patron(&tx, user_id).await?;

        let forgotten = tx
            .query_opt(
                "SELECT 1 FROM bartender_memory_resets
                 WHERE user_id = $1 AND reset_at >= $2",
                &[&user_id, &order.created],
            )
            .await?
            .is_some();
        if forgotten {
            return Ok(Recorded::Forgotten);
        }

        let inserted = tx
            .execute(
                "INSERT INTO bartender_memories
                     (user_id, order_message_id, ordered_at, order_body,
                      replied_at, reply_body, trailing_bodies, remember)
                 VALUES ($1, $2, $3, $4, $5, $6,
                     ARRAY(
                         SELECT t.body FROM chat_messages t
                         WHERE t.user_id = $1
                           AND t.room_id = $7
                           AND (t.created, t.id) > ($3::timestamptz, $2::uuid)
                           AND t.created <= $5::timestamptz + make_interval(secs => $8::double precision)
                         ORDER BY t.created, t.id
                         LIMIT $9
                     ),
                     $10)
                 ON CONFLICT (order_message_id) DO NOTHING",
                &[
                    &user_id,
                    &order.id,
                    &order.created,
                    &order.body,
                    &reply.created,
                    &reply.body,
                    &order.room_id,
                    &(TRAILING_WINDOW_SECS as f64),
                    &TRAILING_MESSAGE_CAP,
                    &remember,
                ],
            )
            .await?;
        if inserted == 0 {
            return Ok(Recorded::AlreadyStored);
        }

        let slots: Vec<MemorySlot> = tx
            .query(
                "SELECT id, remember FROM bartender_memories
                 WHERE user_id = $1
                 ORDER BY ordered_at DESC, id DESC",
                &[&user_id],
            )
            .await?
            .into_iter()
            .map(|row| MemorySlot {
                id: row.get("id"),
                remember: row.get("remember"),
            })
            .collect();
        let evicted = evictions(&slots);
        if !evicted.is_empty() {
            tx.execute(
                "DELETE FROM bartender_memories WHERE id = ANY($1)",
                &[&evicted],
            )
            .await?;
        }
        tx.commit().await?;
        Ok(Recorded::Stored {
            evicted: evicted.len(),
        })
    }

    /// Wipe everything the bartender remembers about `user_id`, and refuse
    /// any exchange ordered at or before `said_at` (the "forget me"
    /// message's own time) that is still on its way in. Returns how many
    /// exchanges were forgotten.
    pub async fn forget(client: &mut Client, user_id: Uuid, said_at: DateTime<Utc>) -> Result<u64> {
        let tx = client.transaction().await?;
        lock_patron(&tx, user_id).await?;
        let forgotten = tx
            .execute(
                "DELETE FROM bartender_memories WHERE user_id = $1",
                &[&user_id],
            )
            .await?;
        tx.execute(
            "INSERT INTO bartender_memory_resets (user_id, reset_at)
             VALUES ($1, $2)
             ON CONFLICT (user_id) DO UPDATE
             SET reset_at = GREATEST(bartender_memory_resets.reset_at, EXCLUDED.reset_at)",
            &[&user_id, &said_at],
        )
        .await?;
        tx.commit().await?;
        Ok(forgotten)
    }

    /// Everything the bartender remembers about `user_id`, oldest first.
    pub async fn list_for_user(client: &Client, user_id: Uuid) -> Result<Vec<Self>> {
        let rows = client
            .query(
                "SELECT id, ordered_at, remember, order_body,
                        replied_at, reply_body, trailing_bodies
                 FROM bartender_memories
                 WHERE user_id = $1
                 ORDER BY ordered_at, id",
                &[&user_id],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| Self {
                id: row.get("id"),
                ordered_at: row.get("ordered_at"),
                remember: row.get("remember"),
                order_body: row.get("order_body"),
                replied_at: row.get("replied_at"),
                reply_body: row.get("reply_body"),
                trailing_bodies: row.get("trailing_bodies"),
            })
            .collect())
    }
}

async fn lock_patron(tx: &Transaction<'_>, user_id: Uuid) -> Result<()> {
    tx.query_one(
        "SELECT pg_advisory_xact_lock(
           hashtextextended(concat_ws(':', 'bartender_memory', ($1::uuid)::text), 0)
         )",
        &[&user_id],
    )
    .await?;
    Ok(())
}
