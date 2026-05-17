CREATE SEQUENCE artboard_snapshot_number_seq;

ALTER TABLE artboard_snapshots
ADD COLUMN snapshot_number BIGINT,
ADD COLUMN curated BOOLEAN NOT NULL DEFAULT false,
ADD COLUMN hidden BOOLEAN NOT NULL DEFAULT false;

WITH numbered AS (
    SELECT id, row_number() OVER (ORDER BY created ASC, board_key ASC) AS snapshot_number
    FROM artboard_snapshots
)
UPDATE artboard_snapshots AS snapshots
SET snapshot_number = numbered.snapshot_number
FROM numbered
WHERE snapshots.id = numbered.id;

SELECT setval(
    'artboard_snapshot_number_seq',
    COALESCE((SELECT MAX(snapshot_number) FROM artboard_snapshots), 0) + 1,
    false
);

ALTER TABLE artboard_snapshots
ALTER COLUMN snapshot_number SET DEFAULT nextval('artboard_snapshot_number_seq'),
ALTER COLUMN snapshot_number SET NOT NULL;

ALTER SEQUENCE artboard_snapshot_number_seq
OWNED BY artboard_snapshots.snapshot_number;

ALTER TABLE artboard_snapshots
ADD CONSTRAINT artboard_snapshots_snapshot_number_key UNIQUE (snapshot_number);

CREATE INDEX idx_artboard_snapshots_visible_number
ON artboard_snapshots (hidden, snapshot_number DESC);
