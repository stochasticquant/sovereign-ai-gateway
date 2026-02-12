//! Healthcare PII patterns.
//!
//! Detects medical record numbers (MRN), ICD-10 diagnosis codes,
//! and common drug names using regex and keyword matching.

use crate::detector::PIIPattern;
use crate::report::PIIType;
use regex::Regex;

/// Return all healthcare PII detection patterns.
pub(crate) fn healthcare_patterns() -> Vec<PIIPattern> {
    vec![
        // Medical Record Number (MRN) — contextual prefix + 6-10 digits
        PIIPattern {
            pii_type: PIIType::MedicalRecord,
            regex: Regex::new(r"(?i)\b(?:MRN|medical\s+record)\s*[:#]?\s*\d{6,10}\b").unwrap(),
            confidence: 0.80,
        },
        // ICD-10 Diagnosis Codes — letter + 2 digits + optional decimal fraction
        // e.g., J18.9 (pneumonia), E11 (type 2 diabetes), I10 (hypertension)
        PIIPattern {
            pii_type: PIIType::DiagnosisCode,
            regex: Regex::new(r"\b[A-Z]\d{2}(?:\.\d{1,4})?\b").unwrap(),
            confidence: 0.90,
        },
        // Common drug names — case-insensitive keyword matching
        // Covers ~30 of the most commonly prescribed medications
        PIIPattern {
            pii_type: PIIType::DrugName,
            regex: Regex::new(
                r"(?i)\b(?:metformin|amoxicillin|lisinopril|atorvastatin|amlodipine|omeprazole|losartan|gabapentin|hydrochlorothiazide|sertraline|simvastatin|montelukast|escitalopram|levothyroxine|pantoprazole|rosuvastatin|acetaminophen|ibuprofen|prednisone|tramadol|furosemide|albuterol|insulin|warfarin|ciprofloxacin|azithromycin|fluoxetine|doxycycline|cephalexin|naproxen)\b",
            )
            .unwrap(),
            confidence: 0.70,
        },
    ]
}
