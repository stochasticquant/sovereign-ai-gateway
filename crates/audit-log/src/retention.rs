//! Audit log retention policies and cleanup.
//!
//! Manages time-based retention policies per tenant with background cleanup jobs.
//! Old audit entries are deleted according to retention policies to manage database size.

use gateway_core::error::{GatewayError, GatewayResult};
use gateway_core::types::TenantId;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};
use tracing::{debug, error, info, instrument, warn};

/// Retention policy for audit logs.
#[derive(Debug, Clone)]
pub struct RetentionPolicy {
    /// Tenant ID (None for default policy).
    pub tenant_id: Option<TenantId>,

    /// Number of days to retain audit logs.
    pub retention_days: u32,
}

impl RetentionPolicy {
    /// Create a default retention policy.
    pub fn default_policy(retention_days: u32) -> Self {
        Self {
            tenant_id: None,
            retention_days,
        }
    }

    /// Create a tenant-specific retention policy.
    pub fn tenant_policy(tenant_id: TenantId, retention_days: u32) -> Self {
        Self {
            tenant_id: Some(tenant_id),
            retention_days,
        }
    }
}

/// Manages audit log retention policies and periodic cleanup.
pub struct RetentionManager {
    db_pool: PgPool,
    policies: Arc<RwLock<HashMap<TenantId, RetentionPolicy>>>,
    default_retention_days: u32,
}

impl RetentionManager {
    /// Create a new retention manager.
    ///
    /// This loads retention policies from the database and starts
    /// a background task that runs daily cleanup.
    #[instrument(skip(db_pool))]
    pub async fn new(db_pool: PgPool, default_retention_days: u32) -> GatewayResult<Self> {
        let policies = Arc::new(RwLock::new(HashMap::new()));

        let manager = Self {
            db_pool: db_pool.clone(),
            policies: policies.clone(),
            default_retention_days,
        };

        // Load policies from database
        manager.load_policies().await?;

        // Spawn background cleanup task
        let db_pool_clone = db_pool.clone();
        let policies_clone = policies.clone();
        let default_days = default_retention_days;

        tokio::spawn(async move {
            Self::cleanup_task(db_pool_clone, policies_clone, default_days).await;
        });

        info!(
            "Retention manager started with default retention: {} days",
            default_retention_days
        );

        Ok(manager)
    }

    /// Load retention policies from the database.
    #[instrument(skip(self))]
    async fn load_policies(&self) -> GatewayResult<()> {
        // Query retention policies table
        let rows = sqlx::query!(
            r#"
            SELECT tenant_id, retention_days
            FROM audit_retention_policies
            "#
        )
        .fetch_all(&self.db_pool)
        .await
        .map_err(|e| {
            warn!("Failed to load retention policies (table may not exist): {}", e);
            GatewayError::Internal(format!("Failed to load retention policies: {}", e))
        })?;

        let mut policies = self.policies.write().await;

        for row in rows {
            let tenant_id = TenantId(row.tenant_id);
            let policy = RetentionPolicy::tenant_policy(tenant_id, row.retention_days as u32);
            policies.insert(tenant_id, policy);
        }

        info!("Loaded {} tenant-specific retention policies", policies.len());

        Ok(())
    }

    /// Get the retention policy for a tenant.
    ///
    /// Returns the tenant-specific policy if it exists, otherwise the default policy.
    pub async fn get_policy(&self, tenant_id: &TenantId) -> RetentionPolicy {
        let policies = self.policies.read().await;

        policies
            .get(tenant_id)
            .cloned()
            .unwrap_or_else(|| RetentionPolicy::default_policy(self.default_retention_days))
    }

    /// Add or update a retention policy for a tenant.
    #[instrument(skip(self))]
    pub async fn set_policy(&self, tenant_id: TenantId, retention_days: u32) -> GatewayResult<()> {
        // Upsert into database
        sqlx::query!(
            r#"
            INSERT INTO audit_retention_policies (tenant_id, retention_days)
            VALUES ($1, $2)
            ON CONFLICT (tenant_id)
            DO UPDATE SET retention_days = EXCLUDED.retention_days
            "#,
            tenant_id.0,
            retention_days as i32
        )
        .execute(&self.db_pool)
        .await
        .map_err(|e| {
            error!("Failed to set retention policy: {}", e);
            GatewayError::Internal(format!("Failed to set retention policy: {}", e))
        })?;

        // Update in-memory cache
        let mut policies = self.policies.write().await;
        policies.insert(
            tenant_id,
            RetentionPolicy::tenant_policy(tenant_id, retention_days),
        );

        info!(
            "Set retention policy for tenant {}: {} days",
            tenant_id.0, retention_days
        );

        Ok(())
    }

    /// Background task that runs daily cleanup.
    async fn cleanup_task(
        db_pool: PgPool,
        policies: Arc<RwLock<HashMap<TenantId, RetentionPolicy>>>,
        default_retention_days: u32,
    ) {
        // Run cleanup daily at 2 AM UTC
        let mut cleanup_interval = interval(Duration::from_secs(24 * 60 * 60));

        loop {
            cleanup_interval.tick().await;

            info!("Running daily audit log cleanup");

            // Get list of all tenants from audit log
            let tenant_ids = match Self::get_tenant_ids(&db_pool).await {
                Ok(ids) => ids,
                Err(e) => {
                    error!("Failed to get tenant IDs for cleanup: {}", e);
                    continue;
                }
            };

            let policies_read = policies.read().await;

            for tenant_id in tenant_ids {
                // Get retention policy for this tenant
                let retention_days = policies_read
                    .get(&tenant_id)
                    .map(|p| p.retention_days)
                    .unwrap_or(default_retention_days);

                // Delete old entries
                match Self::delete_old_entries(&db_pool, &tenant_id, retention_days).await {
                    Ok(deleted_count) => {
                        if deleted_count > 0 {
                            info!(
                                "Deleted {} old audit entries for tenant {} (retention: {} days)",
                                deleted_count, tenant_id.0, retention_days
                            );
                        } else {
                            debug!(
                                "No old audit entries to delete for tenant {}",
                                tenant_id.0
                            );
                        }
                    }
                    Err(e) => {
                        error!(
                            "Failed to delete old audit entries for tenant {}: {}",
                            tenant_id.0, e
                        );
                    }
                }
            }

            info!("Daily audit log cleanup completed");
        }
    }

    /// Get list of distinct tenant IDs from audit log.
    async fn get_tenant_ids(db_pool: &PgPool) -> Result<Vec<TenantId>, sqlx::Error> {
        let rows = sqlx::query!(
            r#"
            SELECT DISTINCT tenant_id
            FROM audit_log
            "#
        )
        .fetch_all(db_pool)
        .await?;

        Ok(rows.into_iter().map(|r| TenantId(r.tenant_id)).collect())
    }

    /// Delete audit log entries older than retention days for a tenant.
    #[instrument(skip(db_pool))]
    async fn delete_old_entries(
        db_pool: &PgPool,
        tenant_id: &TenantId,
        retention_days: u32,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            DELETE FROM audit_log
            WHERE tenant_id = $1
              AND timestamp < NOW() - INTERVAL '1 day' * $2
            "#,
            tenant_id.0,
            retention_days as i32
        )
        .execute(db_pool)
        .await?;

        Ok(result.rows_affected())
    }

    /// Manually trigger cleanup for a specific tenant (for testing/admin).
    #[instrument(skip(self))]
    pub async fn cleanup_tenant(&self, tenant_id: &TenantId) -> GatewayResult<u64> {
        let policy = self.get_policy(tenant_id).await;

        let deleted = Self::delete_old_entries(&self.db_pool, tenant_id, policy.retention_days)
            .await
            .map_err(|e| {
                error!("Failed to cleanup tenant {}: {}", tenant_id.0, e);
                GatewayError::Internal(format!("Failed to cleanup tenant: {}", e))
            })?;

        info!(
            "Manual cleanup for tenant {}: deleted {} entries",
            tenant_id.0, deleted
        );

        Ok(deleted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_default_policy_creation() {
        let policy = RetentionPolicy::default_policy(90);
        assert!(policy.tenant_id.is_none());
        assert_eq!(policy.retention_days, 90);
    }

    #[test]
    fn test_tenant_policy_creation() {
        let tenant_id = TenantId(Uuid::new_v4());
        let policy = RetentionPolicy::tenant_policy(tenant_id, 30);
        assert_eq!(policy.tenant_id.unwrap().0, tenant_id.0);
        assert_eq!(policy.retention_days, 30);
    }
}
