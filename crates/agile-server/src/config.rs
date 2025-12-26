use std::env;

#[derive(Clone)]
pub struct ServerConfig {
    pub port: u16,
    pub database_url: String,
    pub jwt_secret: String,
    pub jwt_expiry_hours: i64,
}

impl ServerConfig {
    pub fn from_env() -> Self {
        Self {
            port: env::var("PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(3000),
            database_url: env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgres://localhost/agile".to_string()),
            jwt_secret: env::var("JWT_SECRET")
                .unwrap_or_else(|_| "development-secret-change-in-production".to_string()),
            jwt_expiry_hours: env::var("JWT_EXPIRY_HOURS")
                .ok()
                .and_then(|h| h.parse().ok())
                .unwrap_or(24),
        }
    }
}
