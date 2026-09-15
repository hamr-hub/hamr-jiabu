-- iter-loop-20260901: Decision audit trail
-- Adds columns to record who/when/why a decision status changed.
-- Addresses "决策可追溯" gap surfaced in round-1 audit:
--   status flips (open -> decided / archived) had no actor / timestamp / rationale.
--
-- Privacy default: rationale is OPTIONAL. existing rows get NULL (no retroactive inference).
-- All new columns are nullable so the migration is non-blocking and does not require
-- application-level backfill.

ALTER TABLE decisions
    ADD COLUMN IF NOT EXISTS decided_by  VARCHAR(100),
    ADD COLUMN IF NOT EXISTS decided_at  TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS rationale   TEXT;

-- Partial index for audit queries: "what did this family decide recently?"
-- Only rows where status actually advanced (decided/archived) are indexed,
-- keeping the index small while supporting the common lookup pattern.
CREATE INDEX IF NOT EXISTS idx_decisions_audit
    ON decisions(family_id, decided_at DESC)
    WHERE decided_at IS NOT NULL;

COMMENT ON COLUMN decisions.decided_by IS 'Username from JWT claims who marked the decision decided/archived';
COMMENT ON COLUMN decisions.decided_at IS 'Timestamp when status transitioned away from open';
COMMENT ON COLUMN decisions.rationale  IS 'Optional free-text explanation captured at decision time (user-provided)';
