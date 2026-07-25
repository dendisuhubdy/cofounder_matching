-- question_id carries no foreign key: the question bank lives in Rust, not in
-- a table. The set of valid ids is enforced in assessment::service.
CREATE TABLE question_responses (
    user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    question_id TEXT NOT NULL,
    value       SMALLINT NOT NULL CHECK (value BETWEEN 1 AND 5),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, question_id)
);
