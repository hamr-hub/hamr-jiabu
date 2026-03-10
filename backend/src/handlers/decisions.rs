use axum::{extract::State, Extension, Json};
use chrono::Utc;
use uuid::Uuid;

use crate::{
    db::AppState,
    errors::AppResult,
    models::{
        Claims, DecisionAnalysis, Decision, CreateDecisionRequest, ScoredOption,
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
    Extension(_claims): Extension<Claims>,
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
    Json(body): Json<serde_json::Value>,
) -> AppResult<Json<Decision>> {
    let status = body.get("status").and_then(|s| s.as_str()).unwrap_or("open");
    let decision = sqlx::query_as::<_, Decision>(
        "UPDATE decisions SET status = $1, updated_at = $2 WHERE id = $3 RETURNING *",
    )
    .bind(status)
    .bind(Utc::now())
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(crate::errors::AppError::NotFound)?;
    Ok(Json(decision))
}
