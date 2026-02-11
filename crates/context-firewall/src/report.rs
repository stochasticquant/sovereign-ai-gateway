use gateway_core::types::DataClassification;
use serde::{Deserialize, Serialize};

/// Summary report produced by the context firewall for each scanned request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensitivityReport {
    /// Whether any PII was detected in the input.
    pub pii_detected: bool,

    /// The overall data classification determined by the firewall.
    pub classification: DataClassification,

    /// Numeric risk score (0–100) based on PII density and severity.
    pub risk_score: u8,

    /// Breakdown of detected PII categories and their counts.
    pub detections: Vec<Detection>,
}

/// A single PII detection within the scanned text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Detection {
    /// The category of PII detected (e.g., "email", "national_id", "medical_term").
    pub category: String,

    /// Number of occurrences found.
    pub count: usize,

    /// Byte offset ranges in the original text (for redaction).
    pub offsets: Vec<(usize, usize)>,
}
