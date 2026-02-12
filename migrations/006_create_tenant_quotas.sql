-- Tenant quotas: daily token limits and rate limiting config.
-- Used by the token governor for pre-flight quota checks.
CREATE TABLE IF NOT EXISTS tenant_quotas (
    tenant_id UUID PRIMARY KEY REFERENCES tenants(id) ON DELETE CASCADE,

    -- Daily token limit (null = unlimited)
    daily_token_limit BIGINT,

    -- Maximum tokens per single request (null = unlimited)
    max_tokens_per_request INTEGER,

    -- Maximum requests per minute (null = unlimited)
    max_requests_per_minute INTEGER,

    -- Maximum concurrent requests (null = unlimited)
    max_concurrent_requests INTEGER,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for fast quota lookups
CREATE INDEX idx_quotas_tenant ON tenant_quotas (tenant_id);
