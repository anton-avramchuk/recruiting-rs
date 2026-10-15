-- Лента изменений кандидатов для других сервисов (воронка строит по ней свою проекцию).
-- Одна строка на человека: последнее состояние либо признак удаления.
CREATE TABLE person_changes (
    seq       bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    tenant_id uuid    NOT NULL,
    person_id uuid    NOT NULL,
    deleted   boolean NOT NULL DEFAULT false,
    snapshot  jsonb,
    CHECK (deleted = (snapshot IS NULL))
);

CREATE INDEX person_changes_tenant_seq_idx ON person_changes (tenant_id, seq);
CREATE INDEX person_changes_person_idx ON person_changes (tenant_id, person_id);

ALTER TABLE person_changes ENABLE ROW LEVEL SECURITY;
ALTER TABLE person_changes FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON person_changes
    USING (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid);

CREATE FUNCTION person_snapshot(p persons) RETURNS jsonb
LANGUAGE sql IMMUTABLE AS $$
    SELECT jsonb_build_object(
        'full_name', p.full_name,
        'source', p.source,
        'city', p.city,
        'expected_salary', p.expected_salary,
        'email', p.email,
        'phone', p.phone,
        'skills', to_jsonb(p.skills),
        'experience_years', p.experience_years
    )
$$;

-- Запись в ленту идёт в транзакции самого изменения. Запись в рамках организации
-- сериализуется advisory-блокировкой: позиции появляются в порядке фиксации,
-- и потребитель с курсором не пропустит запись, закоммиченную позже большей.
CREATE FUNCTION record_person_change() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        PERFORM pg_advisory_xact_lock(hashtextextended('person_changes:' || OLD.tenant_id::text, 0));
        DELETE FROM person_changes WHERE tenant_id = OLD.tenant_id AND person_id = OLD.id;
        INSERT INTO person_changes (tenant_id, person_id, deleted, snapshot)
        VALUES (OLD.tenant_id, OLD.id, true, NULL);
        RETURN OLD;
    END IF;
    PERFORM pg_advisory_xact_lock(hashtextextended('person_changes:' || NEW.tenant_id::text, 0));
    DELETE FROM person_changes WHERE tenant_id = NEW.tenant_id AND person_id = NEW.id;
    INSERT INTO person_changes (tenant_id, person_id, deleted, snapshot)
    VALUES (NEW.tenant_id, NEW.id, false, person_snapshot(NEW));
    RETURN NEW;
END
$$;

CREATE TRIGGER persons_record_change
    AFTER INSERT OR UPDATE OR DELETE ON persons
    FOR EACH ROW EXECUTE FUNCTION record_person_change();

-- Кандидаты, существовавшие до ленты.
INSERT INTO person_changes (tenant_id, person_id, deleted, snapshot)
SELECT tenant_id, id, false, person_snapshot(persons) FROM persons ORDER BY created_at;
