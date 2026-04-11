use late_core::models::user::{User, UserParams};
use late_core::test_utils::{TestDb, test_db};
use tokio::time::{Duration, sleep};

async fn setup_db() -> (deadpool_postgres::Client, TestDb) {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("failed to get connection");

    client
        .execute(
            "CREATE TEMP TABLE users (
            id uuid primary key default uuidv7(),
            created timestamptz not null default current_timestamp,
            updated timestamptz not null default current_timestamp,
            last_seen timestamptz not null default current_timestamp,
            is_admin boolean not null default false,
            fingerprint text not null,
            username text not null default '',
            settings jsonb not null default '{}',
            unique (fingerprint)
        )",
            &[],
        )
        .await
        .expect("failed to create temp users table");

    (client, test_db)
}

#[tokio::test]
async fn user_fingerprint_lookup() {
    let (client, _test_db) = setup_db().await;

    let fingerprint = "fp-test-123";

    let created = User::create(
        &client,
        UserParams {
            fingerprint: fingerprint.to_string(),
            username: "test_user".to_string(),
            settings: serde_json::json!({}),
        },
    )
    .await
    .expect("failed to create user");

    let found = User::find_by_fingerprint(&client, fingerprint)
        .await
        .expect("lookup failed")
        .expect("user not found");

    assert_eq!(found.id, created.id);
    assert_eq!(found.fingerprint, fingerprint);
}

#[tokio::test]
async fn user_last_seen_updates_without_touching_updated() {
    let (client, _test_db) = setup_db().await;

    let mut user = User::create(
        &client,
        UserParams {
            fingerprint: "fp-presence".to_string(),
            username: "presence_user".to_string(),
            settings: serde_json::json!({}),
        },
    )
    .await
    .expect("failed to create user");

    let initial_updated = user.updated;
    let initial_last_seen = user.last_seen;

    sleep(Duration::from_millis(50)).await;

    user.update_last_seen(&client)
        .await
        .expect("failed to update last_seen");

    let fresh = User::get(&client, user.id)
        .await
        .expect("get failed")
        .unwrap();

    assert!(
        fresh.last_seen > initial_last_seen,
        "last_seen should have increased"
    );
    assert_eq!(
        fresh.updated, initial_updated,
        "updated should NOT have changed when only updating presence"
    );
}

#[tokio::test]
async fn user_update_modifies_updated_timestamp() {
    let (client, _test_db) = setup_db().await;

    let user = User::create(
        &client,
        UserParams {
            fingerprint: "fp-edit".to_string(),
            username: "edit_user".to_string(),
            settings: serde_json::json!({}),
        },
    )
    .await
    .expect("failed to create user");

    let initial_updated = user.updated;

    sleep(Duration::from_millis(50)).await;

    let updated_user = User::update(
        &client,
        user.id,
        UserParams {
            fingerprint: "fp-edit".to_string(),
            username: "edited_user".to_string(),
            settings: serde_json::json!({"theme": "dark"}),
        },
    )
    .await
    .expect("failed to update user");

    assert!(
        updated_user.updated > initial_updated,
        "updated timestamp SHOULD have increased after profile edit"
    );
    assert_eq!(updated_user.username, "edited_user");
}
