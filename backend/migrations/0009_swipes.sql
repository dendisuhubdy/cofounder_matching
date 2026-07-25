-- Both directions are permanent and both exclude the target from future
-- decks, so the pair is the primary key rather than a surrogate id.
CREATE TABLE swipes (
    swiper_id  UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    target_id  UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    direction  TEXT NOT NULL CHECK (direction IN ('left', 'right')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (swiper_id, target_id),
    CONSTRAINT no_self_swipe CHECK (swiper_id <> target_id)
);

-- Serves the popularity adjustment, which reads recent swipes by target.
CREATE INDEX swipes_target_recent_idx ON swipes (target_id, created_at DESC);

-- Serves the pass-suppression adjustment, which reads a viewer's recent
-- left swipes.
CREATE INDEX swipes_swiper_recent_idx ON swipes (swiper_id, created_at DESC);
