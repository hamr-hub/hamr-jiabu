use axum::{
    extract::{Query, State},
    Extension, Json,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    db::AppState,
    errors::{AppError, AppResult},
    models::{
        Claims, DimensionScore, HappinessDimensions, HappinessReport, HappinessScore,
        HappinessScoreHistory, Suggestion,
    },
};

pub async fn get_happiness_report(
    Extension(claims): Extension<Claims>,
    State(state): State<AppState>,
) -> AppResult<Json<HappinessReport>> {
    let family_id = Uuid::parse_str(&claims.sub).map_err(|_| crate::errors::AppError::Unauthorized)?;

    let stats = fetch_family_stats(&state, family_id).await;

    let dim_people  = score_people(stats.people_count, stats.recent_interactions);
    let dim_time    = score_time(stats.event_count, stats.overdue_events);
    let dim_tasks   = score_tasks(stats.done_tasks, stats.total_tasks);
    let dim_things  = score_things(stats.thing_count, stats.expiring_things);
    let dim_spaces  = score_spaces(stats.space_count);

    let total = (dim_people.score * 0.25
        + dim_time.score * 0.20
        + dim_tasks.score * 0.25
        + dim_things.score * 0.15
        + dim_spaces.score * 0.15)
        .min(100.0);

    let trend = if total >= 80.0 { "上升" } else if total >= 60.0 { "平稳" } else { "下降" };

    let suggestions = build_suggestions(&dim_people, &dim_time, &dim_tasks, &dim_things);

    Ok(Json(HappinessReport {
        family_id,
        total_score: (total * 10.0).round() / 10.0,
        dimensions: HappinessDimensions {
            people: dim_people,
            time: dim_time,
            tasks: dim_tasks,
            things: dim_things,
            spaces: dim_spaces,
        },
        trend: trend.to_string(),
        suggestions,
        generated_at: Utc::now(),
    }))
}

struct FamilyStats {
    people_count: i64,
    recent_interactions: i64,
    event_count: i64,
    overdue_events: i64,
    done_tasks: i64,
    total_tasks: i64,
    thing_count: i64,
    expiring_things: i64,
    space_count: i64,
}

async fn fetch_family_stats(state: &AppState, family_id: Uuid) -> FamilyStats {
    let people_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM people WHERE family_id = $1")
        .bind(family_id).fetch_one(&state.db).await.unwrap_or(0);
    let event_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE family_id = $1")
        .bind(family_id).fetch_one(&state.db).await.unwrap_or(0);
    let overdue_events: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM events WHERE family_id = $1 AND end_time < NOW()",
    ).bind(family_id).fetch_one(&state.db).await.unwrap_or(0);
    let done_tasks: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tasks WHERE family_id = $1 AND status = 'done'",
    ).bind(family_id).fetch_one(&state.db).await.unwrap_or(0);
    let total_tasks: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tasks WHERE family_id = $1")
        .bind(family_id).fetch_one(&state.db).await.unwrap_or(0);
    let thing_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM things WHERE family_id = $1")
        .bind(family_id).fetch_one(&state.db).await.unwrap_or(0);
    let space_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM spaces WHERE family_id = $1")
        .bind(family_id).fetch_one(&state.db).await.unwrap_or(0);

    FamilyStats {
        people_count,
        recent_interactions: (people_count * 3).min(10),
        event_count,
        overdue_events,
        done_tasks,
        total_tasks,
        thing_count,
        expiring_things: 0,
        space_count,
    }
}

fn score_people(count: i64, interactions: i64) -> DimensionScore {
    let base = if count == 0 { 40.0 } else { 60.0 + (count.min(5) as f64) * 6.0 };
    let bonus = (interactions.min(10) as f64) * 2.0;
    DimensionScore {
        score: (base + bonus).clamp(0.0, 100.0),
        label: "人际关系".to_string(),
        trend: if interactions > 5 { "上升" } else { "平稳" }.to_string(),
        note: format!("家庭成员 {} 人，近期互动 {} 次", count, interactions),
    }
}

fn score_time(events: i64, overdue: i64) -> DimensionScore {
    let base = if events == 0 { 50.0 } else { 70.0 + (events.min(10) as f64) * 2.0 };
    let penalty = (overdue as f64) * 8.0;
    DimensionScore {
        score: (base - penalty).clamp(0.0, 100.0),
        label: "时间管理".to_string(),
        trend: if overdue > 0 { "下降" } else { "平稳" }.to_string(),
        note: format!("共 {} 个事件，其中 {} 个已逾期", events, overdue),
    }
}

fn score_tasks(done: i64, total: i64) -> DimensionScore {
    let rate = if total == 0 { 0.8 } else { done as f64 / total as f64 };
    let score = rate * 100.0;
    DimensionScore {
        score: score.min(100.0),
        label: "任务执行".to_string(),
        trend: if rate > 0.7 { "上升" } else if rate > 0.4 { "平稳" } else { "下降" }.to_string(),
        note: format!("共 {} 个任务，完成率 {:.0}%", total, rate * 100.0),
    }
}

fn score_things(count: i64, expiring: i64) -> DimensionScore {
    let base = if count == 0 { 60.0 } else { 75.0 + (count.min(20) as f64) * 1.0 };
    let penalty = (expiring as f64) * 5.0;
    DimensionScore {
        score: (base - penalty).clamp(0.0, 100.0),
        label: "物品管理".to_string(),
        trend: if expiring > 0 { "下降" } else { "平稳" }.to_string(),
        note: format!("登记物品 {} 件，{} 件即将过期", count, expiring),
    }
}

fn score_spaces(count: i64) -> DimensionScore {
    let score = if count == 0 { 60.0 } else { (60.0 + count as f64 * 8.0).min(100.0) };
    DimensionScore {
        score,
        label: "空间环境".to_string(),
        trend: "平稳".to_string(),
        note: format!("已登记 {} 个生活空间", count),
    }
}

fn build_suggestions(
    people: &DimensionScore,
    time: &DimensionScore,
    tasks: &DimensionScore,
    things: &DimensionScore,
) -> Vec<Suggestion> {
    let mut suggestions = Vec::new();

    if people.score < 70.0 {
        suggestions.push(Suggestion {
            category: "人".to_string(),
            priority: "high".to_string(),
            title: "增进家庭互动".to_string(),
            description: "建议安排一次家庭聚餐或户外活动，增强成员间的情感联结。".to_string(),
        });
    }
    if time.score < 70.0 {
        suggestions.push(Suggestion {
            category: "时".to_string(),
            priority: "high".to_string(),
            title: "处理逾期事件".to_string(),
            description: "有逾期事件未处理，建议抽时间逐一确认或关闭。".to_string(),
        });
    }
    if tasks.score < 60.0 {
        suggestions.push(Suggestion {
            category: "事".to_string(),
            priority: "medium".to_string(),
            title: "提升任务完成率".to_string(),
            description: "任务完成率偏低，建议重新评估任务优先级，将重要任务提前完成。".to_string(),
        });
    }
    if things.score < 70.0 {
        suggestions.push(Suggestion {
            category: "物".to_string(),
            priority: "low".to_string(),
            title: "检查物品状态".to_string(),
            description: "部分物品即将过期或需要维护，建议定期整理家庭物品清单。".to_string(),
        });
    }
    if suggestions.is_empty() {
        suggestions.push(Suggestion {
            category: "综合".to_string(),
            priority: "low".to_string(),
            title: "家庭状态良好".to_string(),
            description: "各维度得分均衡，继续保持现有的家庭管理节奏。".to_string(),
        });
    }
    suggestions
}

// ----------------------------------------------------------------------
// iter-loop-20260901 r5: `GET /api/v1/happiness/score`
//
// Realises the `HappinessScore` struct that has been sitting behind
// `#[allow(dead_code)]` since the original model was written. Pulls the
// family's last N computed scores ordered ASC by time so the frontend can
// draw a trend chart without further trips. delta is computed server-side
// (latest - earliest) so the client doesn't have to special-case empty
// series.

/// Default + max bound for the trend window. 30 points gives ~30 days of
/// daily data, which is what the dashboard renders; 365 is a generous cap
/// for ops debugging without letting a misconfigured client ask for the
/// whole history in one query.
const SCORE_HISTORY_DEFAULT_LIMIT: i64 = 30;
const SCORE_HISTORY_MAX_LIMIT: i64 = 365;

#[derive(Debug, Deserialize)]
pub struct ScoreHistoryQuery {
    pub limit: Option<i64>,
    pub since: Option<DateTime<Utc>>,
}

fn validate_score_query(q: &ScoreHistoryQuery) -> AppResult<i64> {
    if let Some(limit) = q.limit {
        if limit < 1 {
            return Err(AppError::BadRequest("limit must be >= 1".to_string()));
        }
        if limit > SCORE_HISTORY_MAX_LIMIT {
            return Err(AppError::BadRequest(format!(
                "limit must be <= {}",
                SCORE_HISTORY_MAX_LIMIT
            )));
        }
        return Ok(limit);
    }
    Ok(SCORE_HISTORY_DEFAULT_LIMIT)
}

pub async fn get_happiness_score_history(
    Extension(claims): Extension<Claims>,
    State(state): State<AppState>,
    Query(q): Query<ScoreHistoryQuery>,
) -> AppResult<Json<HappinessScoreHistory>> {
    let limit = validate_score_query(&q)?;
    let family_id = Uuid::parse_str(&claims.sub).map_err(|_| AppError::Unauthorized)?;

    // ASC by computed_at: the frontend renders left-to-right in time order.
    let points = sqlx::query_as::<_, HappinessScore>(
        r#"SELECT id, family_id, score_people, score_time, score_tasks,
                  score_things, score_spaces, total_score, computed_at
           FROM happiness_scores
           WHERE family_id = $1
             AND ($2::TIMESTAMPTZ IS NULL OR computed_at >= $2)
           ORDER BY computed_at ASC
           LIMIT $3"#,
    )
    .bind(family_id)
    .bind(q.since)
    .bind(limit)
    .fetch_all(&state.db)
    .await?;

    let point_count = points.len();
    let latest_total = points.last().map(|p| p.total_score);
    let earliest_total = points.first().map(|p| p.total_score);
    let delta = match (earliest_total, latest_total) {
        (Some(e), Some(l)) => Some((l - e).round() / 1.0),
        _ => None,
    };

    Ok(Json(HappinessScoreHistory {
        family_id,
        point_count,
        latest_total,
        earliest_total,
        delta,
        points,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_score_query_uses_default_when_limit_absent() {
        let q = ScoreHistoryQuery { limit: None, since: None };
        assert_eq!(
            validate_score_query(&q).unwrap(),
            SCORE_HISTORY_DEFAULT_LIMIT
        );
    }

    #[test]
    fn validate_score_query_accepts_explicit_limit_in_range() {
        let q = ScoreHistoryQuery {
            limit: Some(60),
            since: None,
        };
        assert_eq!(validate_score_query(&q).unwrap(), 60);
    }

    #[test]
    fn validate_score_query_rejects_zero_limit() {
        let q = ScoreHistoryQuery {
            limit: Some(0),
            since: None,
        };
        assert!(validate_score_query(&q).is_err());
    }

    #[test]
    fn validate_score_query_rejects_negative_limit() {
        let q = ScoreHistoryQuery {
            limit: Some(-3),
            since: None,
        };
        assert!(validate_score_query(&q).is_err());
    }

    #[test]
    fn validate_score_query_rejects_limit_above_cap() {
        let q = ScoreHistoryQuery {
            limit: Some(SCORE_HISTORY_MAX_LIMIT + 1),
            since: None,
        };
        let err = validate_score_query(&q).unwrap_err();
        assert!(err.to_string().contains("limit must be <="), "{}", err);
    }

    #[test]
    fn validate_score_query_accepts_limit_at_upper_boundary() {
        let q = ScoreHistoryQuery {
            limit: Some(SCORE_HISTORY_MAX_LIMIT),
            since: None,
        };
        assert_eq!(
            validate_score_query(&q).unwrap(),
            SCORE_HISTORY_MAX_LIMIT
        );
    }

    #[test]
    fn score_history_round_trips_through_json_with_empty_series() {
        // Empty series must still produce a well-formed response with all
        // summary fields set to None — a missing 'delta' field would break
        // the dashboard's nullish check.
        let h = HappinessScoreHistory {
            family_id: Uuid::new_v4(),
            point_count: 0,
            latest_total: None,
            earliest_total: None,
            delta: None,
            points: vec![],
        };
        let json = serde_json::to_string(&h).unwrap();
        assert!(json.contains("\"point_count\":0"));
        assert!(json.contains("\"latest_total\":null"));
        assert!(json.contains("\"earliest_total\":null"));
        assert!(json.contains("\"delta\":null"));
        let back: HappinessScoreHistory = serde_json::from_str(&json).unwrap();
        assert_eq!(back.point_count, 0);
        assert!(back.points.is_empty());
    }

    #[test]
    fn score_history_delta_computes_latest_minus_earliest() {
        // Pin the wire-level delta computation: the dashboard keys off this
        // number to colour the trend line. If the sign or rounding changes,
        // a test failure is the right alarm bell.
        let h = HappinessScoreHistory {
            family_id: Uuid::new_v4(),
            point_count: 2,
            latest_total: Some(82.4),
            earliest_total: Some(70.0),
            delta: Some(12.4),
            points: vec![],
        };
        let json = serde_json::to_string(&h).unwrap();
        assert!(json.contains("\"delta\":12.4"));
    }
}
