//! Bulk audit export for regulators.
//!
//! Paginated, filterable export producing JSON or CSV output.
//! Designed for regulatory submissions and compliance audits.

use crate::schema::AuditEntry;
use chrono::{DateTime, Utc};
use gateway_core::error::{GatewayError, GatewayResult};
use sqlx::PgPool;
use tracing::instrument;
use uuid::Uuid;

/// Maximum page size to prevent memory exhaustion.
const MAX_PAGE_SIZE: u32 = 10_000;

/// Export output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Json,
    Csv,
}

impl ExportFormat {
    /// Parse from string (case-insensitive).
    pub fn from_str_lossy(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "csv" => Self::Csv,
            _ => Self::Json,
        }
    }

    /// MIME content type for the format.
    pub fn content_type(&self) -> &'static str {
        match self {
            Self::Json => "application/json",
            Self::Csv => "text/csv",
        }
    }
}

/// Filters for audit export queries.
#[derive(Debug, Clone)]
pub struct ExportFilter {
    /// Required: tenant to export for.
    pub tenant_id: Uuid,
    /// Start of time range (inclusive).
    pub start_date: DateTime<Utc>,
    /// End of time range (inclusive).
    pub end_date: DateTime<Utc>,
    /// Optional: filter by data classification.
    pub data_classification: Option<String>,
    /// Optional: filter by provider.
    pub provider_id: Option<String>,
    /// Optional: filter by decision (allow, block, redact_and_route).
    pub decision: Option<String>,
    /// Optional: minimum risk score threshold.
    pub min_risk_score: Option<u8>,
}

/// A page of exported audit entries.
#[derive(Debug, Clone)]
pub struct ExportPage {
    /// Audit entries for this page.
    pub entries: Vec<AuditEntry>,
    /// Total number of matching entries across all pages.
    pub total_count: i64,
    /// Current page number (1-based).
    pub page: u32,
    /// Number of entries per page.
    pub page_size: u32,
    /// Whether more pages exist.
    pub has_more: bool,
}

/// Audit log exporter for regulatory compliance.
pub struct AuditExporter {
    db: PgPool,
}

impl AuditExporter {
    pub fn new(db: PgPool) -> Self {
        Self { db }
    }

    /// Query a paginated page of audit entries matching the filter.
    #[instrument(skip(self), fields(tenant_id = %filter.tenant_id, page, page_size))]
    pub async fn query_page(
        &self,
        filter: &ExportFilter,
        page: u32,
        page_size: u32,
    ) -> GatewayResult<ExportPage> {
        let page_size = page_size.min(MAX_PAGE_SIZE);
        let page = page.max(1);
        let offset = ((page - 1) * page_size) as i64;

        // Count total matching entries
        let total_count = self.count_matching(filter).await?;

        // Fetch the page
        let entries = self.fetch_page(filter, page_size as i64, offset).await?;

        let has_more = (offset + entries.len() as i64) < total_count;

        Ok(ExportPage {
            entries,
            total_count,
            page,
            page_size,
            has_more,
        })
    }

    /// Export matching entries as a JSON string.
    #[instrument(skip(self), fields(tenant_id = %filter.tenant_id))]
    pub async fn export_json(&self, filter: &ExportFilter) -> GatewayResult<String> {
        let mut all_entries = Vec::new();
        let mut page = 1u32;
        let page_size = MAX_PAGE_SIZE;

        loop {
            let result = self.query_page(filter, page, page_size).await?;
            all_entries.extend(result.entries);
            if !result.has_more {
                break;
            }
            page += 1;
        }

        serde_json::to_string_pretty(&all_entries).map_err(|e| {
            GatewayError::Internal(format!("Failed to serialize audit entries to JSON: {}", e))
        })
    }

    /// Export matching entries as a CSV string.
    #[instrument(skip(self), fields(tenant_id = %filter.tenant_id))]
    pub async fn export_csv(&self, filter: &ExportFilter) -> GatewayResult<String> {
        let mut wtr = csv::Writer::from_writer(Vec::new());

        // Write header
        wtr.write_record([
            "id",
            "request_id",
            "timestamp",
            "tenant_id",
            "data_classification",
            "provider_id",
            "model",
            "prompt_tokens",
            "completion_tokens",
            "total_tokens",
            "region",
            "risk_score",
            "decision",
            "latency_ms",
            "status_code",
            "error",
        ])
        .map_err(|e| GatewayError::Internal(format!("CSV header write failed: {}", e)))?;

        let mut page = 1u32;
        let page_size = MAX_PAGE_SIZE;

        loop {
            let result = self.query_page(filter, page, page_size).await?;
            for entry in &result.entries {
                wtr.write_record([
                    entry.id.to_string(),
                    entry.request_id.0.clone(),
                    entry.timestamp.to_rfc3339(),
                    entry.tenant_id.0.to_string(),
                    format!("{:?}", entry.data_classification),
                    entry.provider_id.0.clone(),
                    entry.model.clone(),
                    entry.prompt_tokens.to_string(),
                    entry.completion_tokens.to_string(),
                    entry.total_tokens.to_string(),
                    entry.region.clone(),
                    entry.risk_score.to_string(),
                    entry.decision.clone(),
                    entry.latency_ms.to_string(),
                    entry.status_code.to_string(),
                    entry.error.clone().unwrap_or_default(),
                ])
                .map_err(|e| GatewayError::Internal(format!("CSV row write failed: {}", e)))?;
            }
            if !result.has_more {
                break;
            }
            page += 1;
        }

        let bytes = wtr
            .into_inner()
            .map_err(|e| GatewayError::Internal(format!("CSV flush failed: {}", e)))?;

        String::from_utf8(bytes)
            .map_err(|e| GatewayError::Internal(format!("CSV encoding error: {}", e)))
    }

    /// Count entries matching the filter.
    async fn count_matching(&self, filter: &ExportFilter) -> GatewayResult<i64> {
        let mut builder =
            sqlx::QueryBuilder::new("SELECT COUNT(*) as count FROM audit_log WHERE tenant_id = ");
        builder.push_bind(filter.tenant_id);
        builder.push(" AND timestamp >= ");
        builder.push_bind(filter.start_date);
        builder.push(" AND timestamp <= ");
        builder.push_bind(filter.end_date);

        self.apply_optional_filters(&mut builder, filter);

        let row: (i64,) = builder
            .build_query_as()
            .fetch_one(&self.db)
            .await
            .map_err(|e| GatewayError::Internal(format!("Failed to count audit entries: {}", e)))?;

        Ok(row.0)
    }

    /// Fetch a page of entries matching the filter.
    async fn fetch_page(
        &self,
        filter: &ExportFilter,
        limit: i64,
        offset: i64,
    ) -> GatewayResult<Vec<AuditEntry>> {
        let mut builder = sqlx::QueryBuilder::new(
            "SELECT id, request_id, timestamp, tenant_id, data_classification, \
             provider_id, model, prompt_tokens, completion_tokens, total_tokens, \
             region, risk_score, decision, latency_ms, status_code, error \
             FROM audit_log WHERE tenant_id = ",
        );
        builder.push_bind(filter.tenant_id);
        builder.push(" AND timestamp >= ");
        builder.push_bind(filter.start_date);
        builder.push(" AND timestamp <= ");
        builder.push_bind(filter.end_date);

        self.apply_optional_filters(&mut builder, filter);

        builder.push(" ORDER BY timestamp DESC LIMIT ");
        builder.push_bind(limit);
        builder.push(" OFFSET ");
        builder.push_bind(offset);

        let rows = builder
            .build_query_as::<AuditExportRow>()
            .fetch_all(&self.db)
            .await
            .map_err(|e| GatewayError::Internal(format!("Failed to fetch audit entries: {}", e)))?;

        Ok(rows.into_iter().map(AuditEntry::from).collect())
    }

    /// Append optional WHERE clauses to the query builder.
    fn apply_optional_filters<'a>(
        &self,
        builder: &mut sqlx::QueryBuilder<'a, sqlx::Postgres>,
        filter: &'a ExportFilter,
    ) {
        if let Some(ref classification) = filter.data_classification {
            builder.push(" AND data_classification = ");
            builder.push_bind(classification.clone());
        }
        if let Some(ref provider) = filter.provider_id {
            builder.push(" AND provider_id = ");
            builder.push_bind(provider.clone());
        }
        if let Some(ref decision) = filter.decision {
            builder.push(" AND decision = ");
            builder.push_bind(decision.clone());
        }
        if let Some(min_risk) = filter.min_risk_score {
            builder.push(" AND risk_score >= ");
            builder.push_bind(min_risk as i16);
        }
    }
}

/// Internal row type for sqlx query mapping.
#[derive(sqlx::FromRow)]
struct AuditExportRow {
    id: Uuid,
    request_id: String,
    timestamp: DateTime<Utc>,
    tenant_id: Uuid,
    data_classification: Option<String>,
    provider_id: String,
    model: String,
    prompt_tokens: Option<i32>,
    completion_tokens: Option<i32>,
    total_tokens: Option<i32>,
    region: String,
    risk_score: Option<i16>,
    decision: String,
    latency_ms: Option<i32>,
    status_code: i16,
    error: Option<String>,
}

impl From<AuditExportRow> for AuditEntry {
    fn from(row: AuditExportRow) -> Self {
        use gateway_core::types::{DataClassification, ProviderId, RequestId, TenantId};

        let classification = match row.data_classification.as_deref() {
            Some("confidential") => DataClassification::Confidential,
            Some("restricted") => DataClassification::Restricted,
            Some("internal") => DataClassification::Internal,
            _ => DataClassification::Public,
        };

        Self {
            id: row.id,
            request_id: RequestId(row.request_id),
            timestamp: row.timestamp,
            tenant_id: TenantId(row.tenant_id),
            data_classification: classification,
            provider_id: ProviderId(row.provider_id),
            model: row.model,
            prompt_tokens: row.prompt_tokens.unwrap_or(0) as u32,
            completion_tokens: row.completion_tokens.unwrap_or(0) as u32,
            total_tokens: row.total_tokens.unwrap_or(0) as u32,
            region: row.region,
            risk_score: row.risk_score.unwrap_or(0) as u8,
            decision: row.decision,
            latency_ms: row.latency_ms.unwrap_or(0) as u64,
            status_code: row.status_code as u16,
            error: row.error,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_export_format_from_str() {
        assert_eq!(ExportFormat::from_str_lossy("json"), ExportFormat::Json);
        assert_eq!(ExportFormat::from_str_lossy("JSON"), ExportFormat::Json);
        assert_eq!(ExportFormat::from_str_lossy("csv"), ExportFormat::Csv);
        assert_eq!(ExportFormat::from_str_lossy("CSV"), ExportFormat::Csv);
        assert_eq!(ExportFormat::from_str_lossy("unknown"), ExportFormat::Json);
    }

    #[test]
    fn test_export_format_content_type() {
        assert_eq!(ExportFormat::Json.content_type(), "application/json");
        assert_eq!(ExportFormat::Csv.content_type(), "text/csv");
    }

    #[test]
    fn test_export_page_has_more() {
        let page = ExportPage {
            entries: vec![],
            total_count: 100,
            page: 1,
            page_size: 50,
            has_more: true,
        };
        assert!(page.has_more);
        assert_eq!(page.total_count, 100);
    }

    #[test]
    fn test_audit_export_row_conversion() {
        let row = AuditExportRow {
            id: Uuid::nil(),
            request_id: "req-123".to_string(),
            timestamp: Utc::now(),
            tenant_id: Uuid::nil(),
            data_classification: Some("confidential".to_string()),
            provider_id: "openai".to_string(),
            model: "gpt-4".to_string(),
            prompt_tokens: Some(100),
            completion_tokens: Some(50),
            total_tokens: Some(150),
            region: "ng-west-1".to_string(),
            risk_score: Some(25),
            decision: "allow".to_string(),
            latency_ms: Some(200),
            status_code: 200,
            error: None,
        };

        let entry = AuditEntry::from(row);
        assert_eq!(entry.prompt_tokens, 100);
        assert_eq!(entry.completion_tokens, 50);
        assert_eq!(entry.total_tokens, 150);
        assert_eq!(entry.risk_score, 25);
        assert_eq!(format!("{:?}", entry.data_classification), "Confidential");
    }

    #[test]
    fn test_audit_export_row_null_handling() {
        let row = AuditExportRow {
            id: Uuid::nil(),
            request_id: "req-456".to_string(),
            timestamp: Utc::now(),
            tenant_id: Uuid::nil(),
            data_classification: None,
            provider_id: "local".to_string(),
            model: "llama-3".to_string(),
            prompt_tokens: None,
            completion_tokens: None,
            total_tokens: None,
            region: "ke-east-1".to_string(),
            risk_score: None,
            decision: "allow".to_string(),
            latency_ms: None,
            status_code: 200,
            error: None,
        };

        let entry = AuditEntry::from(row);
        assert_eq!(entry.prompt_tokens, 0);
        assert_eq!(entry.completion_tokens, 0);
        assert_eq!(entry.total_tokens, 0);
        assert_eq!(entry.risk_score, 0);
        assert_eq!(entry.latency_ms, 0);
    }
}
