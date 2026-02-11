-- Audit log table: append-only record of every gateway request.
-- Partitioning by month is recommended for production.
CREATE TABLE IF NOT EXISTS audit_log (
    id                  UUID PRIMARY KEY,
    request_id          TEXT NOT NULL,
    timestamp           TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    tenant_id           UUID NOT NULL REFERENCES tenants(id),
    data_classification TEXT NOT NULL,          -- public, internal, confidential, restricted
    provider_id         TEXT NOT NULL,
    model               TEXT NOT NULL,
    prompt_tokens       INTEGER NOT NULL DEFAULT 0,
    completion_tokens   INTEGER NOT NULL DEFAULT 0,
    total_tokens        INTEGER NOT NULL DEFAULT 0,
    region              TEXT NOT NULL,
    risk_score          SMALLINT NOT NULL DEFAULT 0,
    decision            TEXT NOT NULL,          -- allow, block, redact_and_route
    latency_ms          INTEGER NOT NULL DEFAULT 0,
    status_code         SMALLINT NOT NULL,
    error               TEXT
);

CREATE INDEX idx_audit_log_tenant_ts ON audit_log (tenant_id, timestamp DESC);
CREATE INDEX idx_audit_log_timestamp ON audit_log (timestamp DESC);
CREATE INDEX idx_audit_log_decision ON audit_log (decision);
