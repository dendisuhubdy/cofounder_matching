-- Industry tags are a curated list expected to grow, so unlike roles they are
-- validated in Rust only: adding a tag should not require a migration.
CREATE TABLE profile_interests (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    tag     TEXT NOT NULL,
    PRIMARY KEY (user_id, tag)
);

CREATE INDEX profile_interests_tag_idx ON profile_interests (tag);
