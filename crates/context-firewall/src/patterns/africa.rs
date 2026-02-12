//! Africa-specific PII patterns.
//!
//! Detects South African ID numbers, Nigerian BVN, Kenyan National ID,
//! and mobile money numbers (M-Pesa, MTN, Airtel) across the continent.

use crate::detector::PIIPattern;
use crate::report::PIIType;
use regex::Regex;

/// Return all Africa-specific PII detection patterns.
pub(crate) fn africa_patterns() -> Vec<PIIPattern> {
    vec![
        // South African ID Number — 13 consecutive digits
        // Format: YYMMDD SSSS C A Z (date, sequence, citizenship, gender, checksum)
        // Validation: date check + Luhn done in detector.rs
        PIIPattern {
            pii_type: PIIType::SouthAfricanId,
            regex: Regex::new(r"\b\d{13}\b").unwrap(),
            confidence: 0.90,
        },
        // Nigerian Bank Verification Number (BVN) — exactly 11 digits
        PIIPattern {
            pii_type: PIIType::NigerianBVN,
            regex: Regex::new(r"\b\d{11}\b").unwrap(),
            confidence: 0.85,
        },
        // Kenyan National ID — 7 or 8 digits
        // Older IDs are 7 digits, newer ones are 8
        // Validation: reject fake sequences done in detector.rs
        PIIPattern {
            pii_type: PIIType::KenyanNationalId,
            regex: Regex::new(r"\b\d{7,8}\b").unwrap(),
            confidence: 0.75,
        },
        // Mobile Money numbers — African country code prefixes
        // Kenya (+254), Tanzania (+255), Uganda (+256),
        // Ghana (+233), Nigeria (+234), South Africa (+27)
        PIIPattern {
            pii_type: PIIType::MobileMoney,
            regex: Regex::new(r"\+(?:254|255|256|233|234|27)\d{9,10}\b").unwrap(),
            confidence: 0.85,
        },
    ]
}
