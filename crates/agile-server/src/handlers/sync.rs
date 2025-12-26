use crate::error::{AppError, Result};
use crate::handlers::AppState;
use crate::middleware::AuthUser;
use crate::models::{PullResponse, PushRequest, PushResponse, SyncChange, VersionResponse};
use axum::{
    extract::{Query, State},
    Json,
};
use chrono::Utc;
use serde::Deserialize;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct PullQuery {
    pub entity_type: String,
    pub since: Option<u64>,
    pub team_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct VersionQuery {
    pub entity_type: String,
    pub team_id: Option<Uuid>,
}

pub async fn push(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<PushRequest>,
) -> Result<Json<PushResponse>> {
    // Verify team membership if team_id provided
    if let Some(team_id) = req.team_id {
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

    let mut synced = Vec::new();

    for change in req.changes {
        // Get current version for this entity
        let current_version: Option<(i64,)> = sqlx::query_as(
            r#"
            SELECT version FROM sync_data
            WHERE team_id IS NOT DISTINCT FROM $1
            AND entity_type = $2
            AND entity_id = $3
            "#,
        )
        .bind(req.team_id)
        .bind(&change.entity_type)
        .bind(&change.id)
        .fetch_optional(&state.pool)
        .await?;

        let new_version = current_version.map(|v| v.0 + 1).unwrap_or(1);
        let is_delete = change.operation == "delete";

        // Upsert the sync data
        sqlx::query(
            r#"
            INSERT INTO sync_data (team_id, user_id, entity_type, entity_id, data, version, deleted)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (team_id, entity_type, entity_id)
            DO UPDATE SET
                data = $5,
                version = $6,
                deleted = $7,
                updated_at = NOW()
            "#,
        )
        .bind(req.team_id)
        .bind(auth.user_id)
        .bind(&change.entity_type)
        .bind(&change.id)
        .bind(&change.data)
        .bind(new_version)
        .bind(is_delete)
        .execute(&state.pool)
        .await?;

        synced.push(SyncChange {
            id: change.id,
            entity_type: change.entity_type,
            operation: change.operation,
            data: change.data,
            timestamp: Utc::now(),
            version: new_version as u64,
        });
    }

    Ok(Json(PushResponse { synced }))
}

pub async fn pull(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<PullQuery>,
) -> Result<Json<PullResponse>> {
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

    let since_version = query.since.unwrap_or(0) as i64;

    let rows: Vec<crate::models::SyncData> = sqlx::query_as(
        r#"
        SELECT id, team_id, user_id, entity_type, entity_id, data, version, deleted, created_at, updated_at
        FROM sync_data
        WHERE team_id IS NOT DISTINCT FROM $1
        AND entity_type = $2
        AND version > $3
        ORDER BY version ASC
        "#,
    )
    .bind(query.team_id)
    .bind(&query.entity_type)
    .bind(since_version)
    .fetch_all(&state.pool)
    .await?;

    let changes: Vec<SyncChange> = rows
        .into_iter()
        .map(|row| SyncChange {
            id: row.entity_id,
            entity_type: row.entity_type,
            operation: if row.deleted { "delete".to_string() } else { "update".to_string() },
            data: row.data,
            timestamp: row.updated_at,
            version: row.version as u64,
        })
        .collect();

    Ok(Json(PullResponse { changes }))
}

pub async fn version(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<VersionQuery>,
) -> Result<Json<VersionResponse>> {
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

    let max_version: Option<(i64,)> = sqlx::query_as(
        r#"
        SELECT COALESCE(MAX(version), 0)
        FROM sync_data
        WHERE team_id IS NOT DISTINCT FROM $1
        AND entity_type = $2
        "#,
    )
    .bind(query.team_id)
    .bind(&query.entity_type)
    .fetch_optional(&state.pool)
    .await?;

    Ok(Json(VersionResponse {
        version: max_version.map(|v| v.0 as u64).unwrap_or(0),
    }))
}
