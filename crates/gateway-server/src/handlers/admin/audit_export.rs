//! Audit log export endpoint for regulatory compliance.
//!
//! GET /admin/audit/export?tenant_id=...&start=...&end=...&format=json|csv

use audit_log::{AuditExporter, ExportFilter, ExportFormat};
use axum::{
    extract::{Query, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct ExportQuery {
    pub tenant_id: Uuid,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    #[serde(default = "default_format")]
    pub format: String,
    pub classification: Option<String>,
    pub provider_id: Option<String>,
    pub decision: Option<String>,
    pub min_risk_score: Option<u8>,
}

fn default_format() -> String {
    "json".to_string()
}

/// GET /admin/audit/export
///
/// Export audit log entries as JSON or CSV for regulatory review.
pub async fn export(State(state): State<AppState>, Query(params): Query<ExportQuery>) -> Response {
    let format = ExportFormat::from_str_lossy(&params.format);
    let exporter = AuditExporter::new(state.db.clone());

    let filter = ExportFilter {
        tenant_id: params.tenant_id,
        start_date: params.start,
        end_date: params.end,
        data_classification: params.classification,
        provider_id: params.provider_id,
        decision: params.decision,
        min_risk_score: params.min_risk_score,
    };

    let result = match format {
        ExportFormat::Json => exporter.export_json(&filter).await,
        ExportFormat::Csv => exporter.export_csv(&filter).await,
    };

    match result {
        Ok(body) => {
            let mut response = (StatusCode::OK, body).into_response();
            if let Ok(content_type) = HeaderValue::from_str(format.content_type()) {
                response
                    .headers_mut()
                    .insert(header::CONTENT_TYPE, content_type);
            }
            response
        }
        Err(e) => {
            tracing::error!(error = %e, "Audit export failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Export failed: {}", e),
            )
                .into_response()
        }
    }
}
