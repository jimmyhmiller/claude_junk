use crate::error::{AppError, Result};
use crate::handlers::AppState;
use crate::middleware::AuthUser;
use crate::models::{AddMemberRequest, CreateTeamRequest, Team, TeamMemberInfo, TeamWithMembers};
use axum::{
    extract::{Path, State},
    Json,
};
use uuid::Uuid;

pub async fn create_team(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<CreateTeamRequest>,
) -> Result<Json<Team>> {
    // Create team
    let team: Team = sqlx::query_as(
        r#"
        INSERT INTO teams (name, owner_id)
        VALUES ($1, $2)
        RETURNING id, name, owner_id, created_at, updated_at
        "#,
    )
    .bind(&req.name)
    .bind(auth.user_id)
    .fetch_one(&state.pool)
    .await?;

    // Add owner as member
    sqlx::query(
        "INSERT INTO team_members (team_id, user_id, role) VALUES ($1, $2, 'owner')",
    )
    .bind(team.id)
    .bind(auth.user_id)
    .execute(&state.pool)
    .await?;

    Ok(Json(team))
}

pub async fn list_teams(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Vec<Team>>> {
    let teams: Vec<Team> = sqlx::query_as(
        r#"
        SELECT t.id, t.name, t.owner_id, t.created_at, t.updated_at
        FROM teams t
        JOIN team_members tm ON tm.team_id = t.id
        WHERE tm.user_id = $1
        ORDER BY t.name
        "#,
    )
    .bind(auth.user_id)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(teams))
}

pub async fn get_team(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(team_id): Path<Uuid>,
) -> Result<Json<TeamWithMembers>> {
    // Check membership
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

    // Get team
    let team: Team = sqlx::query_as(
        "SELECT id, name, owner_id, created_at, updated_at FROM teams WHERE id = $1",
    )
    .bind(team_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Team not found".into()))?;

    // Get members
    let members: Vec<TeamMemberInfo> = sqlx::query_as(
        r#"
        SELECT tm.user_id, u.email, u.name, tm.role, tm.joined_at
        FROM team_members tm
        JOIN users u ON u.id = tm.user_id
        WHERE tm.team_id = $1
        ORDER BY tm.joined_at
        "#,
    )
    .bind(team_id)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(TeamWithMembers { team, members }))
}

pub async fn add_member(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(team_id): Path<Uuid>,
    Json(req): Json<AddMemberRequest>,
) -> Result<Json<TeamMemberInfo>> {
    // Check if requester is owner or admin
    let requester_role: Option<(String,)> = sqlx::query_as(
        "SELECT role FROM team_members WHERE team_id = $1 AND user_id = $2",
    )
    .bind(team_id)
    .bind(auth.user_id)
    .fetch_optional(&state.pool)
    .await?;

    match requester_role {
        Some((role,)) if role == "owner" || role == "admin" => {}
        _ => return Err(AppError::Forbidden("Only owners and admins can add members".into())),
    }

    // Find user by email
    let user: crate::models::User = sqlx::query_as(
        "SELECT id, email, name, password_hash, created_at, updated_at FROM users WHERE email = $1",
    )
    .bind(&req.email)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("User not found".into()))?;

    // Check if already a member
    let existing: Option<(i64,)> = sqlx::query_as(
        "SELECT COUNT(*) FROM team_members WHERE team_id = $1 AND user_id = $2",
    )
    .bind(team_id)
    .bind(user.id)
    .fetch_optional(&state.pool)
    .await?;

    if existing.map(|r| r.0 > 0).unwrap_or(false) {
        return Err(AppError::Conflict("User is already a member".into()));
    }

    let role = req.role.unwrap_or_else(|| "member".to_string());

    // Add member and get joined_at
    let row: (chrono::DateTime<chrono::Utc>,) = sqlx::query_as(
        r#"
        INSERT INTO team_members (team_id, user_id, role)
        VALUES ($1, $2, $3)
        RETURNING joined_at
        "#,
    )
    .bind(team_id)
    .bind(user.id)
    .bind(&role)
    .fetch_one(&state.pool)
    .await?;

    let member = TeamMemberInfo {
        user_id: user.id,
        email: user.email,
        name: user.name,
        role,
        joined_at: row.0,
    };

    Ok(Json(member))
}

pub async fn list_members(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(team_id): Path<Uuid>,
) -> Result<Json<Vec<TeamMemberInfo>>> {
    // Check membership
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

    let members: Vec<TeamMemberInfo> = sqlx::query_as(
        r#"
        SELECT tm.user_id, u.email, u.name, tm.role, tm.joined_at
        FROM team_members tm
        JOIN users u ON u.id = tm.user_id
        WHERE tm.team_id = $1
        ORDER BY tm.joined_at
        "#,
    )
    .bind(team_id)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(members))
}
