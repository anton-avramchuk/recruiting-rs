CREATE TABLE persons (
    id                        uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id                 uuid        NOT NULL,
    full_name                 text        NOT NULL,
    source                    text,
    phone                     text,
    email                     text,
    telegram                  text,
    whatsapp                  text,
    expected_salary           bigint,
    summary                   text,
    skills                    text[]      NOT NULL DEFAULT '{}',
    -- стаж, заявленный при заведении; действует, пока нет мест работы
    declared_experience_years integer     NOT NULL DEFAULT 0 CHECK (declared_experience_years >= 0),
    -- итоговый стаж: по местам работы или заявленный (хранится ради фильтра по диапазону)
    experience_years          integer     NOT NULL DEFAULT 0 CHECK (experience_years >= 0),
    -- ключи дедупликации: пересчитываются доменом при каждой записи
    normalized_name           text        NOT NULL,
    normalized_email          text,
    normalized_phone          text,
    created_at                timestamptz NOT NULL DEFAULT now(),
    updated_at                timestamptz NOT NULL DEFAULT now(),
    -- цель составного внешнего ключа: история не может сослаться на человека другого тенанта
    UNIQUE (tenant_id, id)
);

CREATE INDEX persons_tenant_created_idx ON persons (tenant_id, created_at DESC);
CREATE INDEX persons_tenant_email_idx ON persons (tenant_id, normalized_email);
CREATE INDEX persons_tenant_phone_idx ON persons (tenant_id, normalized_phone);
CREATE INDEX persons_tenant_name_idx ON persons (tenant_id, normalized_name);
CREATE INDEX persons_tenant_experience_idx ON persons (tenant_id, experience_years);
CREATE INDEX persons_skills_idx ON persons USING gin (skills);

CREATE TABLE employments (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id      uuid    NOT NULL,
    person_id      uuid    NOT NULL,
    idx            integer NOT NULL,
    company        text    NOT NULL,
    position_title text    NOT NULL,
    start_date     date    NOT NULL,
    end_date       date,
    description    text,
    CHECK (end_date IS NULL OR end_date >= start_date),
    FOREIGN KEY (tenant_id, person_id) REFERENCES persons (tenant_id, id) ON DELETE CASCADE
);

CREATE INDEX employments_person_idx ON employments (tenant_id, person_id, idx);

CREATE TABLE educations (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   uuid    NOT NULL,
    person_id   uuid    NOT NULL,
    idx         integer NOT NULL,
    institution text    NOT NULL,
    specialty   text,
    year_from   integer,
    year_to     integer,
    CHECK (year_from IS NULL OR year_to IS NULL OR year_to >= year_from),
    FOREIGN KEY (tenant_id, person_id) REFERENCES persons (tenant_id, id) ON DELETE CASCADE
);

CREATE INDEX educations_person_idx ON educations (tenant_id, person_id, idx);

ALTER TABLE persons ENABLE ROW LEVEL SECURITY;
ALTER TABLE persons FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON persons
    USING (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid);

ALTER TABLE employments ENABLE ROW LEVEL SECURITY;
ALTER TABLE employments FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON employments
    USING (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid);

ALTER TABLE educations ENABLE ROW LEVEL SECURITY;
ALTER TABLE educations FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON educations
    USING (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid);
