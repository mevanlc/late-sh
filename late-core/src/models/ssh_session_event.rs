use anyhow::Result;
use chrono::{DateTime, Utc};
use tokio_postgres::Client;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct SshSessionEvent {
    pub id: Uuid,
    pub created: DateTime<Utc>,
    pub user_id: Option<Uuid>,
    pub event_type: String,
}

impl From<tokio_postgres::Row> for SshSessionEvent {
    fn from(row: tokio_postgres::Row) -> Self {
        Self {
            id: row.get("id"),
            created: row.get("created"),
            user_id: row.get("user_id"),
            event_type: row.get("event_type"),
        }
    }
}

pub enum SshEventType {
    Connect,
    Disconnect,
    ServerShutdown,
}

impl SshEventType {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Connect => "connect",
            Self::Disconnect => "disconnect",
            Self::ServerShutdown => "server_shutdown",
        }
    }
}

impl SshSessionEvent {
    pub async fn record(
        client: &Client,
        user_id: Option<Uuid>,
        event_type: SshEventType,
    ) -> Result<()> {
        client
            .execute(
                "INSERT INTO ssh_session_events (user_id, event_type) VALUES ($1, $2)",
                &[&user_id, &event_type.as_str()],
            )
            .await?;
        Ok(())
    }

    /// Deletes events older than `retain_days`. Returns the number of rows removed.
    pub async fn prune(client: &Client, retain_days: i64) -> Result<u64> {
        let cutoff = Utc::now() - chrono::Duration::days(retain_days);
        let count = client
            .execute(
                "DELETE FROM ssh_session_events WHERE created < $1",
                &[&cutoff],
            )
            .await?;
        Ok(count)
    }
}
