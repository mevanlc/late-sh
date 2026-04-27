CREATE TABLE ssh_session_events (
    id         UUID        PRIMARY KEY DEFAULT uuidv7(),
    created    TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    user_id    UUID        REFERENCES users(id) ON DELETE SET NULL,
    event_type TEXT        NOT NULL CHECK (event_type IN ('connect', 'disconnect', 'server_shutdown'))
);

CREATE INDEX idx_ssh_session_events_created
    ON ssh_session_events (created);

CREATE INDEX idx_ssh_session_events_user_id
    ON ssh_session_events (user_id)
    WHERE user_id IS NOT NULL;
