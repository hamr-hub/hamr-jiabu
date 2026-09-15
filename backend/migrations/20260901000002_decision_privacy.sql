-- iter-loop-20260901 r2: Decision privacy defaults + TTL + share scope.
-- Addresses the round-1 P0 gap "decisions are implicitly shared": every decision
-- row was readable by anyone in the family_id partition without an explicit
-- consent step, and rows lived forever with no retention control.
--
-- Design (kept compatible with the round-1 audit columns):
--   * is_private defaults TRUE so existing rows + new writes are owner-family only.
--   * share_scope is normalized at write time: any non-private scope flips is_private
--     to FALSE, and a private scope flips is_private to TRUE — there is no
--     ambiguous "private but shared with AI" state.
--   * ttl_days is OPTIONAL. If set, expires_at = created_at + ttl_days at write.
--   * expires_at is the maintenance endpoint's filter — rows past it are purged.
--
-- Constraints use CHECK rather than triggers so the privacy invariants are
-- visible in the schema dump and survive any future ORM switch.

ALTER TABLE decisions
    ADD COLUMN IF NOT EXISTS is_private  BOOLEAN     NOT NULL DEFAULT TRUE,
    ADD COLUMN IF NOT EXISTS share_scope VARCHAR(32) NOT NULL DEFAULT 'private'
        CHECK (share_scope IN ('private', 'family', 'friends', 'public', 'ai_training')),
    ADD COLUMN IF NOT EXISTS ttl_days    INTEGER
        CHECK (ttl_days IS NULL OR (ttl_days >= 1 AND ttl_days <= 3650)),
    ADD COLUMN IF NOT EXISTS expires_at  TIMESTAMPTZ;

-- List endpoint filter: "show me my family's private decisions".
-- Composite (family_id, is_private, share_scope) avoids re-scanning the table
-- when the planner sees the typical access pattern.
CREATE INDEX IF NOT EXISTS idx_decisions_privacy
    ON decisions(family_id, is_private, share_scope);

-- Maintenance endpoint uses expires_at; partial index keeps it tiny
-- (only rows with an actual expiry contribute).
CREATE INDEX IF NOT EXISTS idx_decisions_expires
    ON decisions(expires_at)
    WHERE expires_at IS NOT NULL;

COMMENT ON COLUMN decisions.is_private  IS 'Privacy flag. TRUE = owner family only (default). FALSE = visibility follows share_scope.';
COMMENT ON COLUMN decisions.share_scope IS 'Sharing scope. private|family|friends|public|ai_training. Forced to private when is_private=TRUE.';
COMMENT ON COLUMN decisions.ttl_days    IS 'Optional retention window in days (1-3650). NULL = no expiry. expires_at is computed when set.';
COMMENT ON COLUMN decisions.expires_at  IS 'Auto-purge timestamp. NULL = no expiry. DELETE /api/v1/decisions/expired removes rows past it.';
