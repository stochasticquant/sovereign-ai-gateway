-- Tenants table: core multi-tenancy support.
CREATE TABLE IF NOT EXISTS tenants (
    id          UUID PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    description TEXT,
    policy_id   TEXT,               -- references a policy file name
    region      TEXT NOT NULL,       -- e.g., "ng-west-1", "ke-east-1"
    is_active   BOOLEAN NOT NULL DEFAULT TRUE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_tenants_active ON tenants (is_active) WHERE is_active = TRUE;
