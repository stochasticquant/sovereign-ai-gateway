# Policy Schema Reference

Policies are TOML files that define data sovereignty rules, provider routing, quotas, and PII redaction for each tenant. They live in `policies/` and are validated on load.

## Schema Version

```toml
[metadata]
version = "1.0"                                    # Required. Schema version.
description = "Human-readable policy description"  # Required. What this policy enforces.
```

## Data Rules

Maps each data classification level to a routing constraint. All four levels are required.

```toml
[data_rules]
public       = "external_allowed"   # Can route to any provider
internal     = "regional_only"      # Must stay within the same geographic region
confidential = "local_only"         # Must use local/on-premises providers only
restricted   = "blocked"            # Do not process the request
```

### Routing Constraints

| Value              | Description                                      |
|--------------------|--------------------------------------------------|
| `external_allowed` | Route to any provider (external or local)        |
| `regional_only`    | Must stay within the same geographic region      |
| `local_only`       | Must use local/on-premises providers only        |
| `blocked`          | Request is rejected — do not process             |

### Data Classification Levels

Classification is determined automatically by the context firewall based on PII severity:

| Level          | PII Severity Threshold | Typical Content                          |
|----------------|------------------------|------------------------------------------|
| `public`       | No PII detected        | General queries, public information      |
| `internal`     | Severity 3-5           | Drug names, IP addresses                 |
| `confidential` | Severity 6-8           | Phone numbers, API keys, mobile money    |
| `restricted`   | Severity 9-10          | National IDs, medical records, SSNs, credit cards |

## Providers

Each provider is a named TOML table under `[providers]`.

```toml
[providers.azure_eastafrica]
region   = "ke-east-1"              # Required. Geographic region identifier.
allowed  = true                     # Required. Whether this provider can be used.
priority = 1                        # Optional. Lower = higher priority. Default: 100.
models   = ["gpt-4o", "llama-3"]   # Optional. Available model names. Default: [].
endpoint = "https://custom.url"     # Optional. Custom endpoint URL override.
auth     = "azure_ad"               # Optional. Authentication method override.
```

### Provider Fields

| Field      | Type       | Required | Default | Description                              |
|------------|------------|----------|---------|------------------------------------------|
| `region`   | String     | Yes      | —       | Geographic region this provider operates in |
| `allowed`  | Boolean    | Yes      | —       | Whether this provider is enabled         |
| `priority` | Integer    | No       | `100`   | Selection priority (lower = preferred)   |
| `models`   | String[]   | No       | `[]`    | List of available model identifiers      |
| `endpoint` | String     | No       | —       | Custom endpoint URL                      |
| `auth`     | String     | No       | —       | Authentication method override           |

### Validation Rules

- At least one provider must have `allowed = true`.
- Each provider must have a non-empty `region`.
- A warning is issued if an allowed provider has no `models` configured.

## Quotas

Token and rate-limiting configuration.

```toml
[quotas]
max_tokens_per_request      = 4096      # Required. Max tokens in a single request.
max_tokens_per_day          = 2_000_000 # Required. Daily token budget per tenant.
burst_requests_per_minute   = 120       # Required. Burst rate limit (req/min).
sustained_requests_per_hour = 3600      # Optional. Sustained rate limit (req/hour).
```

| Field                        | Type    | Required | Description                          |
|------------------------------|---------|----------|--------------------------------------|
| `max_tokens_per_request`     | u32     | Yes      | Maximum tokens per single request    |
| `max_tokens_per_day`         | u64     | Yes      | Daily token budget per tenant        |
| `burst_requests_per_minute`  | u32     | Yes      | Burst rate limit (requests/minute)   |
| `sustained_requests_per_hour`| u32     | No       | Sustained rate limit (requests/hour) |

All required quota values must be greater than zero. A warning is issued if `max_tokens_per_request` exceeds 1,000,000.

## Redaction

PII redaction and blocking configuration.

```toml
[redaction]
mode             = "irreversible"
block_categories = ["national_id", "medical_record", "bank_account"]
```

### Redaction Modes

| Mode           | Description                                             |
|----------------|---------------------------------------------------------|
| `irreversible` | PII is masked or removed permanently                    |
| `reversible`   | PII is hashed with a tenant key (can be recovered)      |
| `audit_only`   | PII is logged but not redacted (audit trail only)       |

### Block Categories

PII categories listed in `block_categories` cause the entire request to be blocked (not just redacted). Valid category names correspond to PII type identifiers:

`email`, `phone_number`, `credit_card`, `ssn`, `ip_address`, `api_key`, `national_id`, `medical_record`, `bank_account`, `drug_name`, `diagnosis_code`, `south_african_id`, `nigerian_bvn`, `kenyan_national_id`, `mobile_money`

### Custom Patterns

Optional additional regex patterns for tenant-specific PII detection:

```toml
[redaction.custom_patterns]
employee_id = "\\bEMP-\\d{6}\\b"
```

## Complete Examples

### South African Government (Maximum Restriction)

```toml
[metadata]
version = "1.0"
description = "Government data sovereignty policy — South Africa"

[data_rules]
public       = "local_only"
internal     = "local_only"
confidential = "local_only"
restricted   = "local_only"

[providers.local_llm]
region   = "za-west-1"
allowed  = true
priority = 1
models   = ["llama-3", "mistral"]

[quotas]
max_tokens_per_request    = 2048
max_tokens_per_day        = 200_000
burst_requests_per_minute = 20

[redaction]
mode             = "irreversible"
block_categories = ["national_id", "medical_record", "bank_account", "tax_id"]
```

### Kenyan Fintech (Regional Routing)

```toml
[metadata]
version = "1.0"
description = "Fintech data sovereignty policy — Kenya"

[data_rules]
public       = "external_allowed"
internal     = "regional_only"
confidential = "regional_only"
restricted   = "local_only"

[providers.azure_eastafrica]
region   = "ke-east-1"
allowed  = true
priority = 1
models   = ["gpt-4o"]

[providers.local_llm]
region   = "ke-west-1"
allowed  = true
priority = 2
models   = ["llama-3", "mistral"]

[providers.openai]
region   = "us-east-1"
allowed  = true
priority = 3

[quotas]
max_tokens_per_request    = 4096
max_tokens_per_day        = 2_000_000
burst_requests_per_minute = 120

[redaction]
mode             = "reversible"
block_categories = ["national_id", "bank_account"]
```

### Nigerian Healthcare (Medical Data Protection)

```toml
[metadata]
version = "1.0"
description = "Healthcare data sovereignty policy — Nigeria"

[data_rules]
public       = "regional_only"
internal     = "local_only"
confidential = "local_only"
restricted   = "local_only"

[providers.local_llm]
region   = "ng-west-1"
allowed  = true
priority = 1
models   = ["llama-3-medical", "mistral"]

[quotas]
max_tokens_per_request    = 2048
max_tokens_per_day        = 500_000
burst_requests_per_minute = 30

[redaction]
mode             = "irreversible"
block_categories = ["national_id", "medical_record", "drug_name", "diagnosis_code"]
```
