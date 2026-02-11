//! Detection reports and PII types.
//!
//! Structures for representing PII detections and classification results.

use gateway_core::types::DataClassification;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Types of PII that can be detected.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PIIType {
    Email,
    PhoneNumber,
    CreditCard,
    SSN,
    IPAddress,
    APIKey,
    NationalId,
    MedicalRecord,
    BankAccount,
    DrugName,
    DiagnosisCode,
    // Africa-specific
    SouthAfricanId,
    NigerianBVN,
    KenyanNationalId,
    MobileMoney,
}

impl PIIType {
    /// Get the category name for this PII type.
    pub fn category(&self) -> &'static str {
        match self {
            PIIType::Email => "email",
            PIIType::PhoneNumber => "phone_number",
            PIIType::CreditCard => "credit_card",
            PIIType::SSN => "ssn",
            PIIType::IPAddress => "ip_address",
            PIIType::APIKey => "api_key",
            PIIType::NationalId => "national_id",
            PIIType::MedicalRecord => "medical_record",
            PIIType::BankAccount => "bank_account",
            PIIType::DrugName => "drug_name",
            PIIType::DiagnosisCode => "diagnosis_code",
            PIIType::SouthAfricanId => "south_african_id",
            PIIType::NigerianBVN => "nigerian_bvn",
            PIIType::KenyanNationalId => "kenyan_national_id",
            PIIType::MobileMoney => "mobile_money",
        }
    }

    /// Get the severity score for this PII type (0-10).
    pub fn severity(&self) -> u8 {
        match self {
            PIIType::NationalId
            | PIIType::SouthAfricanId
            | PIIType::NigerianBVN
            | PIIType::KenyanNationalId => 10,
            PIIType::MedicalRecord | PIIType::DiagnosisCode => 10,
            PIIType::CreditCard | PIIType::BankAccount => 9,
            PIIType::SSN => 9,
            PIIType::APIKey => 8,
            PIIType::PhoneNumber | PIIType::MobileMoney => 6,
            PIIType::Email => 5,
            PIIType::IPAddress => 4,
            PIIType::DrugName => 3,
        }
    }
}

/// A single PII detection at a specific location.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Detection {
    /// Type of PII detected.
    pub pii_type: PIIType,

    /// Byte offset in the original text.
    pub offset: usize,

    /// Length of the detected PII in bytes.
    pub length: usize,

    /// Confidence score (0.0 - 1.0).
    pub confidence: f64,

    /// Original text that was detected.
    pub original: String,

    /// Redacted version (if redaction has been applied).
    pub redacted: String,
}

/// Report of all PII detections in a text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionReport {
    /// All individual detections.
    pub detections: Vec<Detection>,

    /// Data classification based on detections.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub classification: Option<DataClassification>,

    /// Risk score (0-100) based on PII severity and count.
    pub risk_score: u8,
}

impl DetectionReport {
    /// Create a new detection report from detections.
    pub fn new(detections: Vec<Detection>) -> Self {
        let classification = Self::determine_classification(&detections);
        let risk_score = Self::calculate_risk_score(&detections);

        Self {
            detections,
            classification: Some(classification),
            risk_score,
        }
    }

    /// Determine data classification based on detections.
    fn determine_classification(detections: &[Detection]) -> DataClassification {
        if detections.is_empty() {
            return DataClassification::Public;
        }

        // If any high-severity PII is detected, classify as RESTRICTED
        let max_severity = detections
            .iter()
            .map(|d| d.pii_type.severity())
            .max()
            .unwrap_or(0);

        if max_severity >= 9 {
            DataClassification::Restricted
        } else if max_severity >= 6 {
            DataClassification::Confidential
        } else if max_severity >= 3 {
            DataClassification::Internal
        } else {
            DataClassification::Public
        }
    }

    /// Calculate risk score (0-100).
    fn calculate_risk_score(detections: &[Detection]) -> u8 {
        if detections.is_empty() {
            return 0;
        }

        // Risk = (average severity * 5) + (count factor)
        let total_severity: u32 = detections
            .iter()
            .map(|d| d.pii_type.severity() as u32)
            .sum();

        let avg_severity = total_severity / detections.len() as u32;
        let count_factor = (detections.len().min(10) * 5) as u32; // Up to 50 points for count

        let risk = (avg_severity * 5) + count_factor;
        risk.min(100) as u8
    }

    /// Get PII categories detected.
    pub fn pii_categories(&self) -> Vec<String> {
        let mut categories: HashMap<String, usize> = HashMap::new();

        for detection in &self.detections {
            *categories.entry(detection.pii_type.category().to_string())
                .or_insert(0) += 1;
        }

        categories.into_keys().collect()
    }

    /// Check if any PII was detected.
    pub fn has_pii(&self) -> bool {
        !self.detections.is_empty()
    }
}

/// Legacy: Summary report produced by the context firewall (for compatibility).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensitivityReport {
    /// Whether any PII was detected in the input.
    pub pii_detected: bool,

    /// The overall data classification determined by the firewall.
    pub classification: DataClassification,

    /// Numeric risk score (0–100) based on PII density and severity.
    pub risk_score: u8,

    /// Breakdown of detected PII categories and their counts.
    pub detections: Vec<CategoryCount>,
}

/// Count of detections for a specific category.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryCount {
    /// The category of PII detected.
    pub category: String,

    /// Number of occurrences found.
    pub count: usize,

    /// Byte offset ranges in the original text (for redaction).
    pub offsets: Vec<(usize, usize)>,
}
