use anyhow::Result;
use chrono::{DateTime, Utc};
use tokio_postgres::Client;
use uuid::Uuid;

crate::model! {
    table = "server_bans";
    params = ServerBanParams;
    struct ServerBan {
        @data
        pub target_user_id: Option<Uuid>,
        pub fingerprint: Option<String>,
        pub actor_user_id: Uuid,
        pub reason: String,
        pub expires_at: Option<DateTime<Utc>>,
    }
}

impl ServerBan {
    pub async fn activate(
        client: &Client,
        target_user_id: Uuid,
        fingerprint: &str,
        actor_user_id: Uuid,
        reason: &str,
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<Self> {
        Self::create(
            client,
            ServerBanParams {
                target_user_id: Some(target_user_id),
                fingerprint: Some(fingerprint.to_string()),
                actor_user_id,
                reason: reason.to_string(),
                expires_at,
            },
        )
        .await
    }

    pub async fn find_active_for_user_id(
        client: &Client,
        target_user_id: Uuid,
    ) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT *
                 FROM server_bans
                 WHERE target_user_id = $1
                   AND (expires_at IS NULL OR expires_at > current_timestamp)
                 ORDER BY created DESC
                 LIMIT 1",
                &[&target_user_id],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    pub async fn find_active_for_fingerprint(
        client: &Client,
        fingerprint: &str,
    ) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT *
                 FROM server_bans
                 WHERE fingerprint = $1
                   AND (expires_at IS NULL OR expires_at > current_timestamp)
                 ORDER BY created DESC
                 LIMIT 1",
                &[&fingerprint],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    pub async fn delete_active_for_user(
        client: &Client,
        target_user_id: Uuid,
        fingerprint: &str,
    ) -> Result<u64> {
        Ok(client
            .execute(
                "DELETE FROM server_bans
                 WHERE (target_user_id = $1 OR fingerprint = $2)
                   AND (expires_at IS NULL OR expires_at > current_timestamp)",
                &[&target_user_id, &fingerprint],
            )
            .await?)
    }
}
