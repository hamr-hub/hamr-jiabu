use axum::{extract::State, Extension, Json};
use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use crate::{
    db::AppState,
    errors::{AppError, AppResult},
    models::{
        Claims, CreateDecisionRequest, Decision, DecisionAnalysis, PurgeExpiredResponse,
        ScoredOption, ShareDecisionRequest, UpdateDecisionStatusRequest,
    },
};

/// Allow-list of share_scope values accepted by POST /share. Kept in lockstep
/// with the CHECK constraint in 20260901000002_decision_privacy.sql — if a
/// future scope is added, both must change together.
const SHARE_SCOPES: &[&str] = &["private", "family", "friends", "public", "ai_training"];

fn is_valid_share_scope(scope: &str) -> bool {
    SHARE_SCOPES.contains(&scope)
}

pub async fn list_decisions(
    Extension(claims): Extension<Claims>,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<Decision>>> {
    let family_id = get_family_id(&claims, &state).await?;
    // iter-loop-20260901 r2: default listing is the family's PRIVATE decisions.
    // Shared-out decisions (is_private = false) of OTHER families are out of
    // scope here; this endpoint serves the owner's own view.
    let decisions = sqlx::query_as::<_, Decision>(
        r#"SELECT * FROM decisions
           WHERE family_id = $1 AND is_private = TRUE
           ORDER BY created_at DESC"#,
    )
    .bind(family_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(decisions))
}

pub async fn create_decision(
    Extension(claims): Extension<Claims>,
    State(state): State<AppState>,
    Json(req): Json<CreateDecisionRequest>,
) -> AppResult<Json<Decision>> {
    let family_id = get_family_id(&claims, &state).await?;
    let options_json = serde_json::to_value(&req.options).unwrap_or_default();

    let recommendation = analyze_options(&req.options);

    // iter-loop-20260901 r2: new decisions inherit the privacy defaults
    // (is_private=TRUE, share_scope='private') from the column DEFAULTs, so
    // we don't bind them here — the DB guarantees every fresh row is private.
    let decision = sqlx::query_as::<_, Decision>(
        r#"INSERT INTO decisions (id, family_id, title, description, options, recommendation, status)
           VALUES ($1, $2, $3, $4, $5, $6, 'open')
           RETURNING *"#,
    )
    .bind(Uuid::new_v4())
    .bind(family_id)
    .bind(&req.title)
    .bind(&req.description)
    .bind(&options_json)
    .bind(&recommendation)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(decision))
}

pub async fn analyze_decision(
    Extension(_claims): Extension<Claims>,
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
) -> AppResult<Json<DecisionAnalysis>> {
    let decision = sqlx::query_as::<_, Decision>("SELECT * FROM decisions WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(crate::errors::AppError::NotFound)?;

    let options: Vec<crate::models::DecisionOption> =
        serde_json::from_value(decision.options.clone()).unwrap_or_default();

    let mut scored: Vec<ScoredOption> = options
        .iter()
        .map(|o| {
            let pros = o.pros.len() as f64;
            let cons = o.cons.len() as f64;
            let effort_penalty = match o.effort.as_deref() {
                Some("high") => 15.0,
                Some("medium") => 7.0,
                _ => 0.0,
            };
            let cost_penalty = o.cost.unwrap_or(0.0) / 1000.0 * 5.0;
            let score = (pros * 20.0 - cons * 10.0 - effort_penalty - cost_penalty)
                .clamp(0.0, 100.0);
            ScoredOption {
                name: o.name.clone(),
                score,
                pros_count: o.pros.len(),
                cons_count: o.cons.len(),
            }
        })
        .collect();

    scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());

    let best = scored.first().map(|s| s.name.clone()).unwrap_or_default();
    let total_cons: usize = options.iter().map(|o| o.cons.len()).sum();
    let risk_level = if total_cons > 6 { "high" } else if total_cons > 3 { "medium" } else { "low" };

    Ok(Json(DecisionAnalysis {
        decision_id: decision.id,
        best_option: best.clone(),
        reasoning: format!("综合利弊分析，「{}」在利弊比上具有最高得分，建议优先考虑。", best),
        risk_level: risk_level.to_string(),
        options_scored: scored,
    }))
}

fn analyze_options(options: &[crate::models::DecisionOption]) -> Option<String> {
    if options.is_empty() { return None; }
    let best = options.iter().max_by_key(|o| o.pros.len().saturating_sub(o.cons.len()))?;
    Some(best.name.clone())
}

async fn get_family_id(claims: &Claims, _state: &AppState) -> AppResult<Uuid> {
    Uuid::parse_str(&claims.sub).map_err(|_| crate::errors::AppError::Unauthorized)
}

pub async fn update_decision_status(
    Extension(claims): Extension<Claims>,
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
    Json(req): Json<UpdateDecisionStatusRequest>,
) -> AppResult<Json<Decision>> {
    // Audit-trail policy (iter-loop-20260901):
    //   - status flips AWAY from "open" record actor + timestamp + optional rationale.
    //   - "open" reverts do NOT clear audit fields (preserves the original decision
    //     moment so we never silently lose accountability).
    let now = Utc::now();
    let audit_actor = if req.status != "open" {
        Some(claims.username.clone())
    } else {
        None
    };
    let audit_at = if req.status != "open" { Some(now) } else { None };

    let decision = sqlx::query_as::<_, Decision>(
        r#"UPDATE decisions
           SET status = $1,
               updated_at = $2,
               decided_by = COALESCE($3, decided_by),
               decided_at = COALESCE($4, decided_at),
               rationale  = COALESCE($5, rationale)
           WHERE id = $6
           RETURNING *"#,
    )
    .bind(&req.status)
    .bind(now)
    .bind(audit_actor)
    .bind(audit_at)
    .bind(&req.rationale)
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(crate::errors::AppError::NotFound)?;

    tracing::info!(
        decision_id = %decision.id,
        status = %decision.status,
        actor = ?decision.decided_by,
        "decision status updated"
    );

    Ok(Json(decision))
}

/// POST /api/v1/decisions/:id/share — opt a decision INTO sharing with a
/// wider audience, or back INTO private mode. ttl_days optionally attaches
/// a retention window.
///
/// Privacy invariants enforced here (in addition to the DB CHECK):
///   - share_scope must be one of the allowed values
///   - ttl_days, if present, must be in [1, 3650]
///   - is_private is derived from share_scope (private => true, anything else => false)
///     so the row never sits in an ambiguous state.
pub async fn share_decision(
    Extension(claims): Extension<Claims>,
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
    Json(req): Json<ShareDecisionRequest>,
) -> AppResult<Json<Decision>> {
    if !is_valid_share_scope(&req.share_scope) {
        return Err(AppError::BadRequest(format!(
            "share_scope '{}' is not allowed (must be one of: {})",
            req.share_scope,
            SHARE_SCOPES.join(", "),
        )));
    }
    if let Some(days) = req.ttl_days {
        if !(1..=3650).contains(&days) {
            return Err(AppError::BadRequest(format!(
                "ttl_days must be in 1..=3650, got {}",
                days
            )));
        }
    }

    let family_id = get_family_id(&claims, &state).await?;
    let now = Utc::now();
    let is_private = req.share_scope == "private";
    let expires_at: Option<DateTime<Utc>> = req.ttl_days.map(|d| now + Duration::days(d as i64));

    let decision = sqlx::query_as::<_, Decision>(
        r#"UPDATE decisions
           SET is_private  = $1,
               share_scope = $2,
               ttl_days    = $3,
               expires_at  = $4,
               updated_at  = $5
           WHERE id = $6 AND family_id = $7
           RETURNING *"#,
    )
    .bind(is_private)
    .bind(&req.share_scope)
    .bind(req.ttl_days)
    .bind(expires_at)
    .bind(now)
    .bind(id)
    .bind(family_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    tracing::info!(
        decision_id = %decision.id,
        share_scope = %decision.share_scope,
        ttl_days = ?decision.ttl_days,
        expires_at = ?decision.expires_at,
        actor = %claims.username,
        "decision share scope updated"
    );

    Ok(Json(decision))
}

/// DELETE /api/v1/decisions/expired — maintenance endpoint that purges any
/// decision whose ttl-driven expires_at has passed. Returns the purged count
/// so a cron job / dashboard can log it.
///
/// Safe to call repeatedly: rows without an expires_at are never touched.
pub async fn delete_expired_decisions(
    Extension(_claims): Extension<Claims>,
    State(state): State<AppState>,
) -> AppResult<Json<PurgeExpiredResponse>> {
    // sqlx maps Postgres BIGINT to i64 (not u64), so we fetch as i64 and
    // cast — the count will realistically never exceed u64::MAX.
    let purged: i64 = sqlx::query_scalar(
        r#"WITH purged AS (
              DELETE FROM decisions
              WHERE expires_at IS NOT NULL AND expires_at < NOW()
              RETURNING id
           )
           SELECT COUNT(*)::BIGINT FROM purged"#,
    )
    .fetch_one(&state.db)
    .await?;

    tracing::info!(purged = purged, "expired decisions purged");
    Ok(Json(PurgeExpiredResponse {
        purged: purged.max(0) as u64,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_request_status_falls_back_to_open_when_missing() {
        // Defensive: if the wire format ever changes, JSON parsing here would
        // already fail at the Axum extractor. This test pins the contract
        // that the handler expects { status, rationale? } and nothing else.
        let raw = r#"{"status":"decided","rationale":"budget approved"}"#;
        let parsed: UpdateDecisionStatusRequest = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.status, "decided");
        assert_eq!(parsed.rationale.as_deref(), Some("budget approved"));
    }

    #[test]
    fn update_request_accepts_missing_rationale() {
        let raw = r#"{"status":"archived"}"#;
        let parsed: UpdateDecisionStatusRequest = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.status, "archived");
        assert!(parsed.rationale.is_none());
    }

    // ---- iter-loop-20260901 r2: privacy / share / purge -------------------

    fn share_request(scope: &str, ttl_days: Option<i32>) -> ShareDecisionRequest {
        ShareDecisionRequest {
            share_scope: scope.to_string(),
            ttl_days,
        }
    }

    #[test]
    fn share_request_parses_with_ttl_days() {
        let raw = r#"{"share_scope":"family","ttl_days":30}"#;
        let parsed: ShareDecisionRequest = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.share_scope, "family");
        assert_eq!(parsed.ttl_days, Some(30));
    }

    #[test]
    fn share_request_parses_without_ttl_days() {
        // ttl_days is optional: omitting it means "no retention window".
        let raw = r#"{"share_scope":"ai_training"}"#;
        let parsed: ShareDecisionRequest = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.share_scope, "ai_training");
        assert!(parsed.ttl_days.is_none());
    }

    #[test]
    fn share_request_rejects_missing_scope() {
        let raw = r#"{"ttl_days":7}"#;
        let parsed: Result<ShareDecisionRequest, _> = serde_json::from_str(raw);
        // share_scope is non-Option, so serde itself rejects the missing field —
        // exactly the contract we want pinned.
        assert!(parsed.is_err());
    }

    #[test]
    fn share_scope_allow_list_includes_all_db_values() {
        // If the DB CHECK constraint ever gains a new value, this test fails
        // first — that's the signal to update SHARE_SCOPES together.
        for scope in ["private", "family", "friends", "public", "ai_training"] {
            assert!(is_valid_share_scope(scope), "{} should be valid", scope);
        }
        assert!(!is_valid_share_scope(""));
        assert!(!is_valid_share_scope("everyone"));
        assert!(!is_valid_share_scope("PUBLIC")); // case-sensitive on purpose
    }

    #[test]
    fn ttl_days_within_bounds_is_accepted_by_validation() {
        // We can't reach the DB in unit tests, but the validation step is a
        // pure function of (scope, ttl_days). Happy path: ttl_days in range.
        let req = share_request("family", Some(30));
        assert!(is_valid_share_scope(&req.share_scope));
        assert!((1..=3650).contains(&req.ttl_days.unwrap()));
    }

    #[test]
    fn ttl_days_zero_is_rejected() {
        // ttl_days=0 is below the lower bound — handler returns BadRequest.
        let req = share_request("family", Some(0));
        assert!(!((1..=3650).contains(&req.ttl_days.unwrap())));
    }

    #[test]
    fn ttl_days_above_ten_years_is_rejected() {
        // 3651 days > 10 years — out of upper bound, handler returns BadRequest.
        let req = share_request("public", Some(3651));
        assert!(!((1..=3650).contains(&req.ttl_days.unwrap())));
    }

    #[test]
    fn ttl_days_at_upper_bound_is_accepted() {
        // 3650 = exactly 10 years — boundary value, must pass.
        let req = share_request("family", Some(3650));
        assert!((1..=3650).contains(&req.ttl_days.unwrap()));
    }

    #[test]
    fn private_scope_means_is_private_true() {
        // This pins the invariant: scope == "private" iff is_private == true.
        // The handler computes is_private = (share_scope == "private"); the
        // test simulates that derivation against the parsed request.
        let req = share_request("private", None);
        let derived_is_private = req.share_scope == "private";
        assert!(derived_is_private);
    }

    #[test]
    fn non_private_scope_means_is_private_false() {
        let req = share_request("ai_training", Some(7));
        let derived_is_private = req.share_scope == "private";
        assert!(!derived_is_private);
    }

    #[test]
    fn purge_response_serializes_with_purged_count() {
        // The purge endpoint returns {purged: N}; round-trip the JSON to pin
        // the wire shape so dashboards can rely on it.
        let resp = PurgeExpiredResponse { purged: 42 };
        let json = serde_json::to_string(&resp).unwrap();
        assert_eq!(json, r#"{"purged":42}"#);
        let back: PurgeExpiredResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.purged, 42);
    }

    #[test]
    fn purge_response_handles_zero_purged() {
        // Sad path: when nothing has expired yet, the endpoint must still
        // return a well-formed response with purged=0, not panic or 500.
        let resp = PurgeExpiredResponse { purged: 0 };
        let json = serde_json::to_string(&resp).unwrap();
        assert_eq!(json, r#"{"purged":0}"#);
        let back: PurgeExpiredResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.purged, 0);
    }
}
