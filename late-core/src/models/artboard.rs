use anyhow::Result;
use serde_json::Value;
use tokio_postgres::Client;

crate::model! {
    table = "artboard_snapshots";
    params = SnapshotParams;
    struct Snapshot {
        @data
        pub board_key: String,
        pub canvas: Value,
    }
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

    pub async fn upsert(client: &Client, board_key: &str, canvas: Value) -> Result<Self> {
        let row = client
            .query_one(
                "INSERT INTO artboard_snapshots (board_key, canvas)
                 VALUES ($1, $2)
                 ON CONFLICT (board_key) DO UPDATE
                 SET canvas = EXCLUDED.canvas, updated = current_timestamp
                 RETURNING *",
                &[&board_key, &canvas],
            )
            .await?;
        Ok(Self::from(row))
    }
}
