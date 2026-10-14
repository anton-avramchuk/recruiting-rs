# Design

## Context

Референс — .NET-версия (`E:\Projects\recruiting-crm`): identity + доменные сервисы за BFF, БД на сервис, Redis, Angular SPA. Репозиторий `recruiting-rs` пуст. См. proposal.md — Why. Скелет содержит только платформу; домен добавляется отдельными change.

## Goals / Non-Goals

**Goals:**
- Рабочий скелет: `docker compose up` + `cargo run`/`cargo leptos watch` → вход пользователя тенанта и admin управляет тенантами.
- Зафиксировать «форму» сервиса (crate-слои, миграции, тесты), которую скопируют доменные сервисы.
- Изоляцию тенантов проверять тестами против настоящего PostgreSQL.

**Non-Goals:**
- Доменные сервисы, шина событий, service discovery/оркестратор (аналог Aspire), Kubernetes, SSO/OAuth, 2FA.

## Decisions

**Workspace-раскладка.**
```
crates/
  platform-core/      # конфиг, ошибки (problem+json), tracing, health, shutdown
  platform-db/        # пул sqlx, TenantTx (SET LOCAL app.tenant_id), мигратор
  platform-auth/      # JWT claims/валидация, роли и права, axum extractors
  bff-core/           # сессии в Redis, cookie, CSRF, refresh, прокси — общий код обоих BFF
  ui-kit/             # общие Leptos-компоненты (формы, таблицы, layout)
  identity-contracts/ admin-contracts/   # DTO (serde), общие с web
services/
  identity/  admin/
  bff-crm/  bff-admin/   # тонкие бинарники поверх bff-core: свои маршруты и allowlist
apps/
  web-crm/  web-admin/   # Leptos (SSR + hydrate), cargo-leptos, у каждого свой BFF
docker/  docker-compose.yml
```
Сервис = бинарник с модулями `domain / application / infrastructure / api` внутри одного crate (вместо 6–8 проектов .NET; разделение на crates — когда появится реальная причина). Альтернатива «crate на слой» отвергнута как избыточная для скелета.

**Методология: TDD + DDD, общий код в общих crates.**
- TDD: red → green → refactor; тест пишется и коммитится раньше реализации. Порядок задач в tasks.md это отражает (тест формулируется первым).
- DDD: каждый сервис — ограниченный контекст. identity: агрегат `User` (email, хеш пароля, роли, статус), `RefreshToken`; admin: агрегаты `Tenant` (slug, статус, переходы `provisioning → active ⇄ suspended / failed`) и `PlatformAdmin`. Инварианты и переходы статусов — в `domain` без зависимостей от axum/sqlx; репозитории — трейты, реализации в `infrastructure`; `application` оркестрирует use case'ы.
- Общее — в crates: `platform-core`, `platform-db`, `platform-auth`, `bff-core`, `ui-kit`, `*-contracts`. Критерий выноса: код нужен двум и более сервисам и не содержит доменной логики контекста. Альтернатива «копировать в сервис» допускается только до второго использования.

**Стек.** axum + tokio, sqlx (PostgreSQL, compile-time-проверка выключена, `sqlx migrate`), utoipa для OpenAPI, tracing + tracing-subscriber, argon2 для паролей, jsonwebtoken. Leptos + cargo-leptos для web; Leptos server functions ходят в свой BFF (BFF не размещается в процессе web-сервера). Альтернативы Dioxus/Yew отклонены пользователем.

**Тенантность: общая БД + RLS.** Тенантные таблицы имеют `tenant_id uuid not null` и политику `USING (tenant_id = current_setting('app.tenant_id', true)::uuid)` + `WITH CHECK`. Приложение подключается ролью без `BYPASSRLS` и не владельцем таблиц (`FORCE ROW LEVEL SECURITY`). `TenantTx` открывает транзакцию и делает `SET LOCAL app.tenant_id`, поэтому контекст не переживает транзакцию и не утекает через пул. Без контекста `current_setting(..., true)` = NULL → ноль строк. Миграции и платформенные запросы идут отдельной ролью/пулом. Альтернативы (схема/БД на тенанта) отклонены пользователем.

**Где живут тенанты и пользователи.** Реестр тенантов — БД `admin` (таблица без RLS, доступ только admin-сервису). Пользователи тенантов — БД `identity`, `users`/`refresh_tokens` с RLS; межсервисных FK нет. Платформенные администраторы — отдельная таблица в БД `admin`, отдельные ключ/audience JWT.

**Вход без известного тенанта.** Логин требует тенанта: пользователь вводит slug (или заходит по поддомену `{slug}.host`; BFF выделяет slug из Host). identity по slug получает `tenant_id` и статус: приостановка проверяется запросом к admin (internal endpoint `GET /internal/tenants/{slug}`) с коротким кэшем в identity. Альтернатива «глобальный email» отклонена: ломает кейс одного email в разных тенантах.

**Провижининг тенанта.** admin: `INSERT tenant(status=provisioning)` → вызов identity `POST /internal/tenants/{id}/owner` → `status=active`; при ошибке — `failed`/удаление записи (спека: не остаётся активным). Без распределённых транзакций и без шины: сага из двух шагов, идемпотентная по `tenant_id`. Internal-эндпоинты защищены сервисным токеном (shared secret в конфиге на старте; mTLS — позже).

**Токены.** Access JWT (RS256 или EdDSA, ~10 мин) с `sub`, `tid`, `roles`; identity подписывает, остальные проверяют по публичному ключу (JWKS endpoint). Refresh — случайная строка, в БД хранится хеш, ротация при каждом обмене. BFF хранит токены в Redis под session id в `HttpOnly; Secure; SameSite=Lax` cookie; браузер JWT не видит.

**BFF: два, по одному на фронтенд.** `bff-crm` (пользователи тенантов) и `bff-admin` (платформенные администраторы) — отдельные процессы на общем `bff-core`. Различаются: allowlist проксируемых маршрутов (crm → identity; admin → admin и identity-internal не наружу), audience токенов, имя cookie и префикс ключей в Redis, CORS/origin. Так компрометация или падение одного слоя не затрагивает другой, а admin-поверхность не присутствует в публичном клиентском процессе. Альтернатива «один BFF с двумя наборами маршрутов» отклонена: раздельные слои по требованию. Общий код — в `bff-core`, чтобы не дублировать сессии, CSRF и refresh. Сессия, CSRF-проверка, прозрачный refresh и выставление `Authorization` вниз по цепочке — в `bff-core`; доменной агрегации пока нет.

**Web: два приложения.** `web-crm` (вход со slug, оболочка) и `web-admin` (вход администратора, тенанты) — отдельные Leptos-приложения и origin'ы, общие компоненты в `ui-kit`. Server functions и запросы идут только в свой BFF. DTO берутся из `*-contracts` напрямую, без кодогенерации клиента — преимущество «фронт тоже Rust». Альтернатива «одно приложение с /admin» отклонена: admin-код не должен попадать в клиентский бандл.

**Локальный запуск.** `docker-compose.yml` (PostgreSQL, Redis), `Makefile`/`justfile` с целями up, migrate, dev. Порты и строки подключения — через env/`config` crate.

**Тесты.** unit на домен; интеграционные на реальном PostgreSQL (testcontainers или compose-БД), в том числе обязательные тесты RLS из спеки tenancy; API-тесты сервисов через `tower::ServiceExt`. CI: `fmt --check`, `clippy -D warnings`, `test`.

## Risks / Trade-offs

- [Leptos + cargo-leptos молодая экосистема, нет готовых UI-kit] → скелет ограничен оболочкой и формами; свой минимальный набор компонентов в `crates/ui-kit`.
- [RLS: забытая политика на новой таблице] → тест в CI, обходящий все таблицы со столбцом `tenant_id` и проверяющий включённый RLS.
- [Микросервисы с первого дня усложняют локальную разработку без Aspire] → `just dev` запускает все процессы; сервисы стартуют независимо, зависимость только на identity→admin по HTTP.
- [Связность identity→admin по HTTP при логине] → кэш статуса тенанта (TTL десятки секунд); приостановка вступает с задержкой не более TTL — зафиксировано в спеке как «блокирует новые входы».
- [Сага провижининга без шины] → идемпотентные internal-эндпоинты и статус `provisioning`, который можно дозавершить повтором.

## Open Questions

- Выбор подписи JWT (RS256 vs EdDSA) — не влияет на спеки и задачи; решается при реализации.
- Схема именования поддоменов/slug в продакшене — не влияет на скелет (локально slug вводится в форме).
