CREATE TABLE vacancies (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id      uuid        NOT NULL,
    title          text        NOT NULL,
    department     text,
    city           text,
    openings       integer     NOT NULL DEFAULT 1 CHECK (openings >= 1),
    priority       text        NOT NULL DEFAULT 'normal' CHECK (priority IN ('high', 'normal', 'low')),
    hiring_manager text,
    recruiter      text,
    required_skills text[]     NOT NULL DEFAULT '{}',
    opened_on      date        NOT NULL,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, id)
);

CREATE INDEX vacancies_tenant_idx ON vacancies (tenant_id, opened_on);

-- Проекция людей из сервиса candidate: данные для списков, фильтров и сортировки.
CREATE TABLE people (
    tenant_id        uuid        NOT NULL,
    person_id        uuid        NOT NULL,
    full_name        text        NOT NULL,
    source           text,
    city             text,
    expected_salary  bigint,
    email            text,
    phone            text,
    skills           text[]      NOT NULL DEFAULT '{}',
    experience_years integer     NOT NULL DEFAULT 0,
    synced_at        timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, person_id)
);

CREATE INDEX people_tenant_name_idx ON people (tenant_id, lower(full_name));

-- Курсор чтения ленты изменений candidate по организации.
CREATE TABLE people_sync (
    tenant_id uuid PRIMARY KEY,
    position  bigint      NOT NULL DEFAULT 0,
    synced_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE applications (
    id               uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id        uuid        NOT NULL,
    person_id        uuid        NOT NULL,
    vacancy_id       uuid        NOT NULL,
    stage            text        NOT NULL DEFAULT 'new'
        CHECK (stage IN ('new', 'screening', 'hr', 'tech', 'final', 'offer', 'hired', 'rejected')),
    stage_changed_at timestamptz NOT NULL,
    created_at       timestamptz NOT NULL,
    UNIQUE (tenant_id, id),
    UNIQUE (tenant_id, person_id, vacancy_id),
    -- исчезновение человека из проекции (удаление кандидата) уносит его отклики
    FOREIGN KEY (tenant_id, person_id) REFERENCES people (tenant_id, person_id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, vacancy_id) REFERENCES vacancies (tenant_id, id) ON DELETE CASCADE
);

CREATE INDEX applications_vacancy_idx ON applications (tenant_id, vacancy_id, stage);
CREATE INDEX applications_person_idx ON applications (tenant_id, person_id);
CREATE INDEX applications_stage_idx ON applications (tenant_id, stage, stage_changed_at);

CREATE TABLE application_events (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    seq            bigint GENERATED ALWAYS AS IDENTITY,
    tenant_id      uuid        NOT NULL,
    application_id uuid        NOT NULL,
    kind           text        NOT NULL
        CHECK (kind IN ('created', 'stage', 'rejected', 'letter', 'interview', 'handoff')),
    text           text        NOT NULL,
    at             timestamptz NOT NULL,
    FOREIGN KEY (tenant_id, application_id) REFERENCES applications (tenant_id, id) ON DELETE CASCADE
);

CREATE INDEX application_events_app_idx ON application_events (tenant_id, application_id, at DESC, seq DESC);

CREATE TABLE interviews (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id      uuid        NOT NULL,
    application_id uuid        NOT NULL,
    scheduled_at   timestamptz NOT NULL,
    created_at     timestamptz NOT NULL,
    FOREIGN KEY (tenant_id, application_id) REFERENCES applications (tenant_id, id) ON DELETE CASCADE
);

CREATE INDEX interviews_tenant_time_idx ON interviews (tenant_id, scheduled_at);

ALTER TABLE vacancies ENABLE ROW LEVEL SECURITY;
ALTER TABLE vacancies FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON vacancies
    USING (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid);

ALTER TABLE people ENABLE ROW LEVEL SECURITY;
ALTER TABLE people FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON people
    USING (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid);

ALTER TABLE people_sync ENABLE ROW LEVEL SECURITY;
ALTER TABLE people_sync FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON people_sync
    USING (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid);

ALTER TABLE applications ENABLE ROW LEVEL SECURITY;
ALTER TABLE applications FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON applications
    USING (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid);

ALTER TABLE application_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE application_events FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON application_events
    USING (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid);

ALTER TABLE interviews ENABLE ROW LEVEL SECURITY;
ALTER TABLE interviews FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON interviews
    USING (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid);
