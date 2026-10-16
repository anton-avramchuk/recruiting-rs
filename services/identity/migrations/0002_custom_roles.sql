-- Собственные роли организации: название и набор прав из ограниченного списка.
CREATE TABLE roles (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   uuid        NOT NULL,
    name        text        NOT NULL,
    name_key    text        NOT NULL,
    permissions text[]      NOT NULL,
    created_at  timestamptz NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, name_key)
);

ALTER TABLE roles ENABLE ROW LEVEL SECURITY;
ALTER TABLE roles FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON roles
    USING (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid);

-- Либо встроенные роли (`roles`), либо одна собственная (`role_id`); удаление назначенной роли запрещено.
ALTER TABLE users ADD COLUMN role_id uuid REFERENCES roles (id);
CREATE INDEX users_role_id_idx ON users (role_id) WHERE role_id IS NOT NULL;
