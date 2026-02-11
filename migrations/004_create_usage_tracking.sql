-- Usage tracking: per-tenant per-day token consumption.
-- Used by the token governor for quota enforcement.
CREATE TABLE IF NOT EXISTS usage_tracking (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   UUID NOT NULL REFERENCES tenants(id),
    date        DATE NOT NULL DEFAULT CURRENT_DATE,
    provider_id TEXT NOT NULL,
    model       TEXT NOT NULL,
    total_tokens BIGINT NOT NULL DEFAULT 0,
    request_count INTEGER NOT NULL DEFAULT 0,
    estimated_cost_usd NUMERIC(10, 6) NOT NULL DEFAULT 0,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE (tenant_id, date, provider_id, model)
);

CREATE INDEX idx_usage_tenant_date ON usage_tracking (tenant_id, date DESC);
