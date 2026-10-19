use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use crate::{
    models::{
        bartender_memory::{
            BartenderMemory, MEMORY_CAP, MemorySlot, REMEMBER_KEEP, Recorded, evictions,
            is_forget_request, memory_phrase_spans, mentions_remember,
        },
        chat_message::{ChatMessage, ChatMessageParams},
        chat_room::ChatRoom,
    },
    test_utils::{create_test_user, test_db},
};

fn slots(remember: &[bool]) -> Vec<MemorySlot> {
    remember
        .iter()
        .map(|&remember| MemorySlot {
            id: Uuid::now_v7(),
            remember,
        })
        .collect()
}

#[test]
fn nothing_goes_while_the_patron_is_under_the_cap() {
    assert!(evictions(&slots(&[false; MEMORY_CAP])).is_empty());
    assert!(evictions(&slots(&[true; MEMORY_CAP])).is_empty());
    assert!(evictions(&[]).is_empty());
}

#[test]
fn the_oldest_plain_exchange_makes_room() {
    let all = slots(&[false; MEMORY_CAP + 1]);
    assert_eq!(evictions(&all), vec![all[MEMORY_CAP].id]);
}

#[test]
fn remembered_exchanges_outlast_newer_plain_ones() {
    // Newest first: nine plain nights on top of eight old "remember"s.
    let mut flags = vec![false; 9];
    flags.extend([true; REMEMBER_KEEP]);
    let all = slots(&flags);
    assert_eq!(
        evictions(&all),
        vec![all[8].id],
        "the oldest plain one goes, not an older remembered one"
    );
}

#[test]
fn only_the_newest_remembers_are_protected() {
    // Sixteen "remember"s and a new plain night: the new one still gets
    // in, and the oldest "remember" is the one that goes.
    let mut flags = vec![false];
    flags.extend([true; MEMORY_CAP]);
    let all = slots(&flags);
    assert_eq!(evictions(&all), vec![all[MEMORY_CAP].id]);
}

#[test]
fn unprotected_remembers_compete_on_recency() {
    // Ten "remember"s on top of seven plain ones: two remembers are past
    // the protected eight and are simply the newest of the rest.
    let mut flags = vec![true; 10];
    flags.extend([false; 7]);
    let all = slots(&flags);
    assert_eq!(evictions(&all), vec![all[16].id]);
}

#[test]
fn forget_me_must_be_said_exactly() {
    for message in [
        "@bartender forget me",
        "  @BARTENDER   Forget ME  ",
        "@bartender, forget me",
        "@bartender: forget me",
        "forget me @bartender",
        "@bartender *hic* forget me",
        "@bartender forget me *hic*",
        "forget me",
    ] {
        assert!(is_forget_request(message, "bartender"), "{message:?}");
    }
    for message in [
        "@bartender please forget me",
        "@bartender forget me?",
        "@bartender forget me.",
        "@bartender don't forget me",
        "@bartender forget me not",
        "@bartender forget",
        "@bartender @bartender forget me",
        "forget @bartender me",
        "@bot forget me",
        "@bartender",
    ] {
        assert!(!is_forget_request(message, "bartender"), "{message:?}");
    }
}

#[test]
fn remember_counts_only_as_a_whole_word() {
    for message in [
        "@bartender remember I take it neat",
        "@bartender REMEMBER: stouts",
        "@bartender please, remember.",
        "@bartender (remember) this",
    ] {
        assert!(mentions_remember(message), "{message:?}");
    }
    for message in [
        "@bartender I remembered my wallet",
        "@bartender remembering old times",
        "@bartender misremember",
        "@bartender the usual",
    ] {
        assert!(!mentions_remember(message), "{message:?}");
    }
}

#[test]
fn drunk_text_spares_both_triggers() {
    let text = "@bartender Remember this, then forget  me";
    let spans = memory_phrase_spans(text);
    let covered: Vec<&str> = spans.iter().map(|&(a, b)| &text[a..b]).collect();
    assert_eq!(covered, vec!["Remember", "forget  me"]);

    assert!(memory_phrase_spans("forget meow, remembered").is_empty());
}

async fn say(
    client: &deadpool_postgres::Client,
    room_id: Uuid,
    user_id: Uuid,
    body: &str,
    at: DateTime<Utc>,
) -> ChatMessage {
    let message = ChatMessage::create_with_reply_to(
        client,
        ChatMessageParams {
            room_id,
            user_id,
            body: body.to_string(),
        },
        None,
    )
    .await
    .expect("message");
    client
        .execute(
            "UPDATE chat_messages SET created = $2 WHERE id = $1",
            &[&message.id, &at],
        )
        .await
        .expect("backdate message");
    ChatMessage {
        created: at,
        ..message
    }
}

fn secs(base: DateTime<Utc>, offset: i64) -> DateTime<Utc> {
    base + Duration::seconds(offset)
}

#[tokio::test]
async fn an_exchange_keeps_what_the_patron_said_until_the_tail_closes() {
    let test_db = test_db().await;
    let patron = create_test_user(&test_db.db, "memory-patron").await;
    let other = create_test_user(&test_db.db, "memory-other").await;
    let bartender = create_test_user(&test_db.db, "memory-keep").await;
    let mut client = test_db.db.get().await.expect("db client");
    let room = ChatRoom::get_or_create_public_room(&client, "memory-room")
        .await
        .expect("room");
    let elsewhere = ChatRoom::get_or_create_public_room(&client, "memory-elsewhere")
        .await
        .expect("room");
    let t0 = Utc::now() - Duration::hours(1);

    let order = say(
        &client,
        room.id,
        patron.id,
        "@memory-keep remember stouts",
        t0,
    )
    .await;
    say(&client, room.id, patron.id, "while I wait", secs(t0, 1)).await;
    say(&client, room.id, other.id, "not theirs", secs(t0, 2)).await;
    say(&client, elsewhere.id, patron.id, "other room", secs(t0, 2)).await;
    let reply = say(&client, room.id, bartender.id, "one stout", secs(t0, 3)).await;
    say(&client, room.id, patron.id, "cheers", secs(t0, 8)).await;
    say(&client, room.id, patron.id, "too late", secs(t0, 9)).await;

    let recorded = BartenderMemory::record(&mut client, &order, &reply, true)
        .await
        .expect("record");
    assert_eq!(recorded, Recorded::Stored { evicted: 0 });

    let memories = BartenderMemory::list_for_user(&client, patron.id)
        .await
        .expect("list");
    assert_eq!(memories.len(), 1);
    let memory = &memories[0];
    assert!(memory.remember);
    assert_eq!(memory.ordered_at, t0);
    assert_eq!(memory.order_body, "@memory-keep remember stouts");
    assert_eq!(memory.reply_body, "one stout");
    assert_eq!(memory.trailing_bodies, vec!["while I wait", "cheers"]);
}

#[tokio::test]
async fn the_tail_keeps_the_first_five_lines() {
    let test_db = test_db().await;
    let patron = create_test_user(&test_db.db, "tail-patron").await;
    let bartender = create_test_user(&test_db.db, "tail-keep").await;
    let mut client = test_db.db.get().await.expect("db client");
    let room = ChatRoom::get_or_create_public_room(&client, "tail-room")
        .await
        .expect("room");
    let t0 = Utc::now() - Duration::hours(1);

    let order = say(&client, room.id, patron.id, "@tail-keep a beer", t0).await;
    let reply = say(&client, room.id, bartender.id, "a beer", secs(t0, 1)).await;
    for line in 1..=7 {
        say(
            &client,
            room.id,
            patron.id,
            &format!("line {line}"),
            secs(t0, 1 + line / 2),
        )
        .await;
    }

    BartenderMemory::record(&mut client, &order, &reply, false)
        .await
        .expect("record");
    let memories = BartenderMemory::list_for_user(&client, patron.id)
        .await
        .expect("list");
    assert_eq!(
        memories[0].trailing_bodies,
        vec!["line 1", "line 2", "line 3", "line 4", "line 5"]
    );
}

#[tokio::test]
async fn the_cap_keeps_an_old_remember_over_newer_plain_nights() {
    let test_db = test_db().await;
    let patron = create_test_user(&test_db.db, "cap-patron").await;
    let bartender = create_test_user(&test_db.db, "cap-keep").await;
    let mut client = test_db.db.get().await.expect("db client");
    let room = ChatRoom::get_or_create_public_room(&client, "cap-room")
        .await
        .expect("room");
    let t0 = Utc::now() - Duration::hours(2);

    let mut orders = Vec::new();
    for night in 0..=MEMORY_CAP as i64 + 1 {
        let at = secs(t0, night * 60);
        let order = say(&client, room.id, patron.id, &format!("order {night}"), at).await;
        let reply = say(&client, room.id, bartender.id, "poured", secs(at, 1)).await;
        BartenderMemory::record(&mut client, &order, &reply, night == 0)
            .await
            .expect("record");
        orders.push(order);
    }

    let memories = BartenderMemory::list_for_user(&client, patron.id)
        .await
        .expect("list");
    assert_eq!(memories.len(), MEMORY_CAP);
    let bodies: Vec<&str> = memories.iter().map(|m| m.order_body.as_str()).collect();
    assert_eq!(bodies[0], "order 0", "the remembered night stays");
    assert!(
        !bodies.contains(&"order 1") && !bodies.contains(&"order 2"),
        "the two oldest plain nights made room: {bodies:?}"
    );
    assert_eq!(bodies[MEMORY_CAP - 1], "order 17");
}

#[tokio::test]
async fn forget_me_wipes_and_turns_away_what_was_still_in_flight() {
    let test_db = test_db().await;
    let patron = create_test_user(&test_db.db, "forget-patron").await;
    let bartender = create_test_user(&test_db.db, "forget-keep").await;
    let mut client = test_db.db.get().await.expect("db client");
    let room = ChatRoom::get_or_create_public_room(&client, "forget-room")
        .await
        .expect("room");
    let t0 = Utc::now() - Duration::hours(1);

    let first = say(&client, room.id, patron.id, "@forget-keep one", t0).await;
    let first_reply = say(&client, room.id, bartender.id, "one", secs(t0, 1)).await;
    BartenderMemory::record(&mut client, &first, &first_reply, false)
        .await
        .expect("record");
    // Ordered before the wipe, answered after it.
    let in_flight = say(
        &client,
        room.id,
        patron.id,
        "@forget-keep two",
        secs(t0, 10),
    )
    .await;
    let wipe = say(
        &client,
        room.id,
        patron.id,
        "@forget-keep forget me",
        secs(t0, 11),
    )
    .await;

    let forgotten = BartenderMemory::forget(&mut client, patron.id, wipe.created)
        .await
        .expect("forget");
    assert_eq!(forgotten, 1);

    let late_reply = say(&client, room.id, bartender.id, "two", secs(t0, 12)).await;
    let recorded = BartenderMemory::record(&mut client, &in_flight, &late_reply, false)
        .await
        .expect("record");
    assert_eq!(recorded, Recorded::Forgotten);
    assert!(
        BartenderMemory::list_for_user(&client, patron.id)
            .await
            .expect("list")
            .is_empty()
    );

    let after = say(
        &client,
        room.id,
        patron.id,
        "@forget-keep three",
        secs(t0, 20),
    )
    .await;
    let after_reply = say(&client, room.id, bartender.id, "three", secs(t0, 21)).await;
    let recorded = BartenderMemory::record(&mut client, &after, &after_reply, false)
        .await
        .expect("record");
    assert_eq!(recorded, Recorded::Stored { evicted: 0 });
}

#[tokio::test]
async fn a_memory_outlives_the_chat_log_it_came_from() {
    let test_db = test_db().await;
    let patron = create_test_user(&test_db.db, "outlive-patron").await;
    let bartender = create_test_user(&test_db.db, "outlive-keep").await;
    let mut client = test_db.db.get().await.expect("db client");
    let room = ChatRoom::get_or_create_public_room(&client, "outlive-room")
        .await
        .expect("room");
    let t0 = Utc::now() - Duration::hours(1);

    let order = say(&client, room.id, patron.id, "@outlive-keep one", t0).await;
    let reply = say(&client, room.id, bartender.id, "one", secs(t0, 1)).await;
    let tail = say(&client, room.id, patron.id, "thanks", secs(t0, 2)).await;
    BartenderMemory::record(&mut client, &order, &reply, false)
        .await
        .expect("record");
    assert_eq!(
        BartenderMemory::record(&mut client, &order, &reply, false)
            .await
            .expect("record again"),
        Recorded::AlreadyStored,
        "one exchange is stored once"
    );

    for message in [&order, &reply, &tail] {
        ChatMessage::delete_by_admin(&client, message.id)
            .await
            .expect("delete");
    }
    let memories = BartenderMemory::list_for_user(&client, patron.id)
        .await
        .expect("list");
    assert_eq!(memories.len(), 1);
    assert_eq!(memories[0].order_body, "@outlive-keep one");
    assert_eq!(memories[0].replied_at, reply.created);
    assert_eq!(memories[0].reply_body, "one");
    assert_eq!(memories[0].trailing_bodies, vec!["thanks"]);
}
