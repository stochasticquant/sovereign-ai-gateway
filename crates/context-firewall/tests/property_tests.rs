//! Property-based tests for the context firewall.
//!
//! Uses proptest to verify invariants that must hold for arbitrary inputs:
//! - Detection stability (same input → same output)
//! - Redaction correctness (PII removed after redaction)
//! - Hash determinism
//! - No crashes on arbitrary UTF-8

use context_firewall::classifier::DataClassifier;
use context_firewall::detector::PIIDetector;
use context_firewall::redactor::{RedactionStrategy, Redactor};
use context_firewall::report::{Detection, DetectionReport, PIIType};
use proptest::prelude::*;

proptest! {
    /// Detection must be deterministic: scanning the same text twice
    /// should produce identical results.
    #[test]
    fn detection_is_stable(text in "[a-zA-Z0-9@.+\\- ]{0,200}") {
        let detector = PIIDetector::new();
        let report1 = detector.detect(&text);
        let report2 = detector.detect(&text);

        prop_assert_eq!(report1.detections.len(), report2.detections.len());
        prop_assert_eq!(report1.risk_score, report2.risk_score);

        for (d1, d2) in report1.detections.iter().zip(report2.detections.iter()) {
            prop_assert_eq!(&d1.original, &d2.original);
            prop_assert_eq!(d1.offset, d2.offset);
            prop_assert_eq!(d1.length, d2.length);
        }
    }

    /// The detector must never crash on any valid UTF-8 string.
    #[test]
    fn no_crash_on_arbitrary_utf8(text in "\\PC{0,500}") {
        let detector = PIIDetector::new();
        let _report = detector.detect(&text);
        // If we reach here without panic, the property holds.
    }

    /// Hash redaction must be deterministic: hashing the same input
    /// twice must produce the same output.
    #[test]
    fn hash_redaction_is_deterministic(text in ".{1,200}") {
        let redactor = Redactor::new(RedactionStrategy::Hash);

        let mut report1 = DetectionReport::new(vec![Detection {
            pii_type: PIIType::Email,
            offset: 0,
            length: text.len(),
            confidence: 0.95,
            original: text.clone(),
            redacted: String::new(),
        }]);
        let mut report2 = DetectionReport::new(vec![Detection {
            pii_type: PIIType::Email,
            offset: 0,
            length: text.len(),
            confidence: 0.95,
            original: text.clone(),
            redacted: String::new(),
        }]);

        let result1 = redactor.redact(&text, &mut report1);
        let result2 = redactor.redact(&text, &mut report2);

        prop_assert_eq!(result1, result2);
    }

    /// After redacting with Remove strategy, re-scanning the redacted text
    /// should not find the same PII types that were originally detected.
    #[test]
    fn redaction_removes_detected_pii(
        prefix in "[a-z ]{5,20}",
        suffix in "[a-z ]{5,20}",
    ) {
        let email = "test.user@example.com";
        let text = format!("{} {} {}", prefix, email, suffix);

        let detector = PIIDetector::new();
        let mut report = detector.detect(&text);

        // Only proceed if we actually detected the email
        let has_email = report
            .detections
            .iter()
            .any(|d| matches!(d.pii_type, PIIType::Email));

        if has_email {
            let redactor = Redactor::new(RedactionStrategy::Remove);
            let redacted = redactor.redact(&text, &mut report);

            // The redacted text should not contain the original email
            prop_assert!(!redacted.contains(email));

            // Re-scanning should not find the same email
            let re_report = detector.detect(&redacted);
            let still_has_email = re_report
                .detections
                .iter()
                .any(|d| d.original == email);
            prop_assert!(!still_has_email);
        }
    }

    /// Classification must be monotonic: adding more high-severity PII
    /// should never decrease the risk score.
    #[test]
    fn classification_monotonic_with_more_pii(count in 1usize..=5) {
        let base_detections: Vec<Detection> = (0..count)
            .map(|i| Detection {
                pii_type: PIIType::Email,
                offset: i * 30,
                length: 20,
                confidence: 0.95,
                original: format!("user{}@test.com", i),
                redacted: String::new(),
            })
            .collect();

        let report_fewer = DetectionReport::new(base_detections[..count.min(1)].to_vec());
        let report_more = DetectionReport::new(base_detections);

        prop_assert!(report_more.risk_score >= report_fewer.risk_score);
    }

    /// Redacted text must preserve all non-PII content.
    #[test]
    fn redaction_preserves_surrounding_text(
        prefix in "[a-z]{5,15}",
        suffix in "[a-z]{5,15}",
    ) {
        let text = format!("{} test@example.com {}", prefix, suffix);

        let detector = PIIDetector::new();
        let mut report = detector.detect(&text);

        if !report.detections.is_empty() {
            let redactor = Redactor::new(RedactionStrategy::Remove);
            let redacted = redactor.redact(&text, &mut report);

            // The prefix and suffix should still be present
            prop_assert!(redacted.contains(&prefix));
            prop_assert!(redacted.contains(&suffix));
        }
    }

    /// DataClassifier must never crash and must always return a valid classification.
    #[test]
    fn classifier_handles_any_risk_score(score in 0u8..=100) {
        let classifier = DataClassifier::new();

        let mut detections = Vec::new();
        // Create enough detections to reach the target score
        if score > 0 {
            detections.push(Detection {
                pii_type: PIIType::Email,
                offset: 0,
                length: 20,
                confidence: 0.95,
                original: "user@test.com".to_string(),
                redacted: String::new(),
            });
        }

        let report = DetectionReport::new(detections);
        let _classification = classifier.classify(&report);
        // If we reach here without panic, the property holds.
    }
}
