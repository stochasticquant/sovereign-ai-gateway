# PII Pattern Catalog

Complete reference of all PII types detected by the context firewall, including regex patterns, confidence scores, validation rules, and severity ratings.

## Overview

The PII detector uses compiled regex patterns with optional post-match validation (Luhn algorithm, date parsing, fake-sequence rejection). Patterns are organized into four modules:

| Module       | Patterns | Types Detected                                           |
|--------------|----------|----------------------------------------------------------|
| Core         | 6        | Email, Phone, Credit Card, SSN, IP Address, API Key      |
| Africa       | 4        | SA ID, Nigerian BVN, Kenyan National ID, Mobile Money    |
| Healthcare   | 3        | Medical Record Number, ICD-10 Code, Drug Name            |
| Financial    | 2        | IBAN, SWIFT/BIC Code                                     |

## Severity Scale

Each PII type has a severity score (0-10) that determines data classification:

| Severity | Classification | PII Types at This Level                                      |
|----------|----------------|--------------------------------------------------------------|
| 10       | Restricted     | National IDs (SA, Nigerian, Kenyan), Medical Record, Diagnosis Code |
| 9        | Restricted     | Credit Card, Bank Account (IBAN/SWIFT), SSN                  |
| 8        | Confidential   | API Key                                                      |
| 6        | Confidential   | Phone Number, Mobile Money                                   |
| 5        | Internal       | Email                                                        |
| 4        | Internal       | IP Address                                                   |
| 3        | Internal       | Drug Name                                                    |

## Risk Score Calculation

The detection report includes a risk score (0-100) computed as:

```
risk = (average_severity * 5) + (min(detection_count, 10) * 5)
```

- Average severity contributes up to 50 points
- Detection count contributes up to 50 points (capped at 10 detections)

---

## Core Patterns

### Email Address

| Field      | Value |
|------------|-------|
| Type       | `email` |
| Severity   | 5 |
| Confidence | 0.95 |
| Regex      | `\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z\|a-z]{2,}\b` |
| Validation | None (regex is sufficient) |
| Example    | `john.doe@example.com` |

### Phone Number

| Field      | Value |
|------------|-------|
| Type       | `phone_number` |
| Severity   | 6 |
| Confidence | 0.80 |
| Regex      | `\+?\d{1,4}[\s\-]?\(?\d{1,4}\)?[\s\-]?\d{1,4}[\s\-]?\d{1,9}` |
| Validation | None |
| Example    | `+1-555-123-4567` |

### Credit Card

| Field      | Value |
|------------|-------|
| Type       | `credit_card` |
| Severity   | 9 |
| Confidence | 0.70 initial, **0.95 after Luhn validation** |
| Regex      | `\b\d{4}[\s\-]?\d{4}[\s\-]?\d{4}[\s\-]?\d{4}\b` |
| Validation | Luhn algorithm checksum; digit count must be 13-19 |
| Example    | `4532-0151-1283-0366` |

### SSN (US Social Security Number)

| Field      | Value |
|------------|-------|
| Type       | `ssn` |
| Severity   | 9 |
| Confidence | 0.90 |
| Regex      | `\b\d{3}-\d{2}-\d{4}\b` |
| Validation | Rejects known fakes: `000-00-0000`, `111-11-1111`, `123-45-6789`, `999-99-9999` |
| Example    | `456-78-9012` |

### IP Address (IPv4)

| Field      | Value |
|------------|-------|
| Type       | `ip_address` |
| Severity   | 4 |
| Confidence | 0.85 |
| Regex      | `\b(?:\d{1,3}\.){3}\d{1,3}\b` |
| Validation | Each octet must parse as a valid `u8` (0-255) |
| Example    | `192.168.1.1` |

### API Key / Token

| Field      | Value |
|------------|-------|
| Type       | `api_key` |
| Severity   | 8 |
| Confidence | 0.50 |
| Regex      | `\b[A-Za-z0-9]{32,}\b` |
| Validation | None (low confidence due to high false positive rate) |
| Example    | `a]b2c3d4e5f6g7h8i9j0k1l2m3n4o5p6q7r8` |

---

## Africa-Specific Patterns

### South African ID Number

| Field      | Value |
|------------|-------|
| Type       | `south_african_id` |
| Severity   | 10 |
| Confidence | 0.90 |
| Regex      | `\b\d{13}\b` |
| Validation | First 6 digits must form valid date (YYMMDD: MM 01-12, DD 01-31); full 13 digits must pass Luhn checksum |
| Format     | `YYMMDD SSSS C A Z` — date of birth + sequence + citizenship + gender + checksum |
| Example    | `8801015009080` |

### Nigerian Bank Verification Number (BVN)

| Field      | Value |
|------------|-------|
| Type       | `nigerian_bvn` |
| Severity   | 10 |
| Confidence | 0.85 |
| Regex      | `\b\d{11}\b` |
| Validation | Exactly 11 digits (enforced by regex word boundaries) |
| Example    | `22234567891` |

### Kenyan National ID

| Field      | Value |
|------------|-------|
| Type       | `kenyan_national_id` |
| Severity   | 10 |
| Confidence | 0.75 |
| Regex      | `\b\d{7,8}\b` |
| Validation | Rejects fake sequences — all same digit (`1111111`) or sequential ascending (`1234567`) |
| Note       | Lower confidence due to short digit length increasing false positive risk. Older IDs are 7 digits, newer IDs are 8 digits. |
| Example    | `31245678` |

### Mobile Money Number

| Field      | Value |
|------------|-------|
| Type       | `mobile_money` |
| Severity   | 6 |
| Confidence | 0.85 |
| Regex      | `\+(?:254\|255\|256\|233\|234\|27)\d{9,10}\b` |
| Validation | None (prefix matching is sufficient) |
| Coverage   | Kenya (+254), Tanzania (+255), Uganda (+256), Ghana (+233), Nigeria (+234), South Africa (+27) |
| Example    | `+254712345678` (M-Pesa), `+2348012345678` (MTN) |

---

## Healthcare Patterns

### Medical Record Number (MRN)

| Field      | Value |
|------------|-------|
| Type       | `medical_record` |
| Severity   | 10 |
| Confidence | 0.80 |
| Regex      | `(?i)\b(?:MRN\|medical\s+record)\s*[:#]?\s*\d{6,10}\b` |
| Validation | Requires contextual prefix ("MRN" or "medical record") followed by 6-10 digits |
| Example    | `MRN: 12345678`, `Medical Record #987654` |

### ICD-10 Diagnosis Code

| Field      | Value |
|------------|-------|
| Type       | `diagnosis_code` |
| Severity   | 10 |
| Confidence | 0.90 |
| Regex      | `\b[A-Z]\d{2}(?:\.\d{1,4})?\b` |
| Validation | None (format is self-validating) |
| Format     | Letter + 2 digits + optional decimal + 1-4 digits |
| Example    | `J18.9` (pneumonia), `E11` (type 2 diabetes), `I10` (hypertension) |

### Drug Name

| Field      | Value |
|------------|-------|
| Type       | `drug_name` |
| Severity   | 3 |
| Confidence | 0.70 |
| Method     | Case-insensitive keyword matching against curated list |
| Validation | Exact word boundary matching (no partial matches) |
| Drug List  | metformin, amoxicillin, lisinopril, atorvastatin, amlodipine, omeprazole, losartan, gabapentin, hydrochlorothiazide, sertraline, simvastatin, montelukast, escitalopram, levothyroxine, pantoprazole, rosuvastatin, acetaminophen, ibuprofen, prednisone, tramadol, furosemide, albuterol, insulin, warfarin, ciprofloxacin, azithromycin, fluoxetine, doxycycline, cephalexin, naproxen |

---

## Financial Patterns

### IBAN (International Bank Account Number)

| Field      | Value |
|------------|-------|
| Type       | `bank_account` |
| Severity   | 9 |
| Confidence | 0.90 |
| Regex      | `\b[A-Z]{2}\d{2}\s?[\dA-Z]{4}\s?(?:[\dA-Z]{4}\s?){2,7}[\dA-Z]{1,4}\b` |
| Validation | None (regex validates structure) |
| Format     | 2-letter country code + 2 check digits + up to 30 alphanumeric characters |
| Example    | `GB29 NWBK 6016 1331 9268 19` |

### SWIFT/BIC Code

| Field      | Value |
|------------|-------|
| Type       | `bank_account` |
| Severity   | 9 |
| Confidence | 0.85 |
| Regex      | `\b[A-Z]{4}[A-Z]{2}[A-Z0-9]{2}(?:[A-Z0-9]{3})?\b` |
| Validation | None (regex validates structure) |
| Format     | 4 bank code + 2 country code + 2 location code + optional 3 branch code (8 or 11 chars) |
| Example    | `DEUTDEFF` (Deutsche Bank), `BNPAFRPPXXX` (BNP Paribas) |

---

## Redaction Strategies

When PII is detected, three redaction strategies are available:

| Strategy  | Behavior                                | Reversible |
|-----------|-----------------------------------------|------------|
| `Remove`  | Replaces PII with `[REDACTED]` marker   | No         |
| `Hash`    | Replaces PII with deterministic SHA-256 hash | Yes (with key) |
| `Mask`    | Replaces characters with `*` while preserving length | No |

## Property Test Invariants

The following invariants are verified with property-based testing (proptest):

1. **Detection stability** — scanning the same text twice produces identical results
2. **No crash on arbitrary UTF-8** — the detector handles any valid UTF-8 without panic
3. **Hash determinism** — `hash(x) == hash(x)` for all inputs
4. **Redaction completeness** — re-scanning redacted text finds no previously-detected PII
5. **Classification monotonicity** — adding more PII detections never lowers the risk score
6. **Content preservation** — text surrounding PII is unchanged after redaction
