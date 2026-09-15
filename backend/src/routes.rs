use axum::{
    middleware,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde_json::json;
use crate::{
    db::AppState,
    handlers::{audit, decisions, happiness, health},
    metrics,
    middleware::auth_middleware,
};

pub fn build_router(state: AppState) -> Router {
    let protected = Router::new()
        .route("/api/v1/happiness/report", get(happiness::get_happiness_report))
        // iter-loop-20260901 r5: trend endpoint for the happiness dashboard.
        .route("/api/v1/happiness/score", get(happiness::get_happiness_score_history))
        .route("/api/v1/decisions", get(decisions::list_decisions).post(decisions::create_decision))
        .route("/api/v1/decisions/expired", delete(decisions::delete_expired_decisions))
        .route("/api/v1/decisions/:id/analyze", get(decisions::analyze_decision))
        .route("/api/v1/decisions/:id/status", put(decisions::update_decision_status))
        // iter-loop-20260901 r2: explicit opt-in to sharing + TTL.
        .route("/api/v1/decisions/:id/share", post(decisions::share_decision))
        // iter-loop-20260901 r5: queryable audit trail (replaces grep-the-logs).
        .route("/api/v1/audit", get(audit::list_audit_logs))
        .route_layer(middleware::from_fn_with_state(state.clone(), auth_middleware));

    Router::new()
        .route("/health", get(health_check))
        // /readyz is intentionally NOT behind auth — k8s / load-balancer
        // probes won't carry a JWT and we want them to fail-open on DB
        // health, not on auth.
        .route("/readyz", get(health::readyz))
        // iter-loop-20260901 r5: Prometheus scrape endpoint, also unauth'd by
        // design (Prometheus servers don't speak JWT). Restrict at the
        // network layer (firewall / k8s NetworkPolicy) in production.
        .route("/metrics", get(serve_metrics))
        .merge(protected)
        .with_state(state)
}

async fn health_check() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok", "service": "jiabu-server", "version": "0.1.0" }))
}

/// `GET /metrics` — Prometheus text format. The text format spec requires a
/// `Content-Type: text/plain; version=0.0.4` header; we set it explicitly so
/// the scrape doesn't fall back to a generic `application/json` sniff.
async fn serve_metrics() -> Result<([(axum::http::HeaderName, &'static str); 1], String), axum::http::StatusCode> {
    let snap = metrics::snapshot();
    let body = metrics::render_prometheus(&snap);
    Ok((
        [(axum::http::header::CONTENT_TYPE, "text/plain; version=0.0.4")],
        body,
    ))
}
