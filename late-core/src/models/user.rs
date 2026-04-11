use anyhow::Result;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use tokio_postgres::Client;
use uuid::Uuid;

crate::model! {
    table = "users";
    params = UserParams;
    struct User {
        @generated
        pub last_seen: DateTime<Utc>,
        pub is_admin: bool;

        @data
        pub fingerprint: String,
        pub username: String,
        pub settings: serde_json::Value,
    }
}

impl User {
    pub async fn find_by_fingerprint(client: &Client, fingerprint: &str) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT u.id, u.created, u.updated, u.last_seen, u.is_admin, u.fingerprint, COALESCE(p.username, '') AS username, u.settings
                 FROM users u
                 LEFT JOIN profiles p ON u.id = p.user_id
                 WHERE u.fingerprint = $1",
                &[&fingerprint],
            )
            .await?;
        Ok(row.map(Self::from))
    }
    pub async fn update_last_seen(&mut self, client: &Client) -> Result<()> {
        self.last_seen = Utc::now();
        client
            .execute(
                &format!("UPDATE {} SET last_seen = $1 WHERE id = $2", Self::TABLE),
                &[&self.last_seen, &self.id],
            )
            .await?;
        Ok(())
    }

    pub async fn list_usernames_by_ids(
        client: &Client,
        user_ids: &[Uuid],
    ) -> Result<HashMap<Uuid, String>> {
        if user_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let rows = client
            .query(
                "SELECT p.user_id AS id, p.username
                 FROM profiles p
                 WHERE p.user_id = ANY($1)",
                &[&user_ids],
            )
            .await?;

        let mut usernames = HashMap::with_capacity(rows.len());
        for row in rows {
            usernames.insert(row.get("id"), row.get("username"));
        }
        Ok(usernames)
    }

    pub async fn list_all_usernames(client: &Client) -> Result<Vec<String>> {
        let rows = client
            .query(
                "SELECT p.username FROM profiles p
                 WHERE p.username IS NOT NULL AND p.username != ''
                 ORDER BY p.username",
                &[],
            )
            .await?;
        Ok(rows.iter().map(|r| r.get("username")).collect())
    }

    pub async fn list_all_username_map(client: &Client) -> Result<HashMap<Uuid, String>> {
        let rows = client
            .query(
                "SELECT p.user_id AS id, p.username
                 FROM profiles p
                 WHERE p.username IS NOT NULL AND p.username != ''",
                &[],
            )
            .await?;
        let mut map = HashMap::with_capacity(rows.len());
        for row in rows {
            map.insert(row.get("id"), row.get("username"));
        }
        Ok(map)
    }

    pub async fn find_by_username(client: &Client, username: &str) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT u.id, u.created, u.updated, u.last_seen, u.is_admin, u.fingerprint,
                        p.username AS username, u.settings
                 FROM users u
                 JOIN profiles p ON u.id = p.user_id
                 WHERE LOWER(p.username) = LOWER($1)",
                &[&username],
            )
            .await?;
        Ok(row.map(Self::from))
    }
}
