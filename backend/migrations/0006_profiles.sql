CREATE TABLE profiles (
    user_id       UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    display_name  TEXT NOT NULL DEFAULT '',
    headline      TEXT NOT NULL DEFAULT '',
    bio           TEXT NOT NULL DEFAULT '',
    city          TEXT NOT NULL DEFAULT '',
    country       TEXT NOT NULL DEFAULT '',
    timezone      TEXT NOT NULL DEFAULT '',
    linkedin_url  TEXT,
    github_url    TEXT,
    website_url   TEXT,
    roles         TEXT[] NOT NULL DEFAULT '{}',
    seeking_roles TEXT[] NOT NULL DEFAULT '{}',
    idea_status   TEXT CHECK (idea_status IN ('committed_idea', 'flexible_idea', 'looking_to_join')),
    stage         TEXT CHECK (stage IN ('idea', 'prototype', 'users', 'revenue')),
    commitment    TEXT CHECK (commitment IN ('full_time_now', 'full_time_when_funded', 'part_time', 'exploring')),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),

    -- The role vocabulary is fixed by the design and feeds the scorer, so the
    -- database enforces it too rather than trusting the service layer alone.
    CONSTRAINT roles_are_known CHECK (
        roles <@ ARRAY['engineering', 'design', 'product', 'gtm', 'ops_finance', 'research']::TEXT[]
    ),
    CONSTRAINT seeking_roles_are_known CHECK (
        seeking_roles <@ ARRAY['engineering', 'design', 'product', 'gtm', 'ops_finance', 'research']::TEXT[]
    )
);
