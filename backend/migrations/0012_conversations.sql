-- One conversation per pair, so the ids are stored in a fixed order and the
-- unique constraint does the deduplication rather than application code.
-- `started_by` is what the new-conversation rate limit counts: without it,
-- being messaged by ten people would exhaust your own allowance.
CREATE TABLE conversations (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_a_id       UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    user_b_id       UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    started_by      UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_message_at TIMESTAMPTZ,
    UNIQUE (user_a_id, user_b_id),
    CONSTRAINT ordered_pair CHECK (user_a_id < user_b_id)
);

CREATE INDEX conversations_user_a_idx ON conversations (user_a_id);
CREATE INDEX conversations_user_b_idx ON conversations (user_b_id);
-- Serves the rolling 24-hour new-conversation limit.
CREATE INDEX conversations_started_by_idx ON conversations (started_by, created_at DESC);
