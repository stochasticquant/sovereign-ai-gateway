-- API keys table: hashed keys for tenant authentication.
CREATE TABLE IF NOT EXISTS api_keys (
    id          UUID PRIMARY KEY,
    tenant_id   UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    key_hash    TEXT NOT NULL UNIQUE,     -- SHA-256 hash of the raw key
    key_prefix  TEXT NOT NULL,            -- first 8 chars for identification (e.g., "sg-live_a1b2")
    name        TEXT,                     -- human-readable label
    scopes      TEXT[] NOT NULL DEFAULT '{}',  -- e.g., ["chat", "embeddings", "admin"]
    is_active   BOOLEAN NOT NULL DEFAULT TRUE,
    expires_at  TIMESTAMPTZ,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_used   TIMESTAMPTZ
);

CREATE INDEX idx_api_keys_hash ON api_keys (key_hash) WHERE is_active = TRUE;
CREATE INDEX idx_api_keys_tenant ON api_keys (tenant_id);
