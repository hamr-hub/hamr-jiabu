use axum::{extract::State, Extension, Json};
use chrono::Utc;
use uuid::Uuid;

use crate::{
    db::AppState,
    errors::AppResult,
    models::{
        Claims, DimensionScore, HappinessDimensions, HappinessReport, Suggestion,
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
        score: (base + bonus).min(100.0),
        label: "人际关系".to_string(),
        trend: if interactions > 5 { "上升" } else { "平稳" }.to_string(),
        note: format!("家庭成员 {} 人，近期互动 {} 次", count, interactions),
    }
}

fn score_time(events: i64, overdue: i64) -> DimensionScore {
    let base = if events == 0 { 50.0 } else { 70.0 + (events.min(10) as f64) * 2.0 };
    let penalty = (overdue as f64) * 8.0;
    DimensionScore {
        score: (base - penalty).max(0.0).min(100.0),
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
        score: (base - penalty).max(0.0).min(100.0),
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
