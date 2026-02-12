-- Retention policies table: configure audit log retention per tenant.
CREATE TABLE IF NOT EXISTS audit_retention_policies (
    tenant_id       UUID PRIMARY KEY REFERENCES tenants(id),
    retention_days  INTEGER NOT NULL DEFAULT 90
);

CREATE INDEX idx_audit_retention_tenant ON audit_retention_policies (tenant_id);
