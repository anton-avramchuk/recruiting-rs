# Мультитенантность: как добавить тенантную таблицу

Данные тенантов лежат в общей БД сервиса, изоляция — Row-Level Security (RLS).
Контекст тенанта задаёт `platform_db::TenantTx` (`set_config('app.tenant_id', .., true)`),
он живёт только до конца транзакции.

## Правила

1. Таблица содержит `tenant_id uuid NOT NULL`.
2. Включены `ENABLE ROW LEVEL SECURITY` **и** `FORCE ROW LEVEL SECURITY`.
3. Есть политика с `USING` и `WITH CHECK`, читающая контекст как
   `NULLIF(current_setting('app.tenant_id', true), '')::uuid`.
   `NULLIF` обязателен: после завершения транзакции параметр остаётся пустой строкой, а не NULL,
   и приведение `''::uuid` падает.
4. Приложение подключается ролью без `BYPASSRLS` и не владельцем таблиц; миграции — владельцем.
   Права приложению выдаются отдельно (`GRANT SELECT, INSERT, UPDATE, DELETE`).
5. Запросы к тенантным таблицам идут только через `TenantTx`. Без контекста строк не видно.
6. В интеграционных тестах сервиса вызывается `platform_db::testing::tables_without_rls` —
   результат должен быть пустым.

## Шаблон миграции

```sql
CREATE TABLE widgets (
    id        uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id uuid NOT NULL,
    name      text NOT NULL
);

ALTER TABLE widgets ENABLE ROW LEVEL SECURITY;
ALTER TABLE widgets FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON widgets
    USING (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid);
```

Этот шаблон — реальная тестовая миграция `crates/platform-db/tests/migrations/0001_widgets.sql`;
тесты `tests/tenant_isolation.rs` и `tests/rls_guard.rs` применяют её и проверяют изоляцию.

## Не тенантные данные

Реестр тенантов и платформенные администраторы (БД `admin`) не содержат `tenant_id` и RLS не используют;
доступ к ним ограничен самим сервисом admin.

## Дочерние таблицы

Если у тенантной таблицы есть дочерняя (места работы и образование кандидата), внешний ключ строится
по паре `(tenant_id, parent_id)`, а у родителя для этого объявляется `UNIQUE (tenant_id, id)`:

```sql
CREATE TABLE employments (
    id        uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id uuid NOT NULL,
    person_id uuid NOT NULL,
    FOREIGN KEY (tenant_id, person_id) REFERENCES persons (tenant_id, id) ON DELETE CASCADE
);
```

Так даже запрос от владельца схемы не может привязать строку одного тенанта к родителю другого.
Пример — `services/candidate/migrations/0001_persons.sql`; проверка — тест `history_cannot_point_at_a_person_of_another_tenant`.
