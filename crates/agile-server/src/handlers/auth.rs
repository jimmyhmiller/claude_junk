use crate::auth::{create_access_token, create_refresh_token, hash_password, hash_refresh_token, verify_password};
use crate::error::{AppError, Result};
use crate::handlers::AppState;
use crate::middleware::AuthUser;
use crate::models::{AuthResponse, LoginRequest, RefreshRequest, RegisterRequest, User};
use axum::{extract::State, Json};
use chrono::{Duration, Utc};
use sqlx::Row;

pub async fn register(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> Result<Json<AuthResponse>> {
    // Check if email exists
    let existing: Option<(i64,)> = sqlx::query_as("SELECT COUNT(*) FROM users WHERE email = $1")
        .bind(&req.email)
        .fetch_optional(&state.pool)
        .await?;

    if existing.map(|r| r.0 > 0).unwrap_or(false) {
        return Err(AppError::Conflict("Email already registered".into()));
    }

    // Hash password
    let password_hash = hash_password(&req.password)?;

    // Create user
    let user: User = sqlx::query_as(
        r#"
        INSERT INTO users (email, name, password_hash)
        VALUES ($1, $2, $3)
        RETURNING id, email, name, password_hash, created_at, updated_at
        "#,
    )
    .bind(&req.email)
    .bind(&req.name)
    .bind(&password_hash)
    .fetch_one(&state.pool)
    .await?;

    // Generate tokens
    let (access_token, expires_at) = create_access_token(user.id, &user.email, &state.config)?;
    let refresh_token = create_refresh_token();

    // Store refresh token
    let refresh_expires = Utc::now() + Duration::days(30);
    sqlx::query(
        "INSERT INTO refresh_tokens (user_id, token_hash, expires_at) VALUES ($1, $2, $3)",
    )
    .bind(user.id)
    .bind(hash_refresh_token(&refresh_token))
    .bind(refresh_expires)
    .execute(&state.pool)
    .await?;

    Ok(Json(AuthResponse {
        user,
        access_token,
        refresh_token,
        expires_at,
    }))
}

pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<AuthResponse>> {
    // Find user
    let user: User = sqlx::query_as(
        "SELECT id, email, name, password_hash, created_at, updated_at FROM users WHERE email = $1",
    )
    .bind(&req.email)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::Unauthorized("Invalid credentials".into()))?;

    // Verify password
    if !verify_password(&req.password, &user.password_hash)? {
        return Err(AppError::Unauthorized("Invalid credentials".into()));
    }

    // Generate tokens
    let (access_token, expires_at) = create_access_token(user.id, &user.email, &state.config)?;
    let refresh_token = create_refresh_token();

    // Store refresh token
    let refresh_expires = Utc::now() + Duration::days(30);
    sqlx::query(
        "INSERT INTO refresh_tokens (user_id, token_hash, expires_at) VALUES ($1, $2, $3)",
    )
    .bind(user.id)
    .bind(hash_refresh_token(&refresh_token))
    .bind(refresh_expires)
    .execute(&state.pool)
    .await?;

    Ok(Json(AuthResponse {
        user,
        access_token,
        refresh_token,
        expires_at,
    }))
}

pub async fn refresh(
    State(state): State<AppState>,
    Json(req): Json<RefreshRequest>,
) -> Result<Json<AuthResponse>> {
    let token_hash = hash_refresh_token(&req.refresh_token);

    // Find and validate refresh token
    let row = sqlx::query(
        r#"
        SELECT rt.user_id, rt.expires_at, u.id, u.email, u.name, u.password_hash, u.created_at, u.updated_at
        FROM refresh_tokens rt
        JOIN users u ON u.id = rt.user_id
        WHERE rt.token_hash = $1
        "#,
    )
    .bind(&token_hash)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::Unauthorized("Invalid refresh token".into()))?;

    let expires_at: chrono::DateTime<Utc> = row.get("expires_at");
    if expires_at < Utc::now() {
        return Err(AppError::Unauthorized("Refresh token expired".into()));
    }

    let user = User {
        id: row.get("id"),
        email: row.get("email"),
        name: row.get("name"),
        password_hash: row.get("password_hash"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    };

    // Delete old refresh token
    sqlx::query("DELETE FROM refresh_tokens WHERE token_hash = $1")
        .bind(&token_hash)
        .execute(&state.pool)
        .await?;

    // Generate new tokens
    let (access_token, expires_at) = create_access_token(user.id, &user.email, &state.config)?;
    let new_refresh_token = create_refresh_token();

    // Store new refresh token
    let refresh_expires = Utc::now() + Duration::days(30);
    sqlx::query(
        "INSERT INTO refresh_tokens (user_id, token_hash, expires_at) VALUES ($1, $2, $3)",
    )
    .bind(user.id)
    .bind(hash_refresh_token(&new_refresh_token))
    .bind(refresh_expires)
    .execute(&state.pool)
    .await?;

    Ok(Json(AuthResponse {
        user,
        access_token,
        refresh_token: new_refresh_token,
        expires_at,
    }))
}

pub async fn me(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<User>> {
    let user: User = sqlx::query_as(
        "SELECT id, email, name, password_hash, created_at, updated_at FROM users WHERE id = $1",
    )
    .bind(auth.user_id)
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(user))
}
