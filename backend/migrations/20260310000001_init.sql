CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

CREATE TABLE IF NOT EXISTS decisions (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    family_id UUID NOT NULL,
    title VARCHAR(200) NOT NULL,
    description TEXT,
    options JSONB NOT NULL DEFAULT '[]',
    recommendation VARCHAR(200),
    status VARCHAR(20) NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'decided', 'archived')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS happiness_scores (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    family_id UUID NOT NULL,
    score_people DOUBLE PRECISION NOT NULL DEFAULT 0,
    score_time DOUBLE PRECISION NOT NULL DEFAULT 0,
    score_tasks DOUBLE PRECISION NOT NULL DEFAULT 0,
    score_things DOUBLE PRECISION NOT NULL DEFAULT 0,
    score_spaces DOUBLE PRECISION NOT NULL DEFAULT 0,
    total_score DOUBLE PRECISION NOT NULL DEFAULT 0,
    computed_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_decisions_family_id ON decisions(family_id);
CREATE INDEX IF NOT EXISTS idx_happiness_family_id ON happiness_scores(family_id);
