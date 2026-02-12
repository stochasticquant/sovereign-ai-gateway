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
pub(crate) struct PIIPattern {
    pub(crate) pii_type: PIIType,
    pub(crate) regex: Regex,
    pub(crate) confidence: f64,
}

impl Default for PIIDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl PIIDetector {
    /// Create a new PII detector with all default patterns.
    pub fn new() -> Self {
        let mut patterns = vec![
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

        // Add domain-specific patterns
        patterns.extend(crate::patterns::africa::africa_patterns());
        patterns.extend(crate::patterns::healthcare::healthcare_patterns());
        patterns.extend(crate::patterns::financial::financial_patterns());

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
                    PIIType::SouthAfricanId => {
                        let valid = validate_south_african_id(matched_text);
                        (valid, if valid { 0.90 } else { 0.0 })
                    }
                    PIIType::KenyanNationalId => {
                        let valid = !is_fake_digit_sequence(matched_text);
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

        parts.iter().all(|part| part.parse::<u8>().is_ok())
    }
}

/// Luhn algorithm for credit card and ID validation.
pub(crate) fn luhn_check(number: &str) -> bool {
    let mut sum = 0;
    let mut double = false;

    for digit in number.chars().rev() {
        if let Some(d) = digit.to_digit(10) {
            let mut d = d;

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
    let fake_patterns = ["000-00-0000", "111-11-1111", "123-45-6789", "999-99-9999"];

    fake_patterns.contains(&ssn)
}

/// Validate a South African ID number (13 digits with date and Luhn checksum).
fn validate_south_african_id(id: &str) -> bool {
    let digits: String = id.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() != 13 {
        return false;
    }

    // First 6 digits must be a valid date (YYMMDD)
    let yy: u32 = match digits[0..2].parse() {
        Ok(v) => v,
        Err(_) => return false,
    };
    let mm: u32 = match digits[2..4].parse() {
        Ok(v) => v,
        Err(_) => return false,
    };
    let dd: u32 = match digits[4..6].parse() {
        Ok(v) => v,
        Err(_) => return false,
    };

    // Basic date validation (YY is 00-99, MM is 01-12, DD is 01-31)
    let _ = yy; // Any year is valid
    if !(1..=12).contains(&mm) || !(1..=31).contains(&dd) {
        return false;
    }

    // Luhn check on full 13 digits
    luhn_check(&digits)
}

/// Check if a digit sequence is obviously fake (all same digit or sequential).
fn is_fake_digit_sequence(number: &str) -> bool {
    let digits: Vec<char> = number.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return true;
    }

    // All same digit
    if digits.iter().all(|&c| c == digits[0]) {
        return true;
    }

    // Sequential ascending (1234567, 12345678)
    let is_sequential = digits.windows(2).all(|w| {
        let a = w[0].to_digit(10).unwrap_or(0);
        let b = w[1].to_digit(10).unwrap_or(0);
        b == a + 1
    });
    if is_sequential {
        return true;
    }

    false
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
        assert!(
            report
                .detections
                .iter()
                .any(|d| matches!(d.pii_type, PIIType::Email))
        );
        assert!(
            report
                .detections
                .iter()
                .any(|d| matches!(d.pii_type, PIIType::PhoneNumber))
        );
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

    // --- Africa-specific pattern tests ---

    #[test]
    fn test_detect_south_african_id() {
        let detector = PIIDetector::new();
        // Valid SA ID: DOB=1988-01-01, Luhn-valid 13-digit number
        let text = "ID: 8801015009080";

        let report = detector.detect(text);

        let sa_id: Vec<_> = report
            .detections
            .iter()
            .filter(|d| matches!(d.pii_type, PIIType::SouthAfricanId))
            .collect();

        assert!(!sa_id.is_empty(), "Should detect SA ID");
        assert_eq!(sa_id[0].original, "8801015009080");
    }

    #[test]
    fn test_reject_invalid_south_african_id() {
        let detector = PIIDetector::new();
        // Invalid: month 13 doesn't exist
        let text = "ID: 8813015009087";

        let report = detector.detect(text);

        let sa_id: Vec<_> = report
            .detections
            .iter()
            .filter(|d| matches!(d.pii_type, PIIType::SouthAfricanId))
            .collect();

        assert!(sa_id.is_empty(), "Should reject invalid SA ID date");
    }

    #[test]
    fn test_detect_nigerian_bvn() {
        let detector = PIIDetector::new();
        let text = "BVN: 22234567891";

        let report = detector.detect(text);

        let bvn: Vec<_> = report
            .detections
            .iter()
            .filter(|d| matches!(d.pii_type, PIIType::NigerianBVN))
            .collect();

        assert!(!bvn.is_empty(), "Should detect Nigerian BVN");
    }

    #[test]
    fn test_detect_kenyan_national_id() {
        let detector = PIIDetector::new();
        let text = "National ID: 31245678";

        let report = detector.detect(text);

        let ke_id: Vec<_> = report
            .detections
            .iter()
            .filter(|d| matches!(d.pii_type, PIIType::KenyanNationalId))
            .collect();

        assert!(!ke_id.is_empty(), "Should detect Kenyan National ID");
    }

    #[test]
    fn test_reject_fake_kenyan_id() {
        let detector = PIIDetector::new();
        // All same digits should be rejected
        let text = "ID: 11111111";

        let report = detector.detect(text);

        let ke_id: Vec<_> = report
            .detections
            .iter()
            .filter(|d| matches!(d.pii_type, PIIType::KenyanNationalId))
            .collect();

        assert!(ke_id.is_empty(), "Should reject fake Kenyan ID");
    }

    #[test]
    fn test_detect_mobile_money() {
        let detector = PIIDetector::new();
        let text = "M-Pesa: +254712345678, MTN: +2348012345678";

        let report = detector.detect(text);

        let mm: Vec<_> = report
            .detections
            .iter()
            .filter(|d| matches!(d.pii_type, PIIType::MobileMoney))
            .collect();

        assert!(mm.len() >= 2, "Should detect M-Pesa and MTN numbers");
    }

    // --- Healthcare pattern tests ---

    #[test]
    fn test_detect_medical_record_number() {
        let detector = PIIDetector::new();
        let text = "Patient MRN: 12345678";

        let report = detector.detect(text);

        let mrn: Vec<_> = report
            .detections
            .iter()
            .filter(|d| matches!(d.pii_type, PIIType::MedicalRecord))
            .collect();

        assert!(!mrn.is_empty(), "Should detect MRN");
    }

    #[test]
    fn test_detect_icd10_code() {
        let detector = PIIDetector::new();
        let text = "Diagnosis: J18.9 (pneumonia), E11 (diabetes)";

        let report = detector.detect(text);

        let icd: Vec<_> = report
            .detections
            .iter()
            .filter(|d| matches!(d.pii_type, PIIType::DiagnosisCode))
            .collect();

        assert!(icd.len() >= 2, "Should detect ICD-10 codes");
    }

    #[test]
    fn test_detect_drug_name() {
        let detector = PIIDetector::new();
        let text = "The patient takes metformin and amoxicillin daily.";

        let report = detector.detect(text);

        let drugs: Vec<_> = report
            .detections
            .iter()
            .filter(|d| matches!(d.pii_type, PIIType::DrugName))
            .collect();

        assert!(drugs.len() >= 2, "Should detect drug names");
    }

    // --- Financial pattern tests ---

    #[test]
    fn test_detect_iban() {
        let detector = PIIDetector::new();
        let text = "Wire to IBAN: GB29 NWBK 6016 1331 9268 19";

        let report = detector.detect(text);

        let iban: Vec<_> = report
            .detections
            .iter()
            .filter(|d| matches!(d.pii_type, PIIType::BankAccount))
            .collect();

        assert!(!iban.is_empty(), "Should detect IBAN");
    }

    #[test]
    fn test_detect_swift_code() {
        let detector = PIIDetector::new();
        let text = "SWIFT code: DEUTDEFF";

        let report = detector.detect(text);

        let swift: Vec<_> = report
            .detections
            .iter()
            .filter(|d| matches!(d.pii_type, PIIType::BankAccount))
            .collect();

        assert!(!swift.is_empty(), "Should detect SWIFT/BIC code");
    }
}
