use late_core::{
    models::{
        user::{User, UserParams},
        vote::Vote,
    },
    test_utils::test_db,
};

#[tokio::test]
async fn test_vote_tally() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let user1 = User::create(
        &client,
        UserParams {
            fingerprint: "vote-user-1".to_string(),
            username: "voter1".to_string(),
            settings: serde_json::json!({}),
        },
    )
    .await
    .unwrap();

    let user2 = User::create(
        &client,
        UserParams {
            fingerprint: "vote-user-2".to_string(),
            username: "voter2".to_string(),
            settings: serde_json::json!({}),
        },
    )
    .await
    .unwrap();

    Vote::upsert(&client, user1.id, "lofi").await.unwrap();
    Vote::upsert(&client, user2.id, "classic").await.unwrap();

    // tally
    let (lofi, classic, ambient, jazz) = Vote::tally(&client).await.unwrap();
    assert_eq!(lofi, 1);
    assert_eq!(classic, 1);
    assert_eq!(ambient, 0);
    assert_eq!(jazz, 0);

    // clear all
    Vote::clear_all(&client).await.unwrap();
    let (lofi, classic, ambient, jazz) = Vote::tally(&client).await.unwrap();
    assert_eq!(lofi, 0);
    assert_eq!(classic, 0);
    assert_eq!(ambient, 0);
    assert_eq!(jazz, 0);
}
