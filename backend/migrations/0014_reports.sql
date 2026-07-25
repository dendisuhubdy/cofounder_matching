-- Queued for manual review. Nothing here triggers automated action: a report
-- is evidence for a person to read, not a lever anyone can pull on someone
-- else's account.
CREATE TABLE reports (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    reporter_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    reported_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    reason      TEXT NOT NULL
                CHECK (reason IN ('harassment', 'spam', 'impersonation', 'off_topic', 'other')),
    body        TEXT NOT NULL DEFAULT '',
    status      TEXT NOT NULL DEFAULT 'pending'
                CHECK (status IN ('pending', 'reviewed', 'dismissed')),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT no_self_report CHECK (reporter_id <> reported_id)
);

CREATE INDEX reports_pending_idx ON reports (status, created_at DESC);
