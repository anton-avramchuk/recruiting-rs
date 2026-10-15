-- Проекция заказчиков и контактных лиц из сервиса client и привязка вакансий.
CREATE TABLE clients (
    tenant_id uuid        NOT NULL,
    client_id uuid        NOT NULL,
    name      text        NOT NULL,
    archived  boolean     NOT NULL DEFAULT false,
    synced_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, client_id)
);

CREATE TABLE client_contacts (
    tenant_id  uuid        NOT NULL,
    contact_id uuid        NOT NULL,
    client_id  uuid        NOT NULL,
    full_name  text        NOT NULL,
    position   text,
    synced_at  timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, contact_id)
);

CREATE INDEX client_contacts_client_idx ON client_contacts (tenant_id, client_id);

-- Курсор чтения ленты изменений client по организации.
CREATE TABLE clients_sync (
    tenant_id uuid PRIMARY KEY,
    position  bigint      NOT NULL DEFAULT 0,
    synced_at timestamptz NOT NULL DEFAULT now()
);

-- Ссылки без внешних ключей: заказчики живут в другом сервисе. У вакансий, созданных до
-- появления заказчиков, обе пусты; прежние текстовые department и hiring_manager остаются
-- в таблице, но больше не читаются и не пишутся.
ALTER TABLE vacancies ADD COLUMN client_id uuid;
ALTER TABLE vacancies ADD COLUMN contact_id uuid;
CREATE INDEX vacancies_client_idx ON vacancies (tenant_id, client_id);

ALTER TABLE clients ENABLE ROW LEVEL SECURITY;
ALTER TABLE clients FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON clients
    USING (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid);

ALTER TABLE client_contacts ENABLE ROW LEVEL SECURITY;
ALTER TABLE client_contacts FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON client_contacts
    USING (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid);

ALTER TABLE clients_sync ENABLE ROW LEVEL SECURITY;
ALTER TABLE clients_sync FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON clients_sync
    USING (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid);
