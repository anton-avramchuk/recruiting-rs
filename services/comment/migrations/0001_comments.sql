-- Комментарии к заказчикам, вакансиям и кандидатам. Объекты живут в других сервисах:
-- ссылка без внешнего ключа, существование объекта не проверяется.
CREATE TABLE comments (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   uuid        NOT NULL,
    target_type text        NOT NULL CHECK (target_type IN ('client', 'vacancy', 'person')),
    target_id   uuid        NOT NULL,
    author_id   uuid        NOT NULL,
    author_name text        NOT NULL,
    body        text        NOT NULL,
    created_at  timestamptz NOT NULL DEFAULT clock_timestamp(),
    edited_at   timestamptz,
    UNIQUE (tenant_id, id)
);

CREATE INDEX comments_target_idx ON comments (tenant_id, target_type, target_id, created_at, id);

ALTER TABLE comments ENABLE ROW LEVEL SECURITY;
ALTER TABLE comments FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON comments
    USING (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid);
