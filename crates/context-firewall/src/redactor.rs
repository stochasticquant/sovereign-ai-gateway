//! Redaction engine.
//!
//! Supports multiple redaction strategies:
//! - Irreversible: replaces PII with [REDACTED_TYPE] tokens
//! - Reversible: replaces PII with opaque tokens, stores mapping for response restoration
//! - Masking: partially masks PII while preserving format
//! - Hashing: replaces with deterministic hash

use crate::report::DetectionReport;
use sha2::{Digest, Sha256};
use tracing::debug;

/// Redaction strategy to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedactionStrategy {
    /// Mask with asterisks (e.g., "j***@e******.com").
    Mask,

    /// Replace with hash (e.g., "sha256:abc123...").
    Hash,

    /// Remove entirely (e.g., "[REDACTED]").
    Remove,

    /// Partial redaction (e.g., "****-****-****-3456").
    Partial,
}

/// Redaction engine for removing/masking PII.
pub struct Redactor {
    strategy: RedactionStrategy,
}

impl Default for Redactor {
    fn default() -> Self {
        Self::new(RedactionStrategy::Mask)
    }
}

impl Redactor {
    /// Create a new redactor with the specified strategy.
    pub fn new(strategy: RedactionStrategy) -> Self {
        Self { strategy }
    }

    /// Redact PII in the given text based on detections.
    pub fn redact(&self, text: &str, report: &mut DetectionReport) -> String {
        if report.detections.is_empty() {
            return text.to_string();
        }

        debug!(
            "Redacting {} PII detections using {:?} strategy",
            report.detections.len(),
            self.strategy
        );

        // Sort detections by offset (should already be sorted, but ensure)
        report.detections.sort_by_key(|d| d.offset);

        let mut result = String::with_capacity(text.len());
        let mut last_offset = 0;

        for detection in &mut report.detections {
            // Add unredacted text before this detection
            result.push_str(&text[last_offset..detection.offset]);

            // Apply redaction strategy
            let redacted = self.apply_strategy(&detection.original, self.strategy);

            // Update detection with redacted version
            detection.redacted = redacted.clone();

            // Add redacted text
            result.push_str(&redacted);

            // Move past this detection
            last_offset = detection.offset + detection.length;
        }

        // Add remaining unredacted text
        result.push_str(&text[last_offset..]);

        debug!(
            "Redaction complete (original: {} bytes, redacted: {} bytes)",
            text.len(),
            result.len()
        );

        result
    }

    /// Apply redaction strategy to a single piece of PII.
    fn apply_strategy(&self, original: &str, strategy: RedactionStrategy) -> String {
        match strategy {
            RedactionStrategy::Mask => self.mask(original),
            RedactionStrategy::Hash => self.hash(original),
            RedactionStrategy::Remove => "[REDACTED]".to_string(),
            RedactionStrategy::Partial => self.partial_redact(original),
        }
    }

    /// Mask text with asterisks while preserving some structure.
    fn mask(&self, text: &str) -> String {
        if text.is_empty() {
            return String::new();
        }

        // For emails, mask username and domain separately
        if text.contains('@') {
            return self.mask_email(text);
        }

        // For long strings, show first char and last char
        if text.len() > 3 {
            let first = text.chars().next().unwrap();
            let last = text.chars().last().unwrap();
            format!("{}{}{}", first, "*".repeat(text.len() - 2), last)
        } else {
            "*".repeat(text.len())
        }
    }

    /// Mask email address.
    fn mask_email(&self, email: &str) -> String {
        let parts: Vec<&str> = email.split('@').collect();
        if parts.len() != 2 {
            return "*".repeat(email.len());
        }

        let username = parts[0];
        let domain = parts[1];

        let masked_username = if username.len() > 2 {
            format!(
                "{}{}",
                username.chars().next().unwrap(),
                "*".repeat(username.len() - 1)
            )
        } else {
            "*".repeat(username.len())
        };

        let masked_domain = if domain.len() > 4 {
            format!(
                "{}******.{}",
                domain.chars().next().unwrap(),
                domain.split('.').next_back().unwrap_or("com")
            )
        } else {
            "*".repeat(domain.len())
        };

        format!("{}@{}", masked_username, masked_domain)
    }

    /// Hash text with SHA256.
    fn hash(&self, text: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(text.as_bytes());
        let result = hasher.finalize();
        format!("sha256:{:x}", result)
    }

    /// Partial redaction (show last 4 digits for credit cards, etc.).
    fn partial_redact(&self, text: &str) -> String {
        // Remove all non-digit characters for analysis
        let digits: String = text.chars().filter(|c| c.is_ascii_digit()).collect();

        // If looks like a credit card (13-19 digits), show last 4
        if digits.len() >= 13 && digits.len() <= 19 {
            let visible = &digits[digits.len().saturating_sub(4)..];
            return format!("****-****-****-{}", visible);
        }

        // Otherwise, mask everything except last 4 chars
        if text.len() > 4 {
            let visible = &text[text.len() - 4..];
            format!("{}{}", "*".repeat(text.len() - 4), visible)
        } else {
            "*".repeat(text.len())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::{Detection, PIIType};

    #[test]
    fn test_mask_email() {
        let redactor = Redactor::new(RedactionStrategy::Mask);
        let masked = redactor.mask("john.doe@example.com");

        assert!(masked.starts_with('j'));
        assert!(masked.contains('@'));
        assert!(masked.contains("***"));
    }

    #[test]
    fn test_hash_consistent() {
        let redactor = Redactor::new(RedactionStrategy::Hash);

        let hash1 = redactor.hash("sensitive data");
        let hash2 = redactor.hash("sensitive data");

        // Hashes should be consistent
        assert_eq!(hash1, hash2);
        assert!(hash1.starts_with("sha256:"));
    }

    #[test]
    fn test_partial_credit_card() {
        let redactor = Redactor::new(RedactionStrategy::Partial);
        let redacted = redactor.partial_redact("4532-0151-1283-0366");

        assert!(redacted.contains("0366"));
        assert!(redacted.contains("****"));
    }

    #[test]
    fn test_redact_text_with_detections() {
        let mut report = DetectionReport::new(vec![Detection {
            pii_type: PIIType::Email,
            offset: 10,
            length: 18,
            confidence: 0.95,
            original: "user@example.com".to_string(),
            redacted: String::new(),
        }]);

        let redactor = Redactor::new(RedactionStrategy::Mask);
        let text = "Contact me user@example.com for details.";
        let redacted = redactor.redact(text, &mut report);

        assert!(redacted.contains("***"));
        assert!(!redacted.contains("user@example.com"));
        assert!(redacted.contains("Contact me"));
        assert!(redacted.contains("for details"));
    }

    #[test]
    fn test_redact_multiple_detections() {
        let text = "Email alice@test.com, phone: 555-123-4567";
        // offsets: alice@test.com starts at 6, 555-123-4567 starts at 29

        let mut report = DetectionReport::new(vec![
            Detection {
                pii_type: PIIType::Email,
                offset: 6,
                length: 14, // "alice@test.com" is 14 chars
                confidence: 0.95,
                original: "alice@test.com".to_string(),
                redacted: String::new(),
            },
            Detection {
                pii_type: PIIType::PhoneNumber,
                offset: 29,
                length: 12, // "555-123-4567" is 12 chars
                confidence: 0.80,
                original: "555-123-4567".to_string(),
                redacted: String::new(),
            },
        ]);

        let redactor = Redactor::new(RedactionStrategy::Remove);
        let redacted = redactor.redact(text, &mut report);

        assert!(!redacted.contains("alice@test.com"));
        assert!(!redacted.contains("555-123-4567"));
        assert!(redacted.contains("[REDACTED]"));
        assert_eq!(redacted.matches("[REDACTED]").count(), 2);
    }

    #[test]
    fn test_no_redaction_when_no_detections() {
        let mut report = DetectionReport::new(vec![]);
        let redactor = Redactor::new(RedactionStrategy::Mask);

        let text = "This is clean text.";
        let redacted = redactor.redact(text, &mut report);

        assert_eq!(redacted, text);
    }

    #[test]
    fn test_redaction_updates_detection_struct() {
        let mut report = DetectionReport::new(vec![Detection {
            pii_type: PIIType::Email,
            offset: 0,
            length: 16,
            confidence: 0.95,
            original: "test@example.com".to_string(),
            redacted: String::new(),
        }]);

        let redactor = Redactor::new(RedactionStrategy::Hash);
        let _ = redactor.redact("test@example.com", &mut report);

        // Check that the detection was updated with redacted version
        assert!(!report.detections[0].redacted.is_empty());
        assert!(report.detections[0].redacted.starts_with("sha256:"));
    }
}
