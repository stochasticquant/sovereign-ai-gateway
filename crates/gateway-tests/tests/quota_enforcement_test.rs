//! End-to-end tests for token quota enforcement.

use chrono::Utc;
use token_governor::{QuotaCheckRequest, QuotaCheckResult, QuotaEnforcer, TenantQuota, TokenCounter, UsageIncrement};
use uuid::Uuid;

/// Helper to create a test database pool.
/// Note: This requires DATABASE_URL environment variable to be set.
async fn create_test_db_pool() -> sqlx::PgPool {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://localhost/sovereign_gateway_test".to_string());

    sqlx::PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to test database")
}

/// Helper to create a test tenant in the database.
async fn create_test_tenant(db: &sqlx::PgPool, name: &str) -> Uuid {
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

/// Helper to clean up test tenant.
async fn cleanup_test_tenant(db: &sqlx::PgPool, tenant_id: Uuid) {
    sqlx::query!("DELETE FROM tenants WHERE id = $1", tenant_id)
        .execute(db)
        .await
        .ok();
}

#[tokio::test]
#[ignore] // Run with: cargo test --package gateway-tests -- --ignored
async fn test_quota_check_no_quota_configured() {
    let db = create_test_db_pool().await;
    let tenant_id = create_test_tenant(&db, "test-tenant-no-quota").await;

    let enforcer = QuotaEnforcer::new(db.clone());

    // No quota configured, should allow request
    let request = QuotaCheckRequest {
        tenant_id,
        requested_tokens: 5000,
    };

    let result = enforcer.check_quota(request).await.unwrap();
    assert!(matches!(result, QuotaCheckResult::Allowed));

    cleanup_test_tenant(&db, tenant_id).await;
}

#[tokio::test]
#[ignore]
async fn test_quota_check_within_daily_limit() {
    let db = create_test_db_pool().await;
    let tenant_id = create_test_tenant(&db, "test-tenant-within-limit").await;

    let enforcer = QuotaEnforcer::new(db.clone());

    // Set quota: 10,000 tokens per day
    let quota = TenantQuota {
        tenant_id,
        daily_token_limit: Some(10_000),
        max_tokens_per_request: None,
        max_requests_per_minute: None,
        max_concurrent_requests: None,
    };
    enforcer.set_quota(quota).await.unwrap();

    // Request 5,000 tokens (within limit)
    let request = QuotaCheckRequest {
        tenant_id,
        requested_tokens: 5000,
    };

    let result = enforcer.check_quota(request).await.unwrap();
    assert!(matches!(result, QuotaCheckResult::Allowed));

    cleanup_test_tenant(&db, tenant_id).await;
}

#[tokio::test]
#[ignore]
async fn test_quota_check_exceeds_daily_limit() {
    let db = create_test_db_pool().await;
    let tenant_id = create_test_tenant(&db, "test-tenant-exceed-limit").await;

    let enforcer = QuotaEnforcer::new(db.clone());
    let counter = TokenCounter::new(db.clone());

    // Set quota: 10,000 tokens per day
    let quota = TenantQuota {
        tenant_id,
        daily_token_limit: Some(10_000),
        max_tokens_per_request: None,
        max_requests_per_minute: None,
        max_concurrent_requests: None,
    };
    enforcer.set_quota(quota).await.unwrap();

    // Use 8,000 tokens
    let increment = UsageIncrement {
        tenant_id,
        provider_id: "openai".to_string(),
        model: "gpt-4".to_string(),
        tokens: 8000,
        estimated_cost_usd: 0.16,
    };
    counter.increment(increment).await.unwrap();

    // Try to use 3,000 more tokens (would exceed 10,000 limit)
    let request = QuotaCheckRequest {
        tenant_id,
        requested_tokens: 3000,
    };

    let result = enforcer.check_quota(request).await.unwrap();
    match result {
        QuotaCheckResult::Denied { reason, retry_after_secs } => {
            assert!(reason.contains("exceed daily token limit"));
            assert!(retry_after_secs.is_some());
        }
        QuotaCheckResult::Allowed => panic!("Expected quota to be denied"),
    }

    cleanup_test_tenant(&db, tenant_id).await;
}

#[tokio::test]
#[ignore]
async fn test_quota_check_exceeds_per_request_limit() {
    let db = create_test_db_pool().await;
    let tenant_id = create_test_tenant(&db, "test-tenant-per-request").await;

    let enforcer = QuotaEnforcer::new(db.clone());

    // Set quota: max 2,000 tokens per request
    let quota = TenantQuota {
        tenant_id,
        daily_token_limit: None,
        max_tokens_per_request: Some(2000),
        max_requests_per_minute: None,
        max_concurrent_requests: None,
    };
    enforcer.set_quota(quota).await.unwrap();

    // Request 3,000 tokens (exceeds per-request limit)
    let request = QuotaCheckRequest {
        tenant_id,
        requested_tokens: 3000,
    };

    let result = enforcer.check_quota(request).await.unwrap();
    match result {
        QuotaCheckResult::Denied { reason, retry_after_secs } => {
            assert!(reason.contains("per-request token limit"));
            assert!(retry_after_secs.is_none()); // No retry-after for per-request limits
        }
        QuotaCheckResult::Allowed => panic!("Expected quota to be denied"),
    }

    cleanup_test_tenant(&db, tenant_id).await;
}

#[tokio::test]
#[ignore]
async fn test_token_counter_upsert() {
    let db = create_test_db_pool().await;
    let tenant_id = create_test_tenant(&db, "test-tenant-counter").await;

    let counter = TokenCounter::new(db.clone());

    // First increment
    let increment1 = UsageIncrement {
        tenant_id,
        provider_id: "openai".to_string(),
        model: "gpt-4".to_string(),
        tokens: 1000,
        estimated_cost_usd: 0.02,
    };
    let record1 = counter.increment(increment1).await.unwrap();
    assert_eq!(record1.total_tokens, 1000);
    assert_eq!(record1.request_count, 1);

    // Second increment (should UPSERT)
    let increment2 = UsageIncrement {
        tenant_id,
        provider_id: "openai".to_string(),
        model: "gpt-4".to_string(),
        tokens: 500,
        estimated_cost_usd: 0.01,
    };
    let record2 = counter.increment(increment2).await.unwrap();
    assert_eq!(record2.total_tokens, 1500);
    assert_eq!(record2.request_count, 2);
    assert_eq!(record2.id, record1.id); // Same record ID (UPSERT)

    // Get total usage
    let today = Utc::now().date_naive();
    let total = counter.get_total_usage(tenant_id, today).await.unwrap();
    assert_eq!(total, 1500);

    cleanup_test_tenant(&db, tenant_id).await;
}

#[tokio::test]
#[ignore]
async fn test_quota_update() {
    let db = create_test_db_pool().await;
    let tenant_id = create_test_tenant(&db, "test-tenant-update").await;

    let enforcer = QuotaEnforcer::new(db.clone());

    // Set initial quota
    let quota1 = TenantQuota {
        tenant_id,
        daily_token_limit: Some(5000),
        max_tokens_per_request: Some(1000),
        max_requests_per_minute: None,
        max_concurrent_requests: None,
    };
    enforcer.set_quota(quota1).await.unwrap();

    // Get quota
    let fetched1 = enforcer.get_quota(tenant_id).await.unwrap().unwrap();
    assert_eq!(fetched1.daily_token_limit, Some(5000));
    assert_eq!(fetched1.max_tokens_per_request, Some(1000));

    // Update quota (should UPSERT)
    let quota2 = TenantQuota {
        tenant_id,
        daily_token_limit: Some(10_000),
        max_tokens_per_request: Some(2000),
        max_requests_per_minute: Some(60),
        max_concurrent_requests: Some(10),
    };
    enforcer.set_quota(quota2).await.unwrap();

    // Verify update
    let fetched2 = enforcer.get_quota(tenant_id).await.unwrap().unwrap();
    assert_eq!(fetched2.daily_token_limit, Some(10_000));
    assert_eq!(fetched2.max_tokens_per_request, Some(2000));
    assert_eq!(fetched2.max_requests_per_minute, Some(60));
    assert_eq!(fetched2.max_concurrent_requests, Some(10));

    cleanup_test_tenant(&db, tenant_id).await;
}
