//! Async buffered audit log writer with PostgreSQL storage and file fallback.
//!
//! This module provides non-blocking audit logging with:
//! - Channel-based async writes (no request blocking)
//! - Batch insertion for efficiency
//! - Local file fallback when DB is unavailable
//! - Automatic recovery from fallback files on startup

use crate::schema::AuditEntry;
use gateway_core::error::{GatewayError, GatewayResult};
use sqlx::{PgPool, QueryBuilder, postgres::PgQueryResult};
use std::path::{Path, PathBuf};
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tokio::time::{Duration, interval};
use tracing::{debug, error, info, instrument, warn};

/// Configuration for the audit writer.
#[derive(Debug, Clone)]
pub struct AuditWriterConfig {
    /// Maximum number of entries to buffer in memory before flushing.
    pub buffer_size: usize,

    /// Number of entries to batch per database write.
    pub batch_size: usize,

    /// Flush interval in seconds (even if batch not full).
    pub flush_interval_secs: u64,

    /// Directory for fallback JSON files when DB is unavailable.
    pub fallback_dir: PathBuf,
}

impl Default for AuditWriterConfig {
    fn default() -> Self {
        Self {
            buffer_size: 10_000,
            batch_size: 100,
            flush_interval_secs: 5,
            fallback_dir: PathBuf::from("./audit_fallback"),
        }
    }
}

/// Async buffered audit log writer.
///
/// Sends entries to a background task via an unbounded channel.
/// The background task batches writes to PostgreSQL for efficiency.
/// If database writes fail, entries are written to fallback JSON files.
#[derive(Clone)]
pub struct AuditWriter {
    sender: mpsc::UnboundedSender<AuditEntry>,
}

impl AuditWriter {
    /// Create a new audit writer and start the background writer task.
    ///
    /// This spawns a Tokio task that receives entries from the channel,
    /// batches them, and writes to PostgreSQL. On DB errors, it falls back
    /// to writing JSON files.
    #[instrument(skip(db_pool, config))]
    pub async fn new(db_pool: PgPool, config: AuditWriterConfig) -> GatewayResult<Self> {
        // Ensure fallback directory exists
        tokio::fs::create_dir_all(&config.fallback_dir)
            .await
            .map_err(|e| {
                GatewayError::Internal(format!("Failed to create audit fallback directory: {}", e))
            })?;

        // Recover any existing fallback files on startup
        Self::recover_fallback_files(&db_pool, &config.fallback_dir).await?;

        // Create unbounded channel for non-blocking writes
        let (sender, receiver) = mpsc::unbounded_channel();

        // Spawn background writer task
        let db_pool_clone = db_pool.clone();
        let config_clone = config.clone();
        tokio::spawn(async move {
            Self::writer_task(receiver, db_pool_clone, config_clone).await;
        });

        info!(
            "Audit writer started with batch_size={}, flush_interval={}s",
            config.batch_size, config.flush_interval_secs
        );

        Ok(Self { sender })
    }

    /// Write an audit entry (non-blocking).
    ///
    /// Sends the entry to the background task via channel.
    /// This never blocks the caller.
    #[instrument(skip(self, entry), fields(entry_id = %entry.id))]
    pub fn write(&self, entry: AuditEntry) -> GatewayResult<()> {
        self.sender.send(entry).map_err(|e| {
            error!("Failed to send audit entry to writer task: {}", e);
            GatewayError::Internal("Audit writer channel closed".to_string())
        })
    }

    /// Background task that receives entries and writes them in batches.
    async fn writer_task(
        mut receiver: mpsc::UnboundedReceiver<AuditEntry>,
        db_pool: PgPool,
        config: AuditWriterConfig,
    ) {
        let mut buffer = Vec::with_capacity(config.batch_size);
        let mut flush_timer = interval(Duration::from_secs(config.flush_interval_secs));

        loop {
            tokio::select! {
                // Receive entries from channel
                Some(entry) = receiver.recv() => {
                    buffer.push(entry);

                    // Flush if buffer is full
                    if buffer.len() >= config.batch_size {
                        Self::flush_batch(&db_pool, &mut buffer, &config.fallback_dir).await;
                    }
                }

                // Periodic flush even if batch not full
                _ = flush_timer.tick() => {
                    if !buffer.is_empty() {
                        Self::flush_batch(&db_pool, &mut buffer, &config.fallback_dir).await;
                    }
                }

                // Channel closed, flush remaining and exit
                else => {
                    if !buffer.is_empty() {
                        Self::flush_batch(&db_pool, &mut buffer, &config.fallback_dir).await;
                    }
                    info!("Audit writer task shutting down");
                    break;
                }
            }
        }
    }

    /// Flush a batch of entries to PostgreSQL.
    /// On error, write to fallback file instead.
    #[instrument(skip(db_pool, buffer, fallback_dir))]
    async fn flush_batch(db_pool: &PgPool, buffer: &mut Vec<AuditEntry>, fallback_dir: &Path) {
        if buffer.is_empty() {
            return;
        }

        let count = buffer.len();
        debug!("Flushing {} audit entries to database", count);

        match Self::write_batch_to_db(db_pool, buffer).await {
            Ok(result) => {
                debug!(
                    "Successfully wrote {} audit entries to database",
                    result.rows_affected()
                );
                buffer.clear();
            }
            Err(e) => {
                error!(
                    "Failed to write audit batch to database: {}. Falling back to file.",
                    e
                );
                if let Err(e) = Self::write_batch_to_fallback(buffer, fallback_dir).await {
                    error!(
                        "CRITICAL: Failed to write audit entries to fallback file: {}",
                        e
                    );
                    // Still clear buffer to avoid infinite growth
                    buffer.clear();
                } else {
                    info!("Wrote {} audit entries to fallback file", count);
                    buffer.clear();
                }
            }
        }
    }

    /// Write a batch of entries to PostgreSQL using batch insert.
    #[instrument(skip(db_pool, entries))]
    async fn write_batch_to_db(
        db_pool: &PgPool,
        entries: &[AuditEntry],
    ) -> Result<PgQueryResult, sqlx::Error> {
        if entries.is_empty() {
            return Ok(PgQueryResult::default());
        }

        // Use QueryBuilder for batch INSERT
        let mut query_builder = QueryBuilder::new(
            "INSERT INTO audit_log (id, request_id, timestamp, tenant_id, data_classification, provider_id, model, prompt_tokens, completion_tokens, total_tokens, region, risk_score, decision, latency_ms, status_code, error) ",
        );

        query_builder.push_values(entries, |mut b, entry| {
            b.push_bind(entry.id)
                .push_bind(&entry.request_id.0)
                .push_bind(entry.timestamp)
                .push_bind(entry.tenant_id.0)
                .push_bind(format!("{:?}", entry.data_classification))
                .push_bind(&entry.provider_id.0)
                .push_bind(&entry.model)
                .push_bind(entry.prompt_tokens as i32)
                .push_bind(entry.completion_tokens as i32)
                .push_bind(entry.total_tokens as i32)
                .push_bind(&entry.region)
                .push_bind(entry.risk_score as i16)
                .push_bind(&entry.decision)
                .push_bind(entry.latency_ms as i64)
                .push_bind(entry.status_code as i32)
                .push_bind(&entry.error);
        });

        query_builder.build().execute(db_pool).await
    }

    /// Write entries to fallback JSON file when DB is unavailable.
    #[instrument(skip(entries, fallback_dir))]
    async fn write_batch_to_fallback(
        entries: &[AuditEntry],
        fallback_dir: &Path,
    ) -> Result<(), std::io::Error> {
        // Create fallback file with timestamp
        let filename = format!("audit_fallback_{}.jsonl", chrono::Utc::now().timestamp());
        let filepath = fallback_dir.join(filename);

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&filepath)
            .await?;

        // Write each entry as JSON line
        for entry in entries {
            let json_line = serde_json::to_string(entry)?;
            file.write_all(json_line.as_bytes()).await?;
            file.write_all(b"\n").await?;
        }

        file.flush().await?;
        debug!(
            "Wrote {} entries to fallback file: {}",
            entries.len(),
            filepath.display()
        );

        Ok(())
    }

    /// Recover audit entries from fallback files on startup.
    ///
    /// Reads all *.jsonl files in the fallback directory,
    /// attempts to write them to the database, and deletes
    /// the files on success.
    #[instrument(skip(db_pool))]
    async fn recover_fallback_files(db_pool: &PgPool, fallback_dir: &PathBuf) -> GatewayResult<()> {
        // Check if directory exists
        if !fallback_dir.exists() {
            return Ok(());
        }

        let mut dir = tokio::fs::read_dir(fallback_dir).await.map_err(|e| {
            GatewayError::Internal(format!("Failed to read fallback directory: {}", e))
        })?;

        let mut recovered_count = 0;

        while let Some(entry) = dir.next_entry().await.map_err(|e| {
            GatewayError::Internal(format!("Failed to read fallback directory entry: {}", e))
        })? {
            let path = entry.path();

            // Only process .jsonl files
            if let Some(ext) = path.extension() {
                if ext != "jsonl" {
                    continue;
                }
            } else {
                continue;
            }

            info!(
                "Recovering audit entries from fallback file: {}",
                path.display()
            );

            // Read and parse entries
            let content = tokio::fs::read_to_string(&path).await.map_err(|e| {
                GatewayError::Internal(format!("Failed to read fallback file: {}", e))
            })?;

            let mut entries = Vec::new();
            for line in content.lines() {
                if line.trim().is_empty() {
                    continue;
                }

                match serde_json::from_str::<AuditEntry>(line) {
                    Ok(entry) => entries.push(entry),
                    Err(e) => {
                        warn!("Failed to parse audit entry from fallback file: {}", e);
                        continue;
                    }
                }
            }

            if entries.is_empty() {
                warn!("No valid entries in fallback file: {}", path.display());
                continue;
            }

            // Try to write to database
            match Self::write_batch_to_db(db_pool, &entries).await {
                Ok(_) => {
                    info!(
                        "Recovered {} audit entries from {}",
                        entries.len(),
                        path.display()
                    );
                    recovered_count += entries.len();

                    // Delete fallback file on success
                    if let Err(e) = tokio::fs::remove_file(&path).await {
                        warn!("Failed to delete fallback file after recovery: {}", e);
                    }
                }
                Err(e) => {
                    warn!(
                        "Failed to recover entries from {}: {}. Will retry on next startup.",
                        path.display(),
                        e
                    );
                }
            }
        }

        if recovered_count > 0 {
            info!(
                "Recovered {} total audit entries from fallback files",
                recovered_count
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gateway_core::types::{DataClassification, ProviderId, RequestId, TenantId};
    use uuid::Uuid;

    fn create_test_entry() -> AuditEntry {
        AuditEntry {
            id: Uuid::new_v4(),
            request_id: RequestId("test-request-123".to_string()),
            timestamp: chrono::Utc::now(),
            tenant_id: TenantId(Uuid::new_v4()),
            data_classification: DataClassification::Public,
            provider_id: ProviderId("openai".to_string()),
            model: "gpt-4".to_string(),
            prompt_tokens: 100,
            completion_tokens: 50,
            total_tokens: 150,
            region: "us-east-1".to_string(),
            risk_score: 0,
            decision: "allow".to_string(),
            latency_ms: 1500,
            status_code: 200,
            error: None,
        }
    }

    #[test]
    fn test_audit_entry_serialization() {
        let entry = create_test_entry();
        let json = serde_json::to_string(&entry).expect("Failed to serialize");
        let deserialized: AuditEntry = serde_json::from_str(&json).expect("Failed to deserialize");

        assert_eq!(entry.id, deserialized.id);
        assert_eq!(entry.request_id.0, deserialized.request_id.0);
        assert_eq!(entry.model, deserialized.model);
    }

    #[test]
    fn test_config_defaults() {
        let config = AuditWriterConfig::default();
        assert_eq!(config.buffer_size, 10_000);
        assert_eq!(config.batch_size, 100);
        assert_eq!(config.flush_interval_secs, 5);
    }
}
