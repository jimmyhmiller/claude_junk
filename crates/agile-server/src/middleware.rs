use crate::auth::validate_token;
use crate::config::ServerConfig;
use crate::error::AppError;
use axum::{
    extract::FromRequestParts,
    http::{header, request::Parts},
};
use uuid::Uuid;

pub struct AuthUser {
    pub user_id: Uuid,
    pub email: String,
}

impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = AppError;

    fn from_request_parts<'life0, 'life1, 'async_trait>(
        parts: &'life0 mut Parts,
        _state: &'life1 S,
    ) -> core::pin::Pin<
        Box<dyn core::future::Future<Output = Result<Self, Self::Rejection>> + Send + 'async_trait>,
    >
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            // Get authorization header
            let auth_header = parts
                .headers
                .get(header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok())
                .ok_or_else(|| AppError::Unauthorized("Missing authorization header".into()))?;

            // Extract bearer token
            let token = auth_header
                .strip_prefix("Bearer ")
                .ok_or_else(|| AppError::Unauthorized("Invalid authorization header format".into()))?;

            // Get config from extensions or fallback to env
            let config = parts
                .extensions
                .get::<ServerConfig>()
                .cloned()
                .unwrap_or_else(ServerConfig::from_env);

            // Validate token
            let claims = validate_token(token, &config)?;

            let user_id = Uuid::parse_str(&claims.sub)
                .map_err(|_| AppError::Unauthorized("Invalid user ID in token".into()))?;

            Ok(AuthUser {
                user_id,
                email: claims.email,
            })
        })
    }
}
