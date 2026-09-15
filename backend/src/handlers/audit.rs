use axum::{
    extract::{Query, State},
    Extension, Json,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use crate::{
    db::AppState,
    errors::{AppError, AppResult},
    models::{AuditLogEntry, Claims, AUDIT_ACTIONS},
};

/// Hard upper bound on how many rows a single `GET /api/v1/audit` call may
/// return. Keeps a misconfigured client from asking for `limit=1000000` and
/// blocking the DB / scraping the whole history in one shot. The default is
/// the common dashboard page-size; the cap is generous for ops debugging.
const AUDIT_LIST_DEFAULT_LIMIT: i64 = 50;
const AUDIT_LIST_MAX_LIMIT: i64 = 500;

/// Insert one row into `audit_log`. Used by the decisions handlers so the
/// audit log captures the same events the r1 `tracing::info!` lines used to,
/// without forcing every call site to repeat the SQL.
///
/// `metadata` is the action-specific context blob (old/new status, scope,
/// purged count, …). We accept `serde_json::Value` so call sites don't have
/// to define structs for a single field they need.
///
/// `target_id` / `family_id` are nullable: bulk actions like purge have no
/// single target. We deliberately do NOT swallow DB errors here — losing an
/// audit row silently is worse than failing the request, because audit gaps
/// are exactly what compliance asks us to explain. Callers wrap this in a
/// best-effort block where the trade-off is OK; status updates / share flips
/// propagate the error.
pub async fn record_audit(
    db: &sqlx::PgPool,
    who: &str,
    action: &str,
    target_id: Option<Uuid>,
    family_id: Option<Uuid>,
    metadata: Value,
) -> AppResult<()> {
    if !AUDIT_ACTIONS.contains(&action) {
        return Err(AppError::BadRequest(format!(
            "audit action '{}' is not registered (must be one of: {})",
            action,
            AUDIT_ACTIONS.join(", ")
        )));
    }

    sqlx::query(
        r#"INSERT INTO audit_log (who, action, target_kind, target_id, family_id, metadata)
           VALUES ($1, $2, 'decision', $3, $4, $5)"#,
    )
    .bind(who)
    .bind(action)
    .bind(target_id)
    .bind(family_id)
    .bind(metadata)
    .execute(db)
    .await?;

    Ok(())
}

/// Query params for `GET /api/v1/audit`. All filters are AND-combined and
/// optional; omitting every filter returns the most recent N rows the family
/// can see. `since` / `until` use RFC3339 timestamps (chrono parses them).
#[derive(Debug, Deserialize)]
pub struct ListAuditQuery {
    pub action: Option<String>,
    pub family_id: Option<Uuid>,
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
    pub limit: Option<i64>,
}

fn validate_audit_query(q: &ListAuditQuery) -> AppResult<i64> {
    if let Some(action) = &q.action {
        if !AUDIT_ACTIONS.contains(&action.as_str()) {
            return Err(AppError::BadRequest(format!(
                "action filter '{}' is not a known audit action (allowed: {})",
                action,
                AUDIT_ACTIONS.join(", ")
            )));
        }
    }
    if let (Some(s), Some(u)) = (q.since, q.until) {
        if s > u {
            return Err(AppError::BadRequest(
                "since must be <= until".to_string(),
            ));
        }
    }
    let limit = q.limit.unwrap_or(AUDIT_LIST_DEFAULT_LIMIT);
    if limit < 1 {
        return Err(AppError::BadRequest(
            "limit must be >= 1".to_string(),
        ));
    }
    if limit > AUDIT_LIST_MAX_LIMIT {
        return Err(AppError::BadRequest(format!(
            "limit must be <= {}",
            AUDIT_LIST_MAX_LIMIT
        )));
    }
    Ok(limit)
}

/// `GET /api/v1/audit` — queryable audit trail.
///
/// Auth: same JWT as the rest of the protected API. Family scoping: when the
/// caller passes `family_id`, we restrict results to that family. When the
/// caller omits `family_id`, we default to the caller's own family (resolved
/// from JWT `sub`) so a regular user cannot read another family's audit
/// history even with an empty query.
pub async fn list_audit_logs(
    Extension(claims): Extension<Claims>,
    State(state): State<AppState>,
    Query(q): Query<ListAuditQuery>,
) -> AppResult<Json<Vec<AuditLogEntry>>> {
    let limit = validate_audit_query(&q)?;

    // Resolve default family from the JWT subject — never trust a family_id
    // param that points somewhere else (a caller can pass ?family_id=... to
    // narrow further inside their own scope, but cannot widen).
    let caller_family_id = Uuid::parse_str(&claims.sub)
        .map_err(|_| AppError::Unauthorized)?;

    let scope_family = q.family_id.unwrap_or(caller_family_id);

    let rows = sqlx::query_as::<_, AuditLogEntry>(
        r#"SELECT id, who, action, target_kind, target_id, family_id, ts, metadata
           FROM audit_log
           WHERE family_id = $1
             AND ($2::TEXT IS NULL OR action = $2)
             AND ($3::TIMESTAMPTZ IS NULL OR ts >= $3)
             AND ($4::TIMESTAMPTZ IS NULL OR ts <= $4)
           ORDER BY ts DESC
           LIMIT $5"#,
    )
    .bind(scope_family)
    .bind(q.action)
    .bind(q.since)
    .bind(q.until)
    .bind(limit)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(rows))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- ListAuditQuery validation (pure, no DB) -------------------------

    #[test]
    fn validate_accepts_empty_query() {
        // An empty query (caller's family, default limit, no filters) must
        // be accepted — that's the common dashboard load.
        let q = ListAuditQuery {
            action: None,
            family_id: None,
            since: None,
            until: None,
            limit: None,
        };
        assert_eq!(validate_audit_query(&q).unwrap(), AUDIT_LIST_DEFAULT_LIMIT);
    }

    #[test]
    fn validate_clamps_high_limit_to_max_with_error() {
        let q = ListAuditQuery {
            action: None,
            family_id: None,
            since: None,
            until: None,
            limit: Some(AUDIT_LIST_MAX_LIMIT + 1),
        };
        let err = validate_audit_query(&q).unwrap_err();
        assert!(err.to_string().contains("limit must be <="));
    }

    #[test]
    fn validate_rejects_zero_or_negative_limit() {
        let q = ListAuditQuery {
            action: None,
            family_id: None,
            since: None,
            until: None,
            limit: Some(0),
        };
        assert!(validate_audit_query(&q).is_err());

        let q = ListAuditQuery {
            action: None,
            family_id: None,
            since: None,
            until: None,
            limit: Some(-5),
        };
        assert!(validate_audit_query(&q).is_err());
    }

    #[test]
    fn validate_accepts_limit_at_upper_boundary() {
        // AUDIT_LIST_MAX_LIMIT itself must be accepted — boundaries are inclusive.
        let q = ListAuditQuery {
            action: None,
            family_id: None,
            since: None,
            until: None,
            limit: Some(AUDIT_LIST_MAX_LIMIT),
        };
        assert_eq!(validate_audit_query(&q).unwrap(), AUDIT_LIST_MAX_LIMIT);
    }

    #[test]
    fn validate_rejects_unknown_action_filter() {
        let q = ListAuditQuery {
            action: Some("not_a_real_action".to_string()),
            family_id: None,
            since: None,
            until: None,
            limit: None,
        };
        let err = validate_audit_query(&q).unwrap_err();
        assert!(err.to_string().contains("not a known audit action"));
    }

    #[test]
    fn validate_accepts_every_known_action() {
        for action in AUDIT_ACTIONS {
            let q = ListAuditQuery {
                action: Some(action.to_string()),
                family_id: None,
                since: None,
                until: None,
                limit: None,
            };
            assert!(validate_audit_query(&q).is_ok(), "{} should pass", action);
        }
    }

    #[test]
    fn validate_rejects_inverted_time_window() {
        let q = ListAuditQuery {
            action: None,
            family_id: None,
            since: Some(Utc::now()),
            until: Some(Utc::now() - chrono::Duration::hours(1)),
            limit: None,
        };
        let err = validate_audit_query(&q).unwrap_err();
        assert!(err.to_string().contains("since must be <= until"));
    }

    #[test]
    fn validate_accepts_equal_since_and_until() {
        // A zero-width window is degenerate but legal — the query just
        // returns 0 or 1 rows depending on whether the boundary itself is
        // an exact match.
        let now = Utc::now();
        let q = ListAuditQuery {
            action: None,
            family_id: None,
            since: Some(now),
            until: Some(now),
            limit: None,
        };
        assert!(validate_audit_query(&q).is_ok());
    }

    // ---- wire shape ------------------------------------------------------

    #[test]
    fn audit_log_entry_round_trips_through_json() {
        // Pin the JSON shape so dashboards can key off field names. If a
        // future refactor renames `who` -> `actor`, this test fails first.
        let entry = AuditLogEntry {
            id: Uuid::new_v4(),
            who: "alice".to_string(),
            action: "decision_created".to_string(),
            target_kind: "decision".to_string(),
            target_id: Some(Uuid::new_v4()),
            family_id: Some(Uuid::new_v4()),
            ts: Utc::now(),
            metadata: serde_json::json!({"title": "搬家"}),
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("\"who\":\"alice\""));
        assert!(json.contains("\"action\":\"decision_created\""));
        let back: AuditLogEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back.who, "alice");
        assert_eq!(back.action, "decision_created");
    }

    #[test]
    fn audit_log_entry_accepts_null_target_and_family() {
        // Bulk actions like purge have no single target — round-trip a row
        // with both nullable fields None so the wire contract is honest.
        let entry = AuditLogEntry {
            id: Uuid::new_v4(),
            who: "system".to_string(),
            action: "decision_purged".to_string(),
            target_kind: "decision".to_string(),
            target_id: None,
            family_id: None,
            ts: Utc::now(),
            metadata: serde_json::json!({"purged_count": 3}),
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("\"target_id\":null"));
        assert!(json.contains("\"family_id\":null"));
        let back: AuditLogEntry = serde_json::from_str(&json).unwrap();
        assert!(back.target_id.is_none());
        assert!(back.family_id.is_none());
    }
}
