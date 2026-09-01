use axum::{
    middleware,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde_json::json;
use crate::{
    db::AppState,
    handlers::{decisions, happiness},
    middleware::auth_middleware,
};

pub fn build_router(state: AppState) -> Router {
    let protected = Router::new()
        .route("/api/v1/happiness/report", get(happiness::get_happiness_report))
        .route("/api/v1/decisions", get(decisions::list_decisions).post(decisions::create_decision))
        .route("/api/v1/decisions/expired", delete(decisions::delete_expired_decisions))
        .route("/api/v1/decisions/:id/analyze", get(decisions::analyze_decision))
        .route("/api/v1/decisions/:id/status", put(decisions::update_decision_status))
        // iter-loop-20260901 r2: explicit opt-in to sharing + TTL.
        .route("/api/v1/decisions/:id/share", post(decisions::share_decision))
        .route_layer(middleware::from_fn_with_state(state.clone(), auth_middleware));

    Router::new()
        .route("/health", get(health_check))
        .merge(protected)
        .with_state(state)
}

async fn health_check() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok", "service": "jiabu-server", "version": "0.1.0" }))
}
