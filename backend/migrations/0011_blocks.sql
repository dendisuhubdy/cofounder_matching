-- Created in slice 3 although POST /blocks arrives in slice 4: the deck's
-- candidate query excludes blocked pairs from the start, so the most
-- safety-critical filter is never bolted on afterwards. Until slice 4 the
-- table is simply empty.
CREATE TABLE blocks (
    blocker_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    blocked_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (blocker_id, blocked_id),
    CONSTRAINT no_self_block CHECK (blocker_id <> blocked_id)
);

-- The deck filters on being blocked as well as on blocking.
CREATE INDEX blocks_blocked_idx ON blocks (blocked_id);
