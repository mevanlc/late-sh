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
}
