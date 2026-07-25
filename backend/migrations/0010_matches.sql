-- The pair is stored in a fixed order so that a match between two people is
-- one row rather than two, and so the primary key catches duplicates.
CREATE TABLE matches (
    user_a_id  UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    user_b_id  UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_a_id, user_b_id),
    CONSTRAINT ordered_pair CHECK (user_a_id < user_b_id)
);

CREATE INDEX matches_user_b_idx ON matches (user_b_id);
