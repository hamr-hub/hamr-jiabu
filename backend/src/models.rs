use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String,
    pub email: String,
    pub username: String,
    pub exp: i64,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
#[allow(dead_code)] // Some dimension fields are reserved for future report shape.
pub struct HappinessScore {
    pub id: Uuid,
    pub family_id: Uuid,
    pub score_people: f64,
    pub score_time: f64,
    pub score_tasks: f64,
    pub score_things: f64,
    pub score_spaces: f64,
    pub total_score: f64,
    pub computed_at: DateTime<Utc>,
}

/// `GET /api/v1/happiness/score` response. Wraps a historical point so the
/// client can render a trend chart without further trips.
///
/// `points` is ordered ASCENDING by `computed_at` — the frontend draws
/// left-to-right and that matches the natural time direction. `delta` is
/// pre-computed server-side (last - first) so the client doesn't have to
/// worry about empty series.
#[derive(Debug, Serialize, Deserialize)]
pub struct HappinessScoreHistory {
    pub family_id: Uuid,
    pub point_count: usize,
    pub latest_total: Option<f64>,
    pub earliest_total: Option<f64>,
    pub delta: Option<f64>,
    pub points: Vec<HappinessScore>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HappinessReport {
    pub family_id: Uuid,
    pub total_score: f64,
    pub dimensions: HappinessDimensions,
    pub trend: String,
    pub suggestions: Vec<Suggestion>,
    pub generated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HappinessDimensions {
    pub people: DimensionScore,
    pub time: DimensionScore,
    pub tasks: DimensionScore,
    pub things: DimensionScore,
    pub spaces: DimensionScore,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DimensionScore {
    pub score: f64,
    pub label: String,
    pub trend: String,
    pub note: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Suggestion {
    pub category: String,
    pub priority: String,
    pub title: String,
    pub description: String,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Decision {
    pub id: Uuid,
    pub family_id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub options: serde_json::Value,
    pub recommendation: Option<String>,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    // Audit trail (iter-loop-20260901 r1) — populated when status flips
    // away from "open". Nullable so existing rows remain valid post-migration.
    pub decided_by: Option<String>,
    pub decided_at: Option<DateTime<Utc>>,
    pub rationale: Option<String>,
    // Privacy defaults (iter-loop-20260901 r2) — every row is private by
    // default. share_scope only matters when is_private = false; ttl_days /
    // expires_at are optional retention controls.
    pub is_private: bool,
    pub share_scope: String,
    pub ttl_days: Option<i32>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// `audit_log` row (iter-loop-20260901 r5). Wire shape returned by
/// `GET /api/v1/audit`; mirrors the table 1:1 so the migration's CHECK
/// constraints stay the single source of truth for what an action can be.
#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct AuditLogEntry {
    pub id: Uuid,
    pub who: String,
    pub action: String,
    pub target_kind: String,
    pub target_id: Option<Uuid>,
    pub family_id: Option<Uuid>,
    pub ts: DateTime<Utc>,
    pub metadata: serde_json::Value,
}

/// Allowed `action` values for `audit_log` — must stay in lockstep with the
/// CHECK constraint in `20260901000003_audit_log.sql`. Re-exported through a
/// constant rather than an enum so the value is the same `&str` we send to
/// the DB (no `as_str()` round-trip at every insert site).
pub const AUDIT_ACTIONS: &[&str] = &[
    "decision_created",
    "decision_status_updated",
    "decision_shared",
    "decision_purged",
];

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateDecisionStatusRequest {
    pub status: String,
    pub rationale: Option<String>,
}

/// Body for POST /api/v1/decisions/:id/share — flip a decision's privacy state.
///
/// `ttl_days` is optional and bounded to [1, 3650] (10 years). The handler
/// rejects values outside the range rather than silently clamping so a typo
/// in the client surfaces immediately.
#[derive(Debug, Serialize, Deserialize)]
pub struct ShareDecisionRequest {
    pub share_scope: String,
    pub ttl_days: Option<i32>,
}

/// Response shape for DELETE /api/v1/decisions/expired — clients want a count
/// of purged rows so dashboards / scheduled jobs can log it.
#[derive(Debug, Serialize, Deserialize)]
pub struct PurgeExpiredResponse {
    pub purged: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateDecisionRequest {
    pub title: String,
    pub description: Option<String>,
    pub options: Vec<DecisionOption>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DecisionOption {
    pub name: String,
    pub pros: Vec<String>,
    pub cons: Vec<String>,
    pub cost: Option<f64>,
    pub effort: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DecisionAnalysis {
    pub decision_id: Uuid,
    pub best_option: String,
    pub reasoning: String,
    pub risk_level: String,
    pub options_scored: Vec<ScoredOption>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ScoredOption {
    pub name: String,
    pub score: f64,
    pub pros_count: usize,
    pub cons_count: usize,
}
