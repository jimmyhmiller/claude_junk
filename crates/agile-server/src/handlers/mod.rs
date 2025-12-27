pub mod auth;
pub mod search;
pub mod sync;
pub mod teams;

use crate::config::ServerConfig;
use axum::{extract::State, Json};
use serde_json::{json, Value};
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: ServerConfig,
}

impl AppState {
    pub fn new(pool: PgPool, config: ServerConfig) -> Self {
        Self { pool, config }
    }
}

pub async fn health_check() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}
