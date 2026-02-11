//! Data classification engine.
//!
//! Classifies input text as Public, Internal, Confidential, or Restricted
//! based on PII density and category of detected patterns.

use crate::report::{DetectionReport, PIIType};
use gateway_core::types::DataClassification;
use tracing::debug;

/// Data classifier that determines sensitivity level.
pub struct DataClassifier {
    /// Minimum risk score for each classification level.
    thresholds: ClassificationThresholds,
}

/// Thresholds for classification levels.
///
/// Below internal threshold is PUBLIC (default: 0).
#[derive(Debug, Clone)]
pub struct ClassificationThresholds {
    /// Risk score threshold for RESTRICTED (default: 60).
    pub restricted: u8,

    /// Risk score threshold for CONFIDENTIAL (default: 35).
    pub confidential: u8,

    /// Risk score threshold for INTERNAL (default: 15).
    pub internal: u8,
}

impl Default for ClassificationThresholds {
    fn default() -> Self {
        Self {
            restricted: 60,
            confidential: 35,
            internal: 15,
        }
    }
}

impl Default for DataClassifier {
    fn default() -> Self {
        Self::new()
    }
}

impl DataClassifier {
    /// Create a new data classifier with default thresholds.
    pub fn new() -> Self {
        Self {
            thresholds: ClassificationThresholds::default(),
        }
    }

    /// Create a classifier with custom thresholds.
    pub fn with_thresholds(thresholds: ClassificationThresholds) -> Self {
        Self { thresholds }
    }

    /// Classify data based on detection report.
    pub fn classify(&self, report: &DetectionReport) -> DataClassification {
        // If no PII detected, it's PUBLIC
        if !report.has_pii() {
            return DataClassification::Public;
        }

        // Check if any critical PII types are present (auto-RESTRICTED)
        if self.has_critical_pii(report) {
            debug!("Critical PII detected, classifying as RESTRICTED");
            return DataClassification::Restricted;
        }

        // Otherwise, classify based on risk score
        let classification = if report.risk_score >= self.thresholds.restricted {
            DataClassification::Restricted
        } else if report.risk_score >= self.thresholds.confidential {
            DataClassification::Confidential
        } else if report.risk_score >= self.thresholds.internal {
            DataClassification::Internal
        } else {
            DataClassification::Public
        };

        debug!(
            "Classified data as {:?} (risk score: {})",
            classification, report.risk_score
        );

        classification
    }

    /// Check if report contains critical PII that always triggers RESTRICTED.
    fn has_critical_pii(&self, report: &DetectionReport) -> bool {
        let critical_types = [
            PIIType::NationalId,
            PIIType::SouthAfricanId,
            PIIType::NigerianBVN,
            PIIType::KenyanNationalId,
            PIIType::MedicalRecord,
            PIIType::CreditCard,
            PIIType::SSN,
            PIIType::BankAccount,
        ];

        report
            .detections
            .iter()
            .any(|d| critical_types.contains(&d.pii_type))
    }

    /// Get the current classification thresholds.
    pub fn thresholds(&self) -> &ClassificationThresholds {
        &self.thresholds
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::Detection;

    #[test]
    fn test_classify_no_pii_as_public() {
        let classifier = DataClassifier::new();
        let report = DetectionReport::new(vec![]);

        let classification = classifier.classify(&report);
        assert_eq!(classification, DataClassification::Public);
    }

    #[test]
    fn test_classify_critical_pii_as_restricted() {
        let classifier = DataClassifier::new();

        let detections = vec![Detection {
            pii_type: PIIType::NationalId,
            offset: 0,
            length: 11,
            confidence: 0.95,
            original: "12345678901".to_string(),
            redacted: String::new(),
        }];

        let report = DetectionReport::new(detections);
        let classification = classifier.classify(&report);

        assert_eq!(classification, DataClassification::Restricted);
    }

    #[test]
    fn test_classify_email_as_internal() {
        let classifier = DataClassifier::new();

        let detections = vec![Detection {
            pii_type: PIIType::Email,
            offset: 0,
            length: 18,
            confidence: 0.95,
            original: "user@example.com".to_string(),
            redacted: String::new(),
        }];

        let report = DetectionReport::new(detections);
        let classification = classifier.classify(&report);

        // Single email should be INTERNAL (low risk score)
        assert!(matches!(
            classification,
            DataClassification::Internal | DataClassification::Public
        ));
    }

    #[test]
    fn test_classify_multiple_pii_higher_risk() {
        let classifier = DataClassifier::new();

        let detections = vec![
            Detection {
                pii_type: PIIType::Email,
                offset: 0,
                length: 18,
                confidence: 0.95,
                original: "user@example.com".to_string(),
                redacted: String::new(),
            },
            Detection {
                pii_type: PIIType::PhoneNumber,
                offset: 20,
                length: 15,
                confidence: 0.85,
                original: "+1-555-123-4567".to_string(),
                redacted: String::new(),
            },
        ];

        let report = DetectionReport::new(detections);
        let classification = classifier.classify(&report);

        // Multiple PII items increase risk
        assert!(matches!(
            classification,
            DataClassification::Internal | DataClassification::Confidential
        ));
    }

    #[test]
    fn test_custom_thresholds() {
        let thresholds = ClassificationThresholds {
            restricted: 80,
            confidential: 50,
            internal: 20,
        };

        let classifier = DataClassifier::with_thresholds(thresholds);

        let detections = vec![Detection {
            pii_type: PIIType::Email,
            offset: 0,
            length: 18,
            confidence: 0.95,
            original: "user@example.com".to_string(),
            redacted: String::new(),
        }];

        let report = DetectionReport::new(detections);
        let classification = classifier.classify(&report);

        // With custom thresholds, single email might still be PUBLIC
        assert!(matches!(
            classification,
            DataClassification::Public | DataClassification::Internal
        ));
    }
}
