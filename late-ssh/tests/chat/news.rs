use late_core::models::article::ArticleEvent;
use late_core::models::article::{Article, ArticleParams};
use late_ssh::app::ai::svc::AiService;
use late_ssh::app::chat::news::svc::ArticleService;
use late_ssh::app::chat::notifications::svc::NotificationService;
use late_ssh::app::chat::svc::ChatService;
use tokio::time::{Duration, timeout};

use super::helpers::new_test_db;
use late_core::test_utils::create_test_user;

fn make_article_service(db: late_core::db::Db) -> ArticleService {
    let ai = AiService::new(false, None, "gemini-3.1-pro-preview".to_string());
    let notif = NotificationService::new(db.clone());
    let chat = ChatService::new(db.clone(), notif);
    ArticleService::new(db, ai, chat)
}

#[tokio::test]
async fn list_articles_publishes_snapshot_with_seeded_articles() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = create_test_user(&test_db.db, "article-author").await;

    Article::create_by_user_id(
        &client,
        user.id,
        ArticleParams {
            user_id: user.id,
            url: "https://example.com/one".to_string(),
            title: "First Post".to_string(),
            summary: "Summary one".to_string(),
            ascii_art: ".:-\n+*#".to_string(),
        },
    )
    .await
    .expect("create article");

    let service = make_article_service(test_db.db.clone());
    let mut snapshot_rx = service.subscribe_snapshot();

    service.list_articles_task();

    timeout(Duration::from_secs(2), snapshot_rx.changed())
        .await
        .expect("snapshot timeout")
        .expect("watch changed");
    let snapshot = snapshot_rx.borrow_and_update().clone();

    assert_eq!(snapshot.articles.len(), 1);
    assert_eq!(snapshot.articles[0].article.title, "First Post");
    assert_eq!(snapshot.articles[0].author_username, "article-author");
    assert!(snapshot.user_id.is_none(), "global feed has no target");
}

#[tokio::test]
async fn list_articles_publishes_empty_snapshot_when_no_articles_exist() {
    let test_db = new_test_db().await;
    let service = make_article_service(test_db.db.clone());
    let mut snapshot_rx = service.subscribe_snapshot();

    service.list_articles_task();

    timeout(Duration::from_secs(2), snapshot_rx.changed())
        .await
        .expect("snapshot timeout")
        .expect("watch changed");
    let snapshot = snapshot_rx.borrow_and_update().clone();

    assert!(snapshot.articles.is_empty());
}

#[tokio::test]
async fn list_articles_resolves_multiple_authors() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let alice = create_test_user(&test_db.db, "alice-art").await;
    let bob = create_test_user(&test_db.db, "bob-art").await;

    Article::create_by_user_id(
        &client,
        alice.id,
        ArticleParams {
            user_id: alice.id,
            url: "https://example.com/alice".to_string(),
            title: "Alice Article".to_string(),
            summary: "By alice".to_string(),
            ascii_art: "...".to_string(),
        },
    )
    .await
    .expect("alice article");

    Article::create_by_user_id(
        &client,
        bob.id,
        ArticleParams {
            user_id: bob.id,
            url: "https://example.com/bob".to_string(),
            title: "Bob Article".to_string(),
            summary: "By bob".to_string(),
            ascii_art: "###".to_string(),
        },
    )
    .await
    .expect("bob article");

    let service = make_article_service(test_db.db.clone());
    let mut snapshot_rx = service.subscribe_snapshot();

    service.list_articles_task();

    timeout(Duration::from_secs(2), snapshot_rx.changed())
        .await
        .expect("snapshot timeout")
        .expect("watch changed");
    let snapshot = snapshot_rx.borrow_and_update().clone();

    assert_eq!(snapshot.articles.len(), 2);
    let usernames: Vec<&str> = snapshot
        .articles
        .iter()
        .map(|item| item.author_username.as_str())
        .collect();
    assert!(usernames.contains(&"alice-art"));
    assert!(usernames.contains(&"bob-art"));
}

#[tokio::test]
async fn process_url_emits_failed_event_for_duplicate_url() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = create_test_user(&test_db.db, "dup-article-user").await;

    Article::create_by_user_id(
        &client,
        user.id,
        ArticleParams {
            user_id: user.id,
            url: "https://example.com/duplicate".to_string(),
            title: "Already Exists".to_string(),
            summary: "Old summary".to_string(),
            ascii_art: "...".to_string(),
        },
    )
    .await
    .expect("seed article");

    let service = make_article_service(test_db.db.clone());
    let mut events = service.subscribe_events();

    service.process_url(user.id, "https://example.com/duplicate");

    let event = timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("event timeout")
        .expect("event");
    match event {
        ArticleEvent::Failed { user_id, error } => {
            assert_eq!(user_id, user.id);
            assert!(
                error.contains("exists"),
                "error should mention duplicate: {error}"
            );
        }
        other => panic!("expected Failed event, got {other:?}"),
    }
}

#[tokio::test]
async fn list_articles_snapshot_updates_after_direct_db_insert() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = create_test_user(&test_db.db, "refresh-user").await;

    let service = make_article_service(test_db.db.clone());
    let mut snapshot_rx = service.subscribe_snapshot();

    // First list: empty
    service.list_articles_task();
    timeout(Duration::from_secs(2), snapshot_rx.changed())
        .await
        .expect("first snapshot timeout")
        .expect("watch changed");
    let snap1 = snapshot_rx.borrow_and_update().clone();
    assert!(snap1.articles.is_empty());

    // Insert directly into DB
    Article::create_by_user_id(
        &client,
        user.id,
        ArticleParams {
            user_id: user.id,
            url: "https://example.com/new-after".to_string(),
            title: "Appeared Later".to_string(),
            summary: "Fresh content".to_string(),
            ascii_art: "+++".to_string(),
        },
    )
    .await
    .expect("insert article");

    // Second list: should pick up the new article
    service.list_articles_task();
    timeout(Duration::from_secs(2), snapshot_rx.changed())
        .await
        .expect("second snapshot timeout")
        .expect("watch changed");
    let snap2 = snapshot_rx.borrow_and_update().clone();
    assert_eq!(snap2.articles.len(), 1);
    assert_eq!(snap2.articles[0].article.title, "Appeared Later");
}
