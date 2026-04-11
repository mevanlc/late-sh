use chrono::{Duration, Utc};
use late_core::{
    models::{
        bonsai::{Grave, Tree},
        user::{User, UserParams},
    },
    test_utils::test_db,
};

#[tokio::test]
async fn ensure_and_find_by_user_round_trip() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = User::create(
        &client,
        UserParams {
            fingerprint: "bonsai-model-user".to_string(),
            username: "bonsai-model-user".to_string(),
            settings: serde_json::json!({}),
        },
    )
    .await
    .expect("create user");

    let tree = Tree::ensure(&client, user.id, 1234).await.expect("ensure");
    assert_eq!(tree.user_id, user.id);
    assert_eq!(tree.seed, 1234);
    assert_eq!(tree.growth_points, 0);
    assert_eq!(tree.last_watered, None);
    assert!(tree.is_alive);

    let fetched = Tree::find_by_user_id(&client, user.id)
        .await
        .expect("find")
        .expect("tree");
    assert_eq!(fetched.id, tree.id);
    assert_eq!(fetched.seed, 1234);
}

#[tokio::test]
async fn tree_mutations_and_graveyard_round_trip() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = User::create(
        &client,
        UserParams {
            fingerprint: "bonsai-model-mutations".to_string(),
            username: "bonsai-model-mutations".to_string(),
            settings: serde_json::json!({}),
        },
    )
    .await
    .expect("create user");
    let today = Utc::now().date_naive();

    Tree::ensure(&client, user.id, 11).await.expect("ensure");
    Tree::water(&client, user.id, today).await.expect("water");
    Tree::add_growth(&client, user.id, 17)
        .await
        .expect("add growth");
    Tree::kill(&client, user.id).await.expect("kill");

    let dead = Tree::find_by_user_id(&client, user.id)
        .await
        .expect("find dead")
        .expect("dead tree");
    assert_eq!(dead.last_watered, Some(today));
    assert_eq!(dead.growth_points, 17);
    assert!(!dead.is_alive);

    Grave::record(&client, user.id, 8)
        .await
        .expect("record grave");
    let graves = Grave::list_by_user(&client, user.id)
        .await
        .expect("list graves");
    assert_eq!(graves.len(), 1);
    assert_eq!(graves[0].survived_days, 8);

    let old_created = dead.created;
    Tree::respawn(&client, user.id, 99).await.expect("respawn");
    let respawned = Tree::find_by_user_id(&client, user.id)
        .await
        .expect("find respawned")
        .expect("respawned tree");
    assert!(respawned.is_alive);
    assert_eq!(respawned.growth_points, 0);
    assert_eq!(respawned.last_watered, None);
    assert_eq!(respawned.seed, 99);
    assert!(respawned.created >= old_created);
    assert!(respawned.updated >= respawned.created - Duration::seconds(1));
}
