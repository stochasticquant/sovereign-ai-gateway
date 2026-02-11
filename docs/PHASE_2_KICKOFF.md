# Phase 2: Policy & Firewall Implementation - Kickoff

**Date Started**: 2026-02-11
**Branch**: `feature/phase-2-policy-firewall`
**Parent Commit**: `6c5f121` (Phase 1 baseline)
**Status**: 🚀 **IN PROGRESS**

---

## Overview

Phase 2 focuses on implementing the **Policy Engine** and **Context Firewall**, the core components that enable data sovereignty, PII protection, and policy-based routing for regulated industries across Africa.

---

## Objectives

### Primary Goals
1. ✅ **Policy Engine**: Evaluate TOML policies for tenant routing and access control
2. ✅ **Hot-Reload**: Watch policy files and reload without downtime
3. ✅ **PII Detection**: Pattern-based detection using regex + aho-corasick
4. ✅ **Data Classification**: Categorize data by sensitivity (PUBLIC, INTERNAL, CONFIDENTIAL, RESTRICTED)
5. ✅ **Redaction Engine**: Mask, hash, or remove sensitive data
6. ✅ **Property Testing**: Comprehensive property tests for firewall logic
7. ✅ **Integration Tests**: End-to-end policy and firewall scenarios

### Success Criteria
- [ ] Policy engine evaluates TOML files correctly
- [ ] Hot-reload detects file changes within 1 second
- [ ] PII detector achieves >95% accuracy on test dataset
- [ ] Redaction preserves data format (e.g., "123-45-6789" → "XXX-XX-6789")
- [ ] Property tests cover edge cases (empty strings, unicode, escape sequences)
- [ ] All tests pass: `cargo nextest run --workspace`
- [ ] Clippy passes: `cargo clippy --workspace -- -D warnings`
- [ ] Documentation updated with examples

---

## Implementation Plan

### 1. Policy Engine (`crates/policy-engine`)

#### 1.1 Schema Definition (`schema.rs`)
**Scope**: Define TOML policy schema structures

```rust
// Key structures to implement:
- Policy (tenant_id, routes, restrictions, quotas)
- RouteRule (provider, model, region, conditions)
- Restriction (allowed_models, blocked_regions, data_classification)
- Quota (max_tokens, max_requests, time_window)
```

**Tests**:
- TOML deserialization
- Schema validation (required fields, type checking)
- Invalid policy rejection

#### 1.2 Policy Loader (`loader.rs`)
**Scope**: Load and parse TOML files from `policies/` directory

```rust
// Key functions:
- load_policy(path: &Path) -> Result<Policy>
- load_all_policies(dir: &Path) -> Result<Vec<Policy>>
- validate_policy(policy: &Policy) -> Result<()>
- watch_directory(dir: &Path) -> Receiver<PolicyUpdate>
```

**Tests**:
- Load valid policies
- Reject malformed TOML
- Handle missing files gracefully
- Hot-reload on file modification

**Dependencies**: `notify` crate for file watching

#### 1.3 Policy Evaluator (`evaluator.rs`)
**Scope**: Evaluate policies against incoming requests

```rust
// Key functions:
- evaluate(request: &Request, policies: &[Policy]) -> Decision
- match_tenant(api_key: &str, policies: &[Policy]) -> Option<&Policy>
- check_restrictions(request: &Request, policy: &Policy) -> Result<()>
- select_route(request: &Request, policy: &Policy) -> RouteDecision
```

**Tests**:
- Tenant matching by API key
- Route selection based on model/region
- Restriction enforcement (blocked regions, model limits)
- Quota checking

#### 1.4 Decision Types (`decision.rs`)
**Scope**: Represent evaluation outcomes

```rust
// Key structures:
- Decision (allow/deny, selected_route, applied_policies)
- RouteDecision (provider, model, endpoint, headers)
- PolicyViolation (rule, reason, severity)
```

**Tests**:
- Decision serialization
- Violation reporting

#### 1.5 Tenant Management (`tenant.rs`)
**Scope**: Tenant context and isolation

```rust
// Key structures:
- TenantContext (id, policies, quotas, settings)
- TenantResolver (resolve tenant from API key)
```

**Tests**:
- Tenant isolation
- Cross-tenant policy enforcement

---

### 2. Context Firewall (`crates/context-firewall`)

#### 2.1 PII Detector (`detector.rs`)
**Scope**: Detect PII patterns in text

**Patterns to Implement**:
- Email addresses
- Phone numbers (international formats)
- Credit card numbers (Luhn validation)
- National IDs (African countries: SA, NG, KE, etc.)
- IP addresses
- URLs with auth tokens
- API keys and secrets

**Implementation**:
- Use `regex` for complex patterns
- Use `aho-corasick` for keyword scanning
- Implement confidence scoring (0.0 - 1.0)

**Tests**:
- Detect all PII types
- Avoid false positives (e.g., "000-00-0000" is not valid SSN)
- Handle unicode and RTL text
- Property tests for edge cases

#### 2.2 Data Classifier (`classifier.rs`)
**Scope**: Classify data sensitivity

**Classification Levels**:
```rust
enum DataClassification {
    Public,        // No restrictions
    Internal,      // Internal use only
    Confidential,  // Sensitive business data
    Restricted,    // PII, PHI, financial data
}
```

**Rules**:
- Presence of PII → RESTRICTED
- Business-specific keywords → CONFIDENTIAL
- Default → INTERNAL

**Tests**:
- Classify text with PII
- Classify mixed content
- Validate classification rules

#### 2.3 Redactor (`redactor.rs`)
**Scope**: Redact or mask sensitive data

**Redaction Strategies**:
```rust
enum RedactionStrategy {
    Mask,        // "john@example.com" → "j***@e******.com"
    Hash,        // "john@example.com" → "sha256:abc123..."
    Remove,      // "john@example.com" → "[REDACTED]"
    Partial,     // "1234-5678-9012-3456" → "****-****-****-3456"
}
```

**Tests**:
- Preserve format (length, structure)
- Hash consistency (same input → same hash)
- Validate redacted output is safe
- Property tests for all strategies

#### 2.4 Pattern Libraries (`patterns/`)

##### `general.rs` - General PII Patterns
- Emails, phones, credit cards, SSNs, IP addresses

##### `healthcare.rs` - Healthcare-Specific
- Medical record numbers (MRN)
- Health insurance numbers
- Prescription numbers
- Patient IDs

##### `financial.rs` - Financial Data
- IBAN, SWIFT codes
- Bank account numbers
- Credit card CVV/CVC
- Cryptocurrency addresses

##### `africa.rs` - African Region-Specific
- South African ID numbers (13 digits with Luhn)
- Nigerian BVN (Bank Verification Number)
- Kenyan National ID
- Ghana Card numbers
- Mobile money numbers (M-Pesa, MTN, Airtel)

**Tests**: Each pattern module has unit tests

#### 2.5 Detection Report (`report.rs`)
**Scope**: Structured output of PII detections

```rust
struct DetectionReport {
    detections: Vec<Detection>,
    classification: DataClassification,
    confidence: f64,
    redacted_text: Option<String>,
}

struct Detection {
    pii_type: PIIType,
    offset: usize,
    length: usize,
    confidence: f64,
    original: String,
    redacted: String,
}
```

**Tests**:
- Report serialization (JSON)
- Report aggregation
- Confidence calculation

---

### 3. Integration & Testing

#### 3.1 Unit Tests
- Each module has focused unit tests
- Coverage target: >80% for policy-engine and context-firewall

#### 3.2 Property Tests
**Location**: `crates/context-firewall/tests/property_tests.rs`

**Properties to Test**:
- **Redaction preserves length** (for mask strategy)
- **Redaction is idempotent** (redact(redact(x)) == redact(x))
- **No PII in redacted output** (detector(redact(x)) == empty)
- **Hash consistency** (hash(x) always produces same output)
- **Unicode safety** (detector handles all UTF-8)

**Tool**: `proptest` crate

#### 3.3 Integration Tests
**Location**: `crates/gateway-tests/tests/`

**Scenarios**:
1. **Policy Hot-Reload**: Modify policy file, verify reload within 1s
2. **Tenant Routing**: Request with API key → correct provider route
3. **PII Filtering**: Request with PII → redacted before forwarding
4. **Quota Enforcement**: Exceed quota → request rejected
5. **Multi-Region**: Region-based routing based on data classification

---

## File Structure Changes

### New Files to Create
```
crates/policy-engine/
├── src/
│   ├── schema.rs         (NEW)
│   ├── loader.rs         (NEW)
│   ├── evaluator.rs      (NEW)
│   ├── decision.rs       (NEW)
│   ├── tenant.rs         (NEW)
│   └── lib.rs            (EXPAND)
└── tests/
    ├── policy_tests.rs   (NEW)
    └── hot_reload_tests.rs (NEW)

crates/context-firewall/
├── src/
│   ├── detector.rs       (IMPLEMENT)
│   ├── classifier.rs     (IMPLEMENT)
│   ├── redactor.rs       (IMPLEMENT)
│   ├── report.rs         (IMPLEMENT)
│   ├── patterns/
│   │   ├── general.rs    (IMPLEMENT)
│   │   ├── healthcare.rs (IMPLEMENT)
│   │   ├── financial.rs  (IMPLEMENT)
│   │   └── africa.rs     (IMPLEMENT)
│   └── lib.rs            (EXPAND)
└── tests/
    ├── detection_tests.rs (NEW)
    └── property_tests.rs  (NEW)

crates/gateway-tests/tests/
├── policy_integration.rs  (NEW)
└── firewall_integration.rs (NEW)
```

### Files to Modify
- `crates/gateway-server/src/handlers/chat.rs` - Add firewall middleware
- `crates/gateway-server/src/middleware/` - Add policy enforcement middleware
- `Cargo.toml` - Add `notify`, `aho-corasick`, `proptest` dependencies

---

## Dependencies to Add

### Policy Engine
```toml
notify = "6.1"              # File watching
glob = "0.3"                # Pattern matching for policy files
```

### Context Firewall
```toml
aho-corasick = "1.1"        # Fast multi-pattern matching
regex = "1.11"              # Regex for PII patterns
sha2 = "0.10"               # Hashing for redaction
```

### Testing
```toml
proptest = "1.4"            # Property-based testing
test-case = "3.3"           # Parameterized tests
```

---

## Timeline

### Week 1: Policy Engine
- [ ] Day 1-2: Schema definition and TOML parsing
- [ ] Day 3-4: Policy loader with hot-reload
- [ ] Day 5: Policy evaluator and decision logic

### Week 2: Context Firewall
- [ ] Day 1-2: PII detector (general patterns)
- [ ] Day 3: Africa-specific patterns
- [ ] Day 4: Data classifier
- [ ] Day 5: Redactor implementation

### Week 3: Testing & Integration
- [ ] Day 1-2: Property tests for firewall
- [ ] Day 3: Integration tests
- [ ] Day 4: Documentation and examples
- [ ] Day 5: Code review, polish, and merge

---

## Risk Mitigation

### Technical Risks
1. **Hot-reload performance**: File watching may cause CPU spikes
   - **Mitigation**: Debounce file events, limit watch frequency

2. **False positives in PII detection**: Over-aggressive pattern matching
   - **Mitigation**: Confidence scoring, manual override in policies

3. **Regex performance on large payloads**: Complex patterns may be slow
   - **Mitigation**: Use aho-corasick for keywords, limit regex complexity

### Process Risks
1. **Scope creep**: Too many PII patterns
   - **Mitigation**: Focus on high-priority patterns first (Africa, finance, healthcare)

2. **Testing coverage**: Missing edge cases
   - **Mitigation**: Property tests catch edge cases automatically

---

## Documentation Deliverables

1. **Policy Schema Reference** (`docs/policy-schema.md`)
   - TOML structure
   - Field descriptions
   - Examples for each use case

2. **PII Pattern Catalog** (`docs/pii-patterns.md`)
   - Supported patterns
   - Region-specific patterns
   - Confidence scoring guide

3. **Firewall Configuration Guide** (`docs/firewall-config.md`)
   - Redaction strategies
   - Classification rules
   - Performance tuning

4. **Integration Guide** (`docs/integration-guide.md`)
   - How to add custom PII patterns
   - How to create custom policies
   - How to extend classification rules

---

## Success Metrics

### Code Quality
- ✅ All tests pass
- ✅ Clippy warnings = 0
- ✅ Code coverage > 80% for policy-engine and context-firewall
- ✅ No `unsafe` code introduced

### Performance
- Policy evaluation: < 1ms per request
- Hot-reload latency: < 1 second
- PII detection: < 10ms for 10KB payload
- Redaction overhead: < 20% of detection time

### Functionality
- Supports 20+ PII patterns (general)
- Supports 10+ Africa-specific patterns
- Policy hot-reload works without errors
- All redaction strategies implemented

---

## Phase 2 Completion Criteria

Phase 2 will be considered **complete** when:

- [x] All implementation tasks above are finished
- [x] All tests pass (`cargo nextest run --workspace`)
- [x] Clippy passes with `-D warnings`
- [x] Documentation is updated with examples
- [x] Integration tests demonstrate end-to-end flows
- [x] Property tests validate correctness properties
- [x] Code review completed
- [x] Branch merged to `main`
- [x] Status document created

---

## Next Phase Preview: Phase 3 - Provider Integration

After Phase 2, we'll implement:
- OpenAI adapter (GPT models)
- Anthropic adapter (Claude models)
- Azure OpenAI adapter
- Streaming support for all providers
- Retry logic and circuit breakers

This will enable actual LLM request proxying with the policy and firewall protections from Phase 2.

---

**Prepared by**: Claude Sonnet 4.5
**Start Date**: 2026-02-11
**Target Completion**: 2026-03-04 (3 weeks)
**Next Review**: End of Week 1 (Policy Engine completion)
