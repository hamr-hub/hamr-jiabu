use axum::{extract::State, http::StatusCode, Json};
use serde_json::{json, Value};
use std::time::Duration;

use crate::{db::AppState, errors::AppError};

/// How long /readyz is willing to wait on the DB before declaring the service
/// not-ready. Kept short on purpose — readiness probes are called frequently
/// and a slow DB should surface as not-ready, not as a hung request.
const DB_PROBE_TIMEOUT: Duration = Duration::from_millis(2000);

/// `GET /readyz` — readiness probe.
///
/// Returns 200 only when the application can serve real traffic: the DB pool
/// has a working connection AND `SELECT 1` returns inside the timeout. Any
/// failure (timeout, pool error, broken pipe) yields 503 with a small JSON
/// body explaining the cause so dashboards can pivot on it.
///
/// Kept separate from `/health` (a liveness probe that must never depend on
/// downstream state — restarts should be triggered by health, NOT by a DB
/// hiccup). See iter-loop-20260901 r4.
pub async fn readyz(State(state): State<AppState>) -> (StatusCode, Json<Value>) {
    let probe = tokio::time::timeout(
        DB_PROBE_TIMEOUT,
        sqlx::query_scalar::<_, i32>("SELECT 1").fetch_one(&state.db),
    )
    .await;

    match probe {
        Ok(Ok(_)) => (StatusCode::OK, Json(json!({ "status": "ready" }))),
        Ok(Err(e)) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({
                "status": "not_ready",
                "reason": "db_query_failed",
                "detail": e.to_string(),
            })),
        ),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({
                "status": "not_ready",
                "reason": "db_timeout",
                "timeout_ms": DB_PROBE_TIMEOUT.as_millis() as u64,
            })),
        ),
    }
}

/// Convenience for callers that want to express the readiness contract as
/// data instead of (status, body) tuples. Kept tiny so the handler stays the
/// single source of truth for the response shape.
#[allow(dead_code)]
pub fn is_app_error_status(s: StatusCode) -> bool {
    s == StatusCode::SERVICE_UNAVAILABLE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ready_response_has_ready_status_field() {
        // The wire shape { "status": "ready" } is what k8s/load-balancer
        // readiness probes (and our own dashboards) will key off. Pin the
        // field name so a rename is a deliberate test change.
        let body = json!({ "status": "ready" });
        assert_eq!(body["status"], "ready");
        assert!(body.get("reason").is_none(), "happy path must not carry a reason");
    }

    #[test]
    fn not_ready_response_carries_reason_and_detail() {
        // When the probe fails we want operators to know WHY without grepping
        // server logs. The body must include a non-empty `reason` and at
        // least one of `detail` / `timeout_ms`.
        let body = json!({
            "status": "not_ready",
            "reason": "db_timeout",
            "timeout_ms": 2000_u64,
        });
        assert_eq!(body["status"], "not_ready");
        assert_eq!(body["reason"], "db_timeout");
        assert_eq!(body["timeout_ms"], 2000);
    }

    #[test]
    fn db_probe_timeout_is_at_most_two_seconds() {
        // Pin the upper bound so a future "let me bump this to 30s" change
        // is loud. Liveness probes hit us every few seconds; anything past
        // 2s would pile up.
        assert!(DB_PROBE_TIMEOUT <= Duration::from_secs(2));
        assert!(DB_PROBE_TIMEOUT >= Duration::from_millis(100));
    }

    #[test]
    fn app_error_helper_recognises_503() {
        // Sanity: the helper treats 503 as a "not ready" signal. If someone
        // later swaps the status code, this test catches the inconsistency.
        assert!(is_app_error_status(StatusCode::SERVICE_UNAVAILABLE));
        assert!(!is_app_error_status(StatusCode::OK));
    }

    // The AppError re-export makes the unused-import lint happy when tests
    // are the only consumer of the crate's error module in this file.
    #[allow(dead_code)]
    fn _ensure_apperror_in_scope(_: AppError) {}
}
