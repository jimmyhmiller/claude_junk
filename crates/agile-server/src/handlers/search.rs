use crate::error::{AppError, Result};
use crate::handlers::AppState;
use crate::middleware::AuthUser;
use axum::{
    extract::{Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    /// Search query string
    pub q: String,
    /// Entity type to search (optional - searches all if not specified)
    pub entity_type: Option<String>,
    /// Team ID (optional)
    pub team_id: Option<Uuid>,
    /// Maximum results per entity type
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    20
}

#[derive(Debug, Serialize)]
pub struct SearchResult {
    pub entity_type: String,
    pub id: String,
    pub title: String,
    pub snippet: String,
    pub rank: f32,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct SearchResponse {
    pub query: String,
    pub total: usize,
    pub results: Vec<SearchResult>,
}

/// Full-text search across all entity types
pub async fn search(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<SearchQuery>,
) -> Result<Json<SearchResponse>> {
    // Verify team membership if team_id provided
    if let Some(team_id) = query.team_id {
        let is_member: Option<(i64,)> = sqlx::query_as(
            "SELECT COUNT(*) FROM team_members WHERE team_id = $1 AND user_id = $2",
        )
        .bind(team_id)
        .bind(auth.user_id)
        .fetch_optional(&state.pool)
        .await?;

        if !is_member.map(|r| r.0 > 0).unwrap_or(false) {
            return Err(AppError::Forbidden("Not a team member".into()));
        }
    }

    let search_query = query.q.trim();
    if search_query.is_empty() {
        return Ok(Json(SearchResponse {
            query: query.q,
            total: 0,
            results: Vec::new(),
        }));
    }

    // Convert to tsquery format
    let ts_query = search_query
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" & ");

    let mut all_results = Vec::new();

    // Search each entity type
    let entity_types = match &query.entity_type {
        Some(t) => vec![t.as_str()],
        None => vec!["standups", "bugs", "tasks", "retros", "decisions", "notes", "reviews", "kudos"],
    };

    for entity_type in entity_types {
        let results = search_entity_type(
            &state.pool,
            entity_type,
            &ts_query,
            query.team_id,
            query.limit,
        ).await?;
        all_results.extend(results);
    }

    // Sort by rank
    all_results.sort_by(|a, b| b.rank.partial_cmp(&a.rank).unwrap_or(std::cmp::Ordering::Equal));

    let total = all_results.len();

    Ok(Json(SearchResponse {
        query: query.q,
        total,
        results: all_results,
    }))
}

async fn search_entity_type(
    pool: &sqlx::PgPool,
    entity_type: &str,
    ts_query: &str,
    team_id: Option<Uuid>,
    limit: i64,
) -> Result<Vec<SearchResult>> {
    let sql = match entity_type {
        "standups" => r#"
            SELECT
                external_id as id,
                author as title,
                array_to_string(today, '; ') as snippet,
                ts_rank(search_vector, to_tsquery('english', $1)) as rank,
                created_at
            FROM standups
            WHERE search_vector @@ to_tsquery('english', $1)
            AND deleted = false
            AND ($2::uuid IS NULL OR team_id = $2)
            ORDER BY rank DESC
            LIMIT $3
        "#,
        "bugs" => r#"
            SELECT
                external_id as id,
                title,
                COALESCE(description, '') as snippet,
                ts_rank(search_vector, to_tsquery('english', $1)) as rank,
                created_at
            FROM bugs
            WHERE search_vector @@ to_tsquery('english', $1)
            AND deleted = false
            AND ($2::uuid IS NULL OR team_id = $2)
            ORDER BY rank DESC
            LIMIT $3
        "#,
        "tasks" => r#"
            SELECT
                external_id as id,
                title,
                COALESCE(description, '') as snippet,
                ts_rank(search_vector, to_tsquery('english', $1)) as rank,
                created_at
            FROM tasks
            WHERE search_vector @@ to_tsquery('english', $1)
            AND deleted = false
            AND ($2::uuid IS NULL OR team_id = $2)
            ORDER BY rank DESC
            LIMIT $3
        "#,
        "retros" => r#"
            SELECT
                external_id as id,
                category as title,
                content as snippet,
                ts_rank(search_vector, to_tsquery('english', $1)) as rank,
                created_at
            FROM retros
            WHERE search_vector @@ to_tsquery('english', $1)
            AND deleted = false
            AND ($2::uuid IS NULL OR team_id = $2)
            ORDER BY rank DESC
            LIMIT $3
        "#,
        "decisions" => r#"
            SELECT
                external_id as id,
                title,
                decision as snippet,
                ts_rank(search_vector, to_tsquery('english', $1)) as rank,
                created_at
            FROM decisions
            WHERE search_vector @@ to_tsquery('english', $1)
            AND deleted = false
            AND ($2::uuid IS NULL OR team_id = $2)
            ORDER BY rank DESC
            LIMIT $3
        "#,
        "notes" => r#"
            SELECT
                external_id as id,
                title,
                LEFT(content, 200) as snippet,
                ts_rank(search_vector, to_tsquery('english', $1)) as rank,
                created_at
            FROM notes
            WHERE search_vector @@ to_tsquery('english', $1)
            AND deleted = false
            AND ($2::uuid IS NULL OR team_id = $2)
            ORDER BY rank DESC
            LIMIT $3
        "#,
        "reviews" => r#"
            SELECT
                external_id as id,
                title,
                branch || ': ' || COALESCE(description, '') as snippet,
                ts_rank(search_vector, to_tsquery('english', $1)) as rank,
                created_at
            FROM reviews
            WHERE search_vector @@ to_tsquery('english', $1)
            AND deleted = false
            AND ($2::uuid IS NULL OR team_id = $2)
            ORDER BY rank DESC
            LIMIT $3
        "#,
        "kudos" => r#"
            SELECT
                external_id as id,
                to_user as title,
                message as snippet,
                ts_rank(search_vector, to_tsquery('english', $1)) as rank,
                created_at
            FROM kudos
            WHERE search_vector @@ to_tsquery('english', $1)
            AND deleted = false
            AND ($2::uuid IS NULL OR team_id = $2)
            ORDER BY rank DESC
            LIMIT $3
        "#,
        _ => return Ok(Vec::new()),
    };

    let rows: Vec<(String, String, String, f32, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(sql)
        .bind(ts_query)
        .bind(team_id)
        .bind(limit)
        .fetch_all(pool)
        .await?;

    Ok(rows
        .into_iter()
        .map(|(id, title, snippet, rank, created_at)| SearchResult {
            entity_type: entity_type.to_string(),
            id,
            title,
            snippet: if snippet.len() > 200 {
                format!("{}...", &snippet[..200])
            } else {
                snippet
            },
            rank,
            created_at,
        })
        .collect())
}

/// Search bugs with filters
#[derive(Debug, Deserialize)]
pub struct BugSearchQuery {
    pub q: Option<String>,
    pub status: Option<String>,
    pub priority: Option<String>,
    pub assignee: Option<String>,
    pub label: Option<String>,
    pub team_id: Option<Uuid>,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

pub async fn search_bugs(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<BugSearchQuery>,
) -> Result<Json<Vec<serde_json::Value>>> {
    // Verify team membership
    if let Some(team_id) = query.team_id {
        let is_member: Option<(i64,)> = sqlx::query_as(
            "SELECT COUNT(*) FROM team_members WHERE team_id = $1 AND user_id = $2",
        )
        .bind(team_id)
        .bind(auth.user_id)
        .fetch_optional(&state.pool)
        .await?;

        if !is_member.map(|r| r.0 > 0).unwrap_or(false) {
            return Err(AppError::Forbidden("Not a team member".into()));
        }
    }

    let rows: Vec<serde_json::Value> = sqlx::query_scalar(
        r#"
        SELECT jsonb_build_object(
            'id', external_id,
            'title', title,
            'description', description,
            'status', status,
            'priority', priority,
            'reporter', reporter,
            'assignee', assignee,
            'labels', labels,
            'created_at', created_at,
            'updated_at', updated_at
        )
        FROM bugs
        WHERE deleted = false
        AND ($1::uuid IS NULL OR team_id = $1)
        AND ($2::text IS NULL OR status = $2)
        AND ($3::text IS NULL OR priority = $3)
        AND ($4::text IS NULL OR assignee ILIKE '%' || $4 || '%')
        AND ($5::text IS NULL OR $5 = ANY(labels))
        AND ($6::text IS NULL OR search_vector @@ to_tsquery('english', $6))
        ORDER BY
            CASE priority
                WHEN 'critical' THEN 1
                WHEN 'high' THEN 2
                WHEN 'medium' THEN 3
                WHEN 'low' THEN 4
            END,
            created_at DESC
        LIMIT $7
        "#,
    )
    .bind(query.team_id)
    .bind(&query.status)
    .bind(&query.priority)
    .bind(&query.assignee)
    .bind(&query.label)
    .bind(query.q.as_ref().map(|q| q.split_whitespace().collect::<Vec<_>>().join(" & ")))
    .bind(query.limit)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(rows))
}

/// Search tasks with filters
#[derive(Debug, Deserialize)]
pub struct TaskSearchQuery {
    pub q: Option<String>,
    pub status: Option<String>,
    pub assignee: Option<String>,
    pub team_id: Option<Uuid>,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

pub async fn search_tasks(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<TaskSearchQuery>,
) -> Result<Json<Vec<serde_json::Value>>> {
    if let Some(team_id) = query.team_id {
        let is_member: Option<(i64,)> = sqlx::query_as(
            "SELECT COUNT(*) FROM team_members WHERE team_id = $1 AND user_id = $2",
        )
        .bind(team_id)
        .bind(auth.user_id)
        .fetch_optional(&state.pool)
        .await?;

        if !is_member.map(|r| r.0 > 0).unwrap_or(false) {
            return Err(AppError::Forbidden("Not a team member".into()));
        }
    }

    let rows: Vec<serde_json::Value> = sqlx::query_scalar(
        r#"
        SELECT jsonb_build_object(
            'id', external_id,
            'title', title,
            'description', description,
            'status', status,
            'assignee', assignee,
            'labels', labels,
            'created_at', created_at,
            'updated_at', updated_at
        )
        FROM tasks
        WHERE deleted = false
        AND ($1::uuid IS NULL OR team_id = $1)
        AND ($2::text IS NULL OR status = $2)
        AND ($3::text IS NULL OR assignee ILIKE '%' || $3 || '%')
        AND ($4::text IS NULL OR search_vector @@ to_tsquery('english', $4))
        ORDER BY
            CASE status
                WHEN 'in_progress' THEN 1
                WHEN 'todo' THEN 2
                WHEN 'review' THEN 3
                WHEN 'done' THEN 4
            END,
            created_at DESC
        LIMIT $5
        "#,
    )
    .bind(query.team_id)
    .bind(&query.status)
    .bind(&query.assignee)
    .bind(query.q.as_ref().map(|q| q.split_whitespace().collect::<Vec<_>>().join(" & ")))
    .bind(query.limit)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(rows))
}
