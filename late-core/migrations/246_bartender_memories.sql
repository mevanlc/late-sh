-- What @bartender remembers about a patron: one row per exchange he
-- answered (the patron's order, his reply, and up to five more lines the
-- patron said in that room until five seconds after the reply landed).
--
-- The words are copied, not referenced: a memory lives by its own rules,
-- whatever happens to the chat log it came from. Only three things end
-- one: the cap below, the patron saying "forget me", and the account
-- going away.
--
-- A patron keeps at most sixteen. The eight most recent rows whose order
-- said "remember" are kept first; every other slot goes to the newest of
-- the rest, so a patron who asks him to remember everything still has
-- room for new nights. `models/bartender_memory.rs` holds the rule.
CREATE TABLE bartender_memories (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- The order's chat message id, kept only so one exchange is stored
    -- once. Deliberately not a foreign key.
    order_message_id UUID NOT NULL UNIQUE,
    -- The order's own `created`: what eviction and the reset below compare.
    ordered_at TIMESTAMPTZ NOT NULL,
    order_body TEXT NOT NULL,
    replied_at TIMESTAMPTZ NOT NULL,
    reply_body TEXT NOT NULL,
    trailing_bodies TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
    remember BOOLEAN NOT NULL
);

CREATE INDEX bartender_memories_by_user
    ON bartender_memories (user_id, ordered_at DESC);

-- When a patron last said "forget me". No exchange whose order is at or
-- before it may be stored, so one still in flight when they said it (the
-- model call, the five-second tail) cannot land after the wipe.
CREATE TABLE bartender_memory_resets (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    reset_at TIMESTAMPTZ NOT NULL
);
