use anyhow::Result;
use chrono::{DateTime, Utc};
use deadpool_postgres::GenericClient;
use serde_json::Value;
use tokio_postgres::Client;

crate::model! {
    table = "artboard_snapshots";
    params = SnapshotParams;
    struct Snapshot {
        @data
        pub snapshot_number: i64,
        pub board_key: String,
        pub canvas: Value,
        pub provenance: Value,
        pub curated: bool,
        pub hidden: bool,
    }
}

#[derive(Debug)]
pub struct SnapshotSummary {
    pub snapshot_number: i64,
    pub board_key: String,
    pub updated: DateTime<Utc>,
    pub curated: bool,
    pub hidden: bool,
}

impl Snapshot {
    pub const MAIN_BOARD_KEY: &'static str = "main";

    pub async fn find_by_board_key(client: &Client, board_key: &str) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT * FROM artboard_snapshots WHERE board_key = $1",
                &[&board_key],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    pub async fn find_by_snapshot_number(
        client: &Client,
        snapshot_number: i64,
    ) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT * FROM artboard_snapshots WHERE snapshot_number = $1",
                &[&snapshot_number],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    pub async fn list_by_board_key_prefix(client: &Client, prefix: &str) -> Result<Vec<Self>> {
        let pattern = format!("{prefix}%");
        let rows = client
            .query(
                "SELECT * FROM artboard_snapshots
                 WHERE board_key LIKE $1
                 ORDER BY board_key DESC, created DESC",
                &[&pattern],
            )
            .await?;
        Ok(rows.into_iter().map(Self::from).collect())
    }

    pub async fn find_summary_by_board_key(
        client: &Client,
        board_key: &str,
    ) -> Result<Option<SnapshotSummary>> {
        let row = client
            .query_opt(
                "SELECT snapshot_number, board_key, updated, curated, hidden
                 FROM artboard_snapshots WHERE board_key = $1",
                &[&board_key],
            )
            .await?;
        Ok(row.map(|row| SnapshotSummary {
            snapshot_number: row.get("snapshot_number"),
            board_key: row.get("board_key"),
            updated: row.get("updated"),
            curated: row.get("curated"),
            hidden: row.get("hidden"),
        }))
    }

    pub async fn list_summaries_by_board_key_prefix(
        client: &Client,
        prefix: &str,
    ) -> Result<Vec<SnapshotSummary>> {
        let pattern = format!("{prefix}%");
        let rows = client
            .query(
                "SELECT snapshot_number, board_key, updated, curated, hidden
                 FROM artboard_snapshots
                 WHERE board_key LIKE $1
                 ORDER BY board_key DESC, created DESC",
                &[&pattern],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| SnapshotSummary {
                snapshot_number: row.get("snapshot_number"),
                board_key: row.get("board_key"),
                updated: row.get("updated"),
                curated: row.get("curated"),
                hidden: row.get("hidden"),
            })
            .collect())
    }

    pub async fn list_archive_summaries(
        client: &Client,
        include_hidden: bool,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<SnapshotSummary>> {
        let rows = client
            .query(
                "SELECT snapshot_number, board_key, updated, curated, hidden
                 FROM artboard_snapshots
                 WHERE board_key <> $1
                   AND (board_key LIKE 'daily:%' OR board_key LIKE 'monthly:%' OR curated)
                   AND ($2 OR NOT hidden)
                 ORDER BY snapshot_number DESC
                 LIMIT $3 OFFSET $4",
                &[&Self::MAIN_BOARD_KEY, &include_hidden, &limit, &offset],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| SnapshotSummary {
                snapshot_number: row.get("snapshot_number"),
                board_key: row.get("board_key"),
                updated: row.get("updated"),
                curated: row.get("curated"),
                hidden: row.get("hidden"),
            })
            .collect())
    }

    pub async fn list_archives(
        client: &Client,
        include_hidden: bool,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Self>> {
        let rows = client
            .query(
                "SELECT *
                 FROM artboard_snapshots
                 WHERE board_key <> $1
                   AND (board_key LIKE 'daily:%' OR board_key LIKE 'monthly:%' OR curated)
                   AND ($2 OR NOT hidden)
                 ORDER BY snapshot_number DESC
                 LIMIT $3 OFFSET $4",
                &[&Self::MAIN_BOARD_KEY, &include_hidden, &limit, &offset],
            )
            .await?;
        Ok(rows.into_iter().map(Self::from).collect())
    }

    pub async fn delete_by_board_key(client: &Client, board_key: &str) -> Result<u64> {
        let count = client
            .execute(
                "DELETE FROM artboard_snapshots WHERE board_key = $1",
                &[&board_key],
            )
            .await?;
        Ok(count)
    }

    pub async fn copy_board_key(
        client: &impl GenericClient,
        source_key: &str,
        target_key: &str,
    ) -> Result<u64> {
        let count = client
            .execute(
                "INSERT INTO artboard_snapshots (board_key, canvas, provenance)
                 SELECT $1, canvas, provenance
                 FROM artboard_snapshots
                 WHERE board_key = $2
                 ON CONFLICT (board_key) DO UPDATE
                 SET canvas = EXCLUDED.canvas,
                     provenance = EXCLUDED.provenance,
                     updated = current_timestamp",
                &[&target_key, &source_key],
            )
            .await?;
        Ok(count)
    }

    pub async fn copy_board_key_with_flags(
        client: &impl GenericClient,
        source_key: &str,
        target_key: &str,
        curated: bool,
        hidden: bool,
    ) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "INSERT INTO artboard_snapshots (board_key, canvas, provenance, curated, hidden)
                 SELECT $1, canvas, provenance, $3, $4
                 FROM artboard_snapshots
                 WHERE board_key = $2
                 ON CONFLICT (board_key) DO UPDATE
                 SET canvas = EXCLUDED.canvas,
                     provenance = EXCLUDED.provenance,
                     curated = EXCLUDED.curated,
                     hidden = EXCLUDED.hidden,
                     updated = current_timestamp
                 RETURNING *",
                &[&target_key, &source_key, &curated, &hidden],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    pub async fn set_hidden_by_snapshot_number(
        client: &impl GenericClient,
        snapshot_number: i64,
        hidden: bool,
    ) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "UPDATE artboard_snapshots
                 SET hidden = $2, updated = current_timestamp
                 WHERE snapshot_number = $1
                 RETURNING *",
                &[&snapshot_number, &hidden],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    pub async fn upsert(
        client: &Client,
        board_key: &str,
        canvas: Value,
        provenance: Value,
    ) -> Result<Self> {
        let row = client
            .query_one(
                "INSERT INTO artboard_snapshots (board_key, canvas, provenance)
                 VALUES ($1, $2, $3)
                 ON CONFLICT (board_key) DO UPDATE
                 SET canvas = EXCLUDED.canvas,
                     provenance = EXCLUDED.provenance,
                     updated = current_timestamp
                 RETURNING *",
                &[&board_key, &canvas, &provenance],
            )
            .await?;
        Ok(Self::from(row))
    }
}
