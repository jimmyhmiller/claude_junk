use axum::{
    routing::{get, post},
    Router,
};
use std::net::SocketAddr;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod auth;
mod config;
mod db;
mod error;
mod handlers;
mod middleware;
mod models;

use config::ServerConfig;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "agile_server=debug,tower_http=debug".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Load config
    let config = ServerConfig::from_env();

    // Initialize database
    let pool = db::create_pool(&config.database_url).await?;

    // Run migrations
    db::run_migrations(&pool).await?;

    // Build application state
    let state = handlers::AppState::new(pool, config.clone());

    // Build router
    let app = Router::new()
        // Health check
        .route("/health", get(handlers::health_check))
        // Auth routes
        .route("/api/v1/auth/register", post(handlers::auth::register))
        .route("/api/v1/auth/login", post(handlers::auth::login))
        .route("/api/v1/auth/refresh", post(handlers::auth::refresh))
        .route("/api/v1/auth/me", get(handlers::auth::me))
        // Team routes
        .route("/api/v1/teams", post(handlers::teams::create_team))
        .route("/api/v1/teams", get(handlers::teams::list_teams))
        .route("/api/v1/teams/:team_id", get(handlers::teams::get_team))
        .route("/api/v1/teams/:team_id/members", post(handlers::teams::add_member))
        .route("/api/v1/teams/:team_id/members", get(handlers::teams::list_members))
        // Sync routes
        .route("/api/v1/sync/push", post(handlers::sync::push))
        .route("/api/v1/sync/pull", get(handlers::sync::pull))
        .route("/api/v1/sync/version", get(handlers::sync::version))
        // Middleware
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    tracing::info!("Server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
