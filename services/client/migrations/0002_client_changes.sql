-- Лента изменений заказчиков и контактов для других сервисов (воронка строит по ней проекцию).
-- Одна строка на сущность: последнее состояние либо признак удаления.
CREATE TABLE client_changes (
    seq       bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    tenant_id uuid    NOT NULL,
    entity    text    NOT NULL CHECK (entity IN ('client', 'contact')),
    entity_id uuid    NOT NULL,
    deleted   boolean NOT NULL DEFAULT false,
    snapshot  jsonb,
    CHECK (deleted = (snapshot IS NULL))
);

CREATE INDEX client_changes_tenant_seq_idx ON client_changes (tenant_id, seq);
CREATE INDEX client_changes_entity_idx ON client_changes (tenant_id, entity, entity_id);

ALTER TABLE client_changes ENABLE ROW LEVEL SECURITY;
ALTER TABLE client_changes FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON client_changes
    USING (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid);

-- Запись в ленту идёт в транзакции самого изменения. Запись в рамках организации
-- сериализуется advisory-блокировкой: позиции появляются в порядке фиксации,
-- и потребитель с курсором не пропустит запись, закоммиченную позже большей.
CREATE FUNCTION record_client_change(
    p_tenant uuid, p_entity text, p_id uuid, p_deleted boolean, p_snapshot jsonb
) RETURNS void
LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_advisory_xact_lock(hashtextextended('client_changes:' || p_tenant::text, 0));
    DELETE FROM client_changes WHERE tenant_id = p_tenant AND entity = p_entity AND entity_id = p_id;
    INSERT INTO client_changes (tenant_id, entity, entity_id, deleted, snapshot)
    VALUES (p_tenant, p_entity, p_id, p_deleted, CASE WHEN p_deleted THEN NULL ELSE p_snapshot END);
END
$$;

CREATE FUNCTION contact_snapshot(c contacts) RETURNS jsonb
LANGUAGE sql STABLE AS $$
    SELECT jsonb_build_object(
        'client_id', c.client_id,
        'full_name', c.full_name,
        'position', (SELECT p.name FROM positions p
                      WHERE p.tenant_id = c.tenant_id AND p.id = c.position_id)
    )
$$;

CREATE FUNCTION trg_client_change() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        PERFORM record_client_change(OLD.tenant_id, 'client', OLD.id, true, NULL);
        RETURN OLD;
    END IF;
    PERFORM record_client_change(NEW.tenant_id, 'client', NEW.id, false,
        jsonb_build_object('name', NEW.name, 'archived', NEW.archived));
    RETURN NEW;
END
$$;

CREATE FUNCTION trg_contact_change() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        PERFORM record_client_change(OLD.tenant_id, 'contact', OLD.id, true, NULL);
        RETURN OLD;
    END IF;
    PERFORM record_client_change(NEW.tenant_id, 'contact', NEW.id, false, contact_snapshot(NEW));
    RETURN NEW;
END
$$;

-- Переименование должности меняет состояние всех её контактов.
CREATE FUNCTION trg_position_rename() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
    c contacts;
BEGIN
    FOR c IN SELECT * FROM contacts WHERE tenant_id = NEW.tenant_id AND position_id = NEW.id LOOP
        PERFORM record_client_change(c.tenant_id, 'contact', c.id, false, contact_snapshot(c));
    END LOOP;
    RETURN NEW;
END
$$;

CREATE TRIGGER clients_change AFTER INSERT OR UPDATE OR DELETE ON clients
    FOR EACH ROW EXECUTE FUNCTION trg_client_change();
CREATE TRIGGER contacts_change AFTER INSERT OR UPDATE OR DELETE ON contacts
    FOR EACH ROW EXECUTE FUNCTION trg_contact_change();
CREATE TRIGGER positions_rename AFTER UPDATE OF name ON positions
    FOR EACH ROW EXECUTE FUNCTION trg_position_rename();
