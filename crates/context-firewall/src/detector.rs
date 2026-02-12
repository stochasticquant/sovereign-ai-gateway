//! Multi-pattern PII detector.
//!
//! Uses Aho-Corasick automaton for keyword matching and compiled regex for pattern matching.
//! Scans text and returns all PII matches with their types and positions.

use crate::report::{Detection, DetectionReport, PIIType};
use regex::Regex;
use tracing::{debug, trace};

/// PII detector that scans text for sensitive information.
pub struct PIIDetector {
    patterns: Vec<PIIPattern>,
}

/// A single PII pattern with its regex and metadata.
struct PIIPattern {
    pii_type: PIIType,
    regex: Regex,
    confidence: f64,
}

impl Default for PIIDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl PIIDetector {
    /// Create a new PII detector with all default patterns.
    pub fn new() -> Self {
        let patterns = vec![
            // Email addresses
            PIIPattern {
                pii_type: PIIType::Email,
                regex: Regex::new(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}\b").unwrap(),
                confidence: 0.95,
            },
            // Phone numbers (international format)
            PIIPattern {
                pii_type: PIIType::PhoneNumber,
                regex: Regex::new(r"\+?\d{1,4}[\s\-]?\(?\d{1,4}\)?[\s\-]?\d{1,4}[\s\-]?\d{1,9}")
                    .unwrap(),
                confidence: 0.80,
            },
            // Credit card numbers (basic pattern, will validate with Luhn)
            PIIPattern {
                pii_type: PIIType::CreditCard,
                regex: Regex::new(r"\b\d{4}[\s\-]?\d{4}[\s\-]?\d{4}[\s\-]?\d{4}\b").unwrap(),
                confidence: 0.70, // Lower confidence, needs Luhn validation
            },
            // SSN (US format)
            PIIPattern {
                pii_type: PIIType::SSN,
                regex: Regex::new(r"\b\d{3}-\d{2}-\d{4}\b").unwrap(),
                confidence: 0.90,
            },
            // IPv4 addresses
            PIIPattern {
                pii_type: PIIType::IPAddress,
                regex: Regex::new(r"\b(?:\d{1,3}\.){3}\d{1,3}\b").unwrap(),
                confidence: 0.85,
            },
            // API keys and tokens (generic pattern)
            PIIPattern {
                pii_type: PIIType::APIKey,
                regex: Regex::new(r"\b[A-Za-z0-9]{32,}\b").unwrap(),
                confidence: 0.50, // Low confidence, many false positives
            },
        ];

        Self { patterns }
    }

    /// Detect PII in the given text.
    pub fn detect(&self, text: &str) -> DetectionReport {
        debug!("Scanning text for PII ({} bytes)", text.len());

        let mut detections = Vec::new();

        for pattern in &self.patterns {
            trace!("Scanning for {:?}", pattern.pii_type);

            for mat in pattern.regex.find_iter(text) {
                let matched_text = mat.as_str();
                let offset = mat.start();

                // Additional validation for specific types
                let (is_valid, adjusted_confidence) = match pattern.pii_type {
                    PIIType::CreditCard => {
                        let valid = self.validate_credit_card(matched_text);
                        (valid, if valid { 0.95 } else { 0.0 })
                    }
                    PIIType::IPAddress => {
                        let valid = self.validate_ipv4(matched_text);
                        (valid, if valid { pattern.confidence } else { 0.0 })
                    }
                    PIIType::SSN => {
                        let valid = !is_obviously_fake_ssn(matched_text);
                        (valid, if valid { pattern.confidence } else { 0.0 })
                    }
                    _ => (true, pattern.confidence),
                };

                if !is_valid || adjusted_confidence == 0.0 {
                    continue; // Skip invalid matches
                }

                detections.push(Detection {
                    pii_type: pattern.pii_type.clone(),
                    offset,
                    length: matched_text.len(),
                    confidence: adjusted_confidence,
                    original: matched_text.to_string(),
                    redacted: String::new(), // Will be filled by redactor
                });
            }
        }

        // Sort detections by offset
        detections.sort_by_key(|d| d.offset);

        debug!("Found {} PII detections", detections.len());

        DetectionReport::new(detections)
    }

    /// Validate credit card number using Luhn algorithm.
    fn validate_credit_card(&self, number: &str) -> bool {
        let digits: String = number.chars().filter(|c| c.is_ascii_digit()).collect();

        if digits.len() < 13 || digits.len() > 19 {
            return false;
        }

        luhn_check(&digits)
    }

    /// Validate IPv4 address.
    fn validate_ipv4(&self, ip: &str) -> bool {
        let parts: Vec<&str> = ip.split('.').collect();

        if parts.len() != 4 {
            return false;
        }

        parts
            .iter()
            .all(|part| part.parse::<u8>().is_ok())
    }
}

/// Luhn algorithm for credit card validation.
fn luhn_check(number: &str) -> bool {
    let mut sum = 0;
    let mut double = false;

    for digit in number.chars().rev() {
        if let Some(d) = digit.to_digit(10) {
            let mut d = d as u32;

            if double {
                d *= 2;
                if d > 9 {
                    d -= 9;
                }
            }

            sum += d;
            double = !double;
        } else {
            return false;
        }
    }

    sum % 10 == 0
}

/// Check if SSN is obviously fake (e.g., "000-00-0000", "123-45-6789").
fn is_obviously_fake_ssn(ssn: &str) -> bool {
    let fake_patterns = [
        "000-00-0000",
        "111-11-1111",
        "123-45-6789",
        "999-99-9999",
    ];

    fake_patterns.contains(&ssn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_email() {
        let detector = PIIDetector::new();
        let text = "Contact me at john.doe@example.com for more info.";

        let report = detector.detect(text);

        assert_eq!(report.detections.len(), 1);
        assert!(matches!(report.detections[0].pii_type, PIIType::Email));
        assert_eq!(report.detections[0].original, "john.doe@example.com");
    }

    #[test]
    fn test_detect_multiple_pii() {
        let detector = PIIDetector::new();
        let text = "Email: user@test.com, Phone: +1-555-123-4567";

        let report = detector.detect(text);

        assert!(report.detections.len() >= 2);
        assert!(report.detections.iter().any(|d| matches!(d.pii_type, PIIType::Email)));
        assert!(report
            .detections
            .iter()
            .any(|d| matches!(d.pii_type, PIIType::PhoneNumber)));
    }

    #[test]
    fn test_validate_credit_card_valid() {
        let detector = PIIDetector::new();
        // Valid test credit card number (Visa)
        assert!(detector.validate_credit_card("4532015112830366"));
    }

    #[test]
    fn test_validate_credit_card_invalid() {
        let detector = PIIDetector::new();
        assert!(!detector.validate_credit_card("1234567890123456"));
    }

    #[test]
    fn test_detect_credit_card() {
        let detector = PIIDetector::new();
        // Valid Visa test number
        let text = "Card: 4532-0151-1283-0366";

        let report = detector.detect(text);

        // Should find and validate the credit card
        let cc_detections: Vec<_> = report
            .detections
            .iter()
            .filter(|d| matches!(d.pii_type, PIIType::CreditCard))
            .collect();

        assert!(!cc_detections.is_empty());
    }

    #[test]
    fn test_reject_invalid_credit_card() {
        let detector = PIIDetector::new();
        let text = "Card: 1234-5678-9012-3456"; // Invalid by Luhn

        let report = detector.detect(text);

        // Should NOT detect as credit card (fails Luhn check)
        let cc_detections: Vec<_> = report
            .detections
            .iter()
            .filter(|d| matches!(d.pii_type, PIIType::CreditCard))
            .collect();

        assert!(cc_detections.is_empty());
    }

    #[test]
    fn test_detect_ipv4() {
        let detector = PIIDetector::new();
        let text = "Server IP: 192.168.1.1";

        let report = detector.detect(text);

        let ip_detections: Vec<_> = report
            .detections
            .iter()
            .filter(|d| matches!(d.pii_type, PIIType::IPAddress))
            .collect();

        assert!(!ip_detections.is_empty());
        assert_eq!(ip_detections[0].original, "192.168.1.1");
    }

    #[test]
    fn test_reject_obviously_fake_ssn() {
        let detector = PIIDetector::new();
        let text = "SSN: 000-00-0000 or 123-45-6789";

        let report = detector.detect(text);

        // Should NOT detect these as valid SSNs
        let ssn_detections: Vec<_> = report
            .detections
            .iter()
            .filter(|d| matches!(d.pii_type, PIIType::SSN))
            .collect();

        assert!(ssn_detections.is_empty());
    }

    #[test]
    fn test_no_pii_in_clean_text() {
        let detector = PIIDetector::new();
        let text = "This is a completely clean text with no sensitive information.";

        let report = detector.detect(text);

        assert!(report.detections.is_empty());
    }
}
