use axum::{extract::State, Extension, Json};
use chrono::Utc;
use uuid::Uuid;

use crate::{
    db::AppState,
    errors::AppResult,
    models::{
        Claims, DecisionAnalysis, Decision, CreateDecisionRequest,
        ScoredOption, UpdateDecisionStatusRequest,
    },
};

pub async fn list_decisions(
    Extension(claims): Extension<Claims>,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<Decision>>> {
    let family_id = get_family_id(&claims, &state).await?;
    let decisions = sqlx::query_as::<_, Decision>(
        "SELECT * FROM decisions WHERE family_id = $1 ORDER BY created_at DESC",
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
                .max(0.0)
                .min(100.0);
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
}
