//! Integration test for the complete auth → quota → handler pipeline.

use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

/// Helper to create a test database pool.
async fn create_test_db_pool() -> PgPool {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://localhost/sovereign_gateway_test".to_string());

    PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to test database")
}

/// Helper to create a test tenant.
async fn create_test_tenant(db: &PgPool, name: &str) -> Uuid {
    let tenant_id = Uuid::now_v7();

    sqlx::query!(
        "INSERT INTO tenants (id, name, is_active) VALUES ($1, $2, true)",
        tenant_id,
        name,
    )
    .execute(db)
    .await
    .expect("Failed to create test tenant");

    tenant_id
}

/// Helper to create a test API key and return the raw key.
async fn create_test_api_key(db: &PgPool, tenant_id: Uuid, name: &str) -> String {
    let key_id = Uuid::now_v7();
    let raw_key = format!("sk-test_{}", Uuid::now_v7());

    // Hash the key
    let mut hasher = Sha256::new();
    hasher.update(raw_key.as_bytes());
    let key_hash = format!("{:x}", hasher.finalize());

    // Get prefix (first 8 chars)
    let key_prefix = raw_key.chars().take(8).collect::<String>();

    sqlx::query!(
        r#"
        INSERT INTO api_keys (id, tenant_id, key_hash, key_prefix, name, scopes, is_active)
        VALUES ($1, $2, $3, $4, $5, $6, true)
        "#,
        key_id,
        tenant_id,
        key_hash,
        key_prefix,
        name,
        &["chat"] as &[&str],
    )
    .execute(db)
    .await
    .expect("Failed to create API key");

    raw_key
}

/// Helper to set quota for a tenant.
async fn set_tenant_quota(
    db: &PgPool,
    tenant_id: Uuid,
    daily_limit: Option<i64>,
    max_per_request: Option<i32>,
) {
    use token_governor::{QuotaEnforcer, TenantQuota};

    let enforcer = QuotaEnforcer::new(db.clone());
    let quota = TenantQuota {
        tenant_id,
        daily_token_limit: daily_limit,
        max_tokens_per_request: max_per_request,
        max_requests_per_minute: None,
        max_concurrent_requests: None,
    };

    enforcer.set_quota(quota).await.expect("Failed to set quota");
}

/// Helper to cleanup test tenant.
async fn cleanup_test_tenant(db: &PgPool, tenant_id: Uuid) {
    sqlx::query!("DELETE FROM tenants WHERE id = $1", tenant_id)
        .execute(db)
        .await
        .ok();
}

#[tokio::test]
#[ignore] // Run with: cargo test --package gateway-tests -- --ignored
async fn test_auth_quota_pipeline_success() {
    let db = create_test_db_pool().await;

    // Create tenant and API key
    let tenant_id = create_test_tenant(&db, "test-pipeline-success").await;
    let api_key = create_test_api_key(&db, tenant_id, "test-key").await;

    // Set generous quota
    set_tenant_quota(&db, tenant_id, Some(100_000), Some(4000)).await;

    // TODO: Make actual HTTP request to gateway server
    // For now, we verify the database setup
    let quota = token_governor::QuotaEnforcer::new(db.clone())
        .get_quota(tenant_id)
        .await
        .expect("Failed to get quota");

    assert!(quota.is_some());
    let quota = quota.unwrap();
    assert_eq!(quota.daily_token_limit, Some(100_000));
    assert_eq!(quota.max_tokens_per_request, Some(4000));

    cleanup_test_tenant(&db, tenant_id).await;
}

#[tokio::test]
#[ignore]
async fn test_auth_invalid_key() {
    // Test that invalid API keys are rejected with 401
    // TODO: Requires running gateway server
}

#[tokio::test]
#[ignore]
async fn test_quota_exceeded() {
    // Test that requests exceeding quota are rejected with 429
    // TODO: Requires running gateway server
}

#[tokio::test]
#[ignore]
async fn test_full_chat_request_flow() {
    let db = create_test_db_pool().await;

    // Create tenant with quota
    let tenant_id = create_test_tenant(&db, "test-chat-flow").await;
    let api_key = create_test_api_key(&db, tenant_id, "chat-key").await;
    set_tenant_quota(&db, tenant_id, Some(10_000), Some(2000)).await;

    // TODO: Start gateway server, make request, verify:
    // 1. Auth passes
    // 2. Quota check passes
    // 3. Handler returns response
    // 4. Token usage is tracked

    cleanup_test_tenant(&db, tenant_id).await;
}

// NOTE: Full E2E testing requires:
// 1. Starting the gateway server in a test harness
// 2. Making HTTP requests via reqwest
// 3. Verifying responses and side effects
//
// Example structure:
// ```rust,ignore
// let server = spawn_test_server(db.clone()).await;
// let client = reqwest::Client::new();
//
// let response = client
//     .post(format!("{}/v1/chat/completions", server.addr()))
//     .header("Authorization", format!("Bearer {}", api_key))
//     .json(&json!({
//         "model": "gpt-4",
//         "messages": [{"role": "user", "content": "Hello"}],
//         "max_tokens": 100,
//         "stream": false,
//     }))
//     .send()
//     .await
//     .unwrap();
//
// assert_eq!(response.status(), 200);
// let body: serde_json::Value = response.json().await.unwrap();
// assert_eq!(body["object"], "chat.completion");
// ```
