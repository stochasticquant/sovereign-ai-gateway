//! API key authentication middleware.
//!
//! Extracts and validates API keys from request headers.

use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use chrono::Utc;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use crate::state::AppState;

/// Header name for API key.
pub const X_API_KEY: &str = "x-api-key";

/// API key record from database.
#[derive(Debug, Deserialize)]
struct ApiKeyRecord {
    id: Uuid,
    tenant_id: Uuid,
    scopes: Vec<String>,
    is_active: bool,
    expires_at: Option<chrono::DateTime<Utc>>,
}

/// Authentication middleware that extracts and validates API keys.
///
/// Supports two authentication methods:
/// 1. `Authorization: Bearer <key>` header
/// 2. `X-Api-Key: <key>` header
///
/// On successful authentication:
/// - Stores tenant_id in request extensions
/// - Updates last_used timestamp in database
///
/// On failure:
/// - Returns 401 Unauthorized
pub async fn auth_middleware(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Response {
    // Extract API key from headers
    let api_key = extract_api_key(&req);

    let Some(api_key) = api_key else {
        return (
            StatusCode::UNAUTHORIZED,
            "Missing API key. Use 'Authorization: Bearer <key>' or 'X-Api-Key: <key>' header.",
        )
            .into_response();
    };

    // Hash the API key
    let key_hash = hash_api_key(&api_key);

    // Validate API key and get tenant_id
    match validate_api_key(&state.db, &key_hash).await {
        Ok(tenant_id) => {
            // Store tenant_id in request extensions for downstream middleware/handlers
            req.extensions_mut().insert(tenant_id);

            // Update last_used timestamp (fire and forget - don't block request)
            let db = state.db.clone();
            tokio::spawn(async move {
                update_last_used(&db, &key_hash).await.ok();
            });

            next.run(req).await
        }
        Err(AuthError::InvalidKey) => (
            StatusCode::UNAUTHORIZED,
            "Invalid API key",
        )
            .into_response(),
        Err(AuthError::KeyExpired) => (
            StatusCode::UNAUTHORIZED,
            "API key has expired",
        )
            .into_response(),
        Err(AuthError::KeyInactive) => (
            StatusCode::UNAUTHORIZED,
            "API key is inactive",
        )
            .into_response(),
        Err(AuthError::DatabaseError(msg)) => {
            tracing::error!("Database error during auth: {}", msg);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Authentication service unavailable",
            )
                .into_response()
        }
    }
}

/// Extract API key from request headers.
///
/// Checks both `Authorization: Bearer <key>` and `X-Api-Key: <key>` headers.
fn extract_api_key(req: &Request) -> Option<String> {
    // Try Authorization: Bearer <key>
    if let Some(auth_header) = req.headers().get("authorization") {
        if let Ok(auth_str) = auth_header.to_str() {
            if let Some(key) = auth_str.strip_prefix("Bearer ") {
                return Some(key.to_string());
            }
        }
    }

    // Try X-Api-Key header
    if let Some(api_key_header) = req.headers().get(X_API_KEY) {
        if let Ok(key) = api_key_header.to_str() {
            return Some(key.to_string());
        }
    }

    None
}

/// Hash an API key using SHA-256.
fn hash_api_key(key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Validate an API key and return the associated tenant_id.
async fn validate_api_key(db: &PgPool, key_hash: &str) -> Result<Uuid, AuthError> {
    let record = sqlx::query_as!(
        ApiKeyRecord,
        r#"
        SELECT id, tenant_id, scopes, is_active, expires_at
        FROM api_keys
        WHERE key_hash = $1
        "#,
        key_hash,
    )
    .fetch_optional(db)
    .await
    .map_err(|e| AuthError::DatabaseError(e.to_string()))?;

    let Some(record) = record else {
        return Err(AuthError::InvalidKey);
    };

    // Check if key is active
    if !record.is_active {
        return Err(AuthError::KeyInactive);
    }

    // Check if key is expired
    if let Some(expires_at) = record.expires_at {
        if expires_at < Utc::now() {
            return Err(AuthError::KeyExpired);
        }
    }

    Ok(record.tenant_id)
}

/// Update the last_used timestamp for an API key.
async fn update_last_used(db: &PgPool, key_hash: &str) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE api_keys SET last_used = NOW() WHERE key_hash = $1",
        key_hash,
    )
    .execute(db)
    .await?;

    Ok(())
}

/// Authentication errors.
#[derive(Debug)]
enum AuthError {
    InvalidKey,
    KeyExpired,
    KeyInactive,
    DatabaseError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_api_key() {
        let key = "sg-live_test123";
        let hash = hash_api_key(key);

        // SHA-256 hash should be 64 hex characters
        assert_eq!(hash.len(), 64);

        // Same key should produce same hash
        assert_eq!(hash_api_key(key), hash);
    }
}
