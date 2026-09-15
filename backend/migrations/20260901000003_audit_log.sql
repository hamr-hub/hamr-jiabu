-- iter-loop-20260901 r5: Persistent audit log table.
--
-- The round-1 audit fields on `decisions` (decided_by/decided_at/rationale) only
-- capture status flips. Round-5 graduates audit-worthy events out of
-- `tracing::info!` and into a queryable SQL table so the security/compliance
-- story no longer depends on grepping log files.
--
-- Scope is deliberately narrow: we log DECISION-shaped actions only. General
-- request-level activity stays in tracing; that's `metrics-exporter-prometheus`
-- territory (also r5), not audit-log territory. The two streams stay separate
-- on purpose — metrics lose fidelity fast, audit logs must not.
--
-- Design choices:
--   * `action` is a small fixed enum (text + CHECK) rather than a free string
--     so dashboards / SIEM rules can pivot on it without parsing semantics.
--   * `target_id` is nullable: a purge-style action has no single target. We
--     keep `target_kind` so logs can mix decisions (uuid) with future
--     happiness/family targets without a column migration.
--   * `metadata` is jsonb so each action type can stash its own context
--     (old_status, new_status, share_scope, purged_count) without ALTER TABLE.
--   * `family_id` is nullable because cross-family admin actions (none today,
--     but `purge` already iterates across families) might want to log without
--     scoping. NULL is the explicit "no scope" marker.
--   * No FK to decisions(id): ON DELETE SET NULL would lose evidence of the
--     target row, and audits must outlive the data they describe.

CREATE TABLE IF NOT EXISTS audit_log (
    id           UUID         PRIMARY KEY DEFAULT uuid_generate_v4(),
    who          VARCHAR(100) NOT NULL,
    action       VARCHAR(32)  NOT NULL
        CHECK (action IN (
            'decision_created',
            'decision_status_updated',
            'decision_shared',
            'decision_purged'
        )),
    target_kind  VARCHAR(32)  NOT NULL DEFAULT 'decision'
        CHECK (target_kind IN ('decision')),
    target_id    UUID,
    family_id    UUID,
    ts           TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    metadata     JSONB        NOT NULL DEFAULT '{}'::jsonb
);

-- Time-series reads dominate: "what happened in the last 24h?" / "what did
-- this family do last week?" / "show me everything by actor X".
-- Descending ts matches the dashboard's natural ordering and avoids a sort.
CREATE INDEX IF NOT EXISTS idx_audit_log_ts
    ON audit_log(ts DESC);

CREATE INDEX IF NOT EXISTS idx_audit_log_family_ts
    ON audit_log(family_id, ts DESC)
    WHERE family_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_audit_log_actor_ts
    ON audit_log(who, ts DESC);

CREATE INDEX IF NOT EXISTS idx_audit_log_action_ts
    ON audit_log(action, ts DESC);

-- target_id lookups ("history of decision X") are the second-most-common
-- pattern. Partial because most actions reference a decision row.
CREATE INDEX IF NOT EXISTS idx_audit_log_target
    ON audit_log(target_id, ts DESC)
    WHERE target_id IS NOT NULL;

COMMENT ON TABLE  audit_log                IS 'iter-loop-20260901 r5: append-only audit trail for DECISION-shaped actions; replaces ad-hoc tracing::info lines';
COMMENT ON COLUMN audit_log.who           IS 'Username from JWT claims (actor). Never NULL — system actions use actor=system';
COMMENT ON COLUMN audit_log.action        IS 'Fixed enum: decision_created|decision_status_updated|decision_shared|decision_purged';
COMMENT ON COLUMN audit_log.target_kind   IS 'Resource the action targeted. Currently only decision';
COMMENT ON COLUMN audit_log.target_id     IS 'UUID of the target resource when applicable; NULL for bulk actions (purge)';
COMMENT ON COLUMN audit_log.family_id     IS 'Owning family if known; NULL for cross-family admin actions';
COMMENT ON COLUMN audit_log.ts            IS 'Wall-clock time the action was recorded (server-side, NOW())';
COMMENT ON COLUMN audit_log.metadata      IS 'Action-specific context (old/new status, share_scope, purged_count, etc.) as JSONB';
