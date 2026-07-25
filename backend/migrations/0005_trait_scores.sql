-- Derived from question_responses. A row exists only when all 18 questions
-- have been answered, so the deck can join here and be certain every axis is
-- comparable.
CREATE TABLE trait_scores (
    user_id        UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    risk_tolerance SMALLINT NOT NULL CHECK (risk_tolerance BETWEEN 0 AND 100),
    pace_vs_rigor  SMALLINT NOT NULL CHECK (pace_vs_rigor BETWEEN 0 AND 100),
    conflict_style SMALLINT NOT NULL CHECK (conflict_style BETWEEN 0 AND 100),
    decision_basis SMALLINT NOT NULL CHECK (decision_basis BETWEEN 0 AND 100),
    work_mode      SMALLINT NOT NULL CHECK (work_mode BETWEEN 0 AND 100),
    orientation    SMALLINT NOT NULL CHECK (orientation BETWEEN 0 AND 100),
    computed_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
