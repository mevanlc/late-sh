use late_core::models::artboard::Snapshot;
use late_core::test_utils::test_db;

#[tokio::test]
async fn artboard_snapshot_upsert_replaces_existing_canvas() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("failed to get connection");

    let first_canvas = serde_json::json!({
        "width": 384,
        "height": 192,
        "cells": [],
        "colors": [],
    });
    let second_canvas = serde_json::json!({
        "width": 384,
        "height": 192,
        "cells": [[{"x": 3, "y": 2}, {"Narrow": "A"}]],
        "colors": [],
    });

    Snapshot::upsert(&client, Snapshot::MAIN_BOARD_KEY, first_canvas)
        .await
        .expect("insert snapshot");
    let updated = Snapshot::upsert(&client, Snapshot::MAIN_BOARD_KEY, second_canvas.clone())
        .await
        .expect("update snapshot");

    assert_eq!(updated.canvas, second_canvas);

    let reloaded = Snapshot::find_by_board_key(&client, Snapshot::MAIN_BOARD_KEY)
        .await
        .expect("reload snapshot")
        .expect("snapshot exists");
    assert_eq!(reloaded.canvas, second_canvas);

    let count = client
        .query_one(
            "SELECT COUNT(*)::int AS count FROM artboard_snapshots WHERE board_key = $1",
            &[&Snapshot::MAIN_BOARD_KEY],
        )
        .await
        .expect("count snapshots")
        .get::<_, i32>("count");
    assert_eq!(count, 1);
}
