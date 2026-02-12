//! Financial PII patterns.
//!
//! Detects IBAN numbers and SWIFT/BIC codes.

use crate::detector::PIIPattern;
use crate::report::PIIType;
use regex::Regex;

/// Return all financial PII detection patterns.
pub(crate) fn financial_patterns() -> Vec<PIIPattern> {
    vec![
        // IBAN — 2 letter country code + 2 check digits + up to 30 alphanumeric chars
        // e.g., GB29 NWBK 6016 1331 9268 19
        PIIPattern {
            pii_type: PIIType::BankAccount,
            regex: Regex::new(
                r"\b[A-Z]{2}\d{2}\s?[\dA-Z]{4}\s?(?:[\dA-Z]{4}\s?){2,7}[\dA-Z]{1,4}\b",
            )
            .unwrap(),
            confidence: 0.90,
        },
        // SWIFT/BIC Code — 8 or 11 characters (4 bank + 2 country + 2 location + optional 3 branch)
        // e.g., DEUTDEFF, BNPAFRPPXXX
        PIIPattern {
            pii_type: PIIType::BankAccount,
            regex: Regex::new(r"\b[A-Z]{4}[A-Z]{2}[A-Z0-9]{2}(?:[A-Z0-9]{3})?\b").unwrap(),
            confidence: 0.85,
        },
    ]
}
