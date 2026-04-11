use anyhow::Result;
use chrono::{DateTime, Utc};
use tokio_postgres::Client;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct ArticleFeedRead {
    pub user_id: Uuid,
    pub last_read_created: Option<DateTime<Utc>>,
    pub last_read_article_id: Option<Uuid>,
}

impl ArticleFeedRead {
    pub async fn mark_read_latest(client: &Client, user_id: Uuid) -> Result<()> {
        let latest = client
            .query_opt(
                "SELECT created, id
                 FROM articles
                 ORDER BY created DESC, id DESC
                 LIMIT 1",
                &[],
            )
            .await?;

        let (last_read_created, last_read_article_id): (Option<DateTime<Utc>>, Option<Uuid>) =
            if let Some(row) = latest {
                (Some(row.get("created")), Some(row.get("id")))
            } else {
                (None, None)
            };

        client
            .execute(
                "INSERT INTO article_feed_reads (user_id, last_read_created, last_read_article_id, updated)
                 VALUES ($1, $2, $3, current_timestamp)
                 ON CONFLICT (user_id)
                 DO UPDATE SET
                   last_read_created = EXCLUDED.last_read_created,
                   last_read_article_id = EXCLUDED.last_read_article_id,
                   updated = current_timestamp",
                &[&user_id, &last_read_created, &last_read_article_id],
            )
            .await?;

        Ok(())
    }

    pub async fn unread_count_for_user(client: &Client, user_id: Uuid) -> Result<i64> {
        let row = client
            .query_one(
                "SELECT COUNT(a.id)::bigint AS unread_count
                 FROM articles a
                 LEFT JOIN article_feed_reads afr ON afr.user_id = $1
                 WHERE
                   afr.user_id IS NULL
                   OR (a.created, a.id) > (
                        COALESCE(afr.last_read_created, '-infinity'::timestamptz),
                        COALESCE(afr.last_read_article_id, '00000000-0000-0000-0000-000000000000'::uuid)
                   )",
                &[&user_id],
            )
            .await?;
        Ok(row.get("unread_count"))
    }
}
