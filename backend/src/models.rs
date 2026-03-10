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
