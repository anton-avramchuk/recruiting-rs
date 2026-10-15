# recruiting-rs

Мультитенантная рекрутинговая CRM целиком на Rust, включая фронтенд (Leptos). Платформа: организации (тенанты),
пользователи, вход, админка платформы, оболочка приложения («Ритм»), **кандидаты** (карточка с
контактами и историей, поиск и фильтры, предупреждение о дубле) и **воронка найма**: вакансии, отклики,
доска по этапам с перетаскиванием, таблица откликов с групповыми действиями и панель человека. Импорт
файлов, режим разбора, аналитика и палитра команд добавляются отдельными изменениями.

## Архитектура

```
браузер ──► web-crm ──► bff-crm ──► identity ──┐
                              ├──► candidate (токены проверяет по JWKS identity)
                              └──► recruitment ──► candidate (лента изменений людей)
браузер ──► web-admin ► bff-admin ► admin ◄────┘ (справочник тенантов, создание владельца)
```

| Компонент | Назначение |
|---|---|
| `services/identity` | пользователи тенантов, вход, JWT (EdDSA), refresh с ротацией, роли; БД `identity` |
| `services/candidate` | кандидаты организации: контакты, места работы, образование, стаж, поиск, предупреждение о дубле; БД `candidate` |
| `services/recruitment` | вакансии, отклики и воронка (8 этапов, SLA, совпадение по навыкам, история, групповые действия); БД `recruitment`; людей берёт из `candidate` (см. ниже) |
| `services/admin` | реестр тенантов, платформенные администраторы, жизненный цикл тенанта; БД `admin` |
| `services/bff-crm`, `services/bff-admin` | по BFF на каждый фронтенд: серверная сессия в Redis, cookie `HttpOnly`, CSRF, прозрачный refresh, прокси по allowlist |
| `apps/web-crm`, `apps/web-admin` | Leptos (SSR + hydrate); каждое приложение общается только со своим BFF |
| `crates/platform-core` | конфиг из env, ошибки `problem+json`, health, shutdown, `TenantId` |
| `crates/platform-db` | пул, миграции, `TenantTx` (контекст тенанта для RLS), помощники тестов |
| `crates/platform-auth` | JWT, JWKS (в том числе удалённый верификатор), роли и права, extractor, межсервисный токен, пароли (argon2) |
| `crates/platform-values` | значимые типы `Email` и `Phone` с нормализацией |
| `crates/bff-core` | общий код обоих BFF |
| `crates/ui-kit` | общие Leptos-компоненты и server functions входа/сессии |
| `crates/admin-contracts`, `crates/candidate-contracts`, `crates/recruitment-contracts` | DTO сервисов, общие для сервера и фронтенда (собираются и в wasm) |
| `tests/e2e` | запуск платформы процессами и сквозные тесты, в том числе в браузере |

Изоляция тенантов: общая БД сервиса, колонка `tenant_id` и Row-Level Security. Правила для новых
таблиц — в [docs/tenancy.md](docs/tenancy.md). Проектные решения и спецификации — в `openspec/`.

Правила разработки: TDD (сначала падающий тест), DDD (инварианты в `domain` без зависимостей от HTTP и БД),
общий код — в общие crates.

## Требования

- Rust (стабильный, см. `rust-toolchain.toml`) и цель `wasm32-unknown-unknown`:
  `rustup target add wasm32-unknown-unknown`
- Docker (PostgreSQL 17 и Redis)
- `cargo install cargo-leptos --locked` — сборка веб-приложений
- Google Chrome — для браузерных e2e-тестов
- по желанию `just` — короткие команды из `justfile`

## Быстрый старт

```sh
docker compose up -d --wait        # PostgreSQL (БД identity, admin, candidate, recruitment; роль recruiting_app) и Redis
cargo run -p e2e --example dev     # вся платформа на свободных портах, одноразовые данные
```

Команда соберёт сервисы и веб-приложения, поднимет их, создаст демо-организацию и напечатает адреса
и учётные данные:

- клиентское приложение: организация `demo`, `owner@demo.io` / `demo-password-1` (с тремя демо-кандидатами, двумя вакансиями и откликами)
- админка платформы: `root@platform.io` / `root-password-1`

## В Docker

Вся платформа целиком, с демо-данными:

```sh
docker compose --profile app up -d --build   # первая сборка - несколько минут
```

- клиентское приложение: <http://localhost:3000>, организация `demo`, `owner@demo.io` / `demo-password-1`
- админка платформы: <http://localhost:3010>, `root@platform.io` / `root-password-1`
- PostgreSQL `localhost:5432`, Redis `localhost:6379`

Одноразовый контейнер `demo-seed` создаёт организацию `demo` и трёх кандидатов, двух вакансий и четырёх откликов (повторный запуск безопасен:
`docker compose --profile app run --rm demo-seed`). Остановить: `docker compose --profile app down`
(`-v` дополнительно удалит данные). Учётные данные и `SERVICE_TOKEN` в `docker-compose.yml` - только для
локального просмотра. Без профиля `app` поднимаются лишь PostgreSQL и Redis (их используют тесты).

## Тесты

```sh
docker compose up -d --wait
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace             # юнит, интеграционные (реальный PostgreSQL и Redis), e2e
```

Интеграционные тесты создают временные БД и подключаются ролью без `BYPASSRLS`, поэтому проверяют
реальную изоляцию. Адреса по умолчанию: `TEST_DATABASE_URL=postgres://postgres:postgres@localhost:5432/postgres`,
`TEST_REDIS_URL=redis://127.0.0.1:6379`. Браузерные тесты и `cargo leptos build` требуют установленных
Chrome и cargo-leptos; первый запуск e2e собирает все приложения (несколько минут).

## Запуск сервисов вручную

Каждый сервис настраивается переменными окружения.

| Сервис | Обязательные | Необязательные |
|---|---|---|
| identity | `DATABASE_URL`, `ADMIN_URL`, `SERVICE_TOKEN` | `MIGRATION_DATABASE_URL`, `JWT_PRIVATE_KEY`, `ACCESS_TTL_SECS`, `TENANT_CACHE_TTL_SECS`, `LISTEN_ADDR` |
| candidate | `DATABASE_URL`, `IDENTITY_JWKS_URL`, `SERVICE_TOKEN` | `MIGRATION_DATABASE_URL`, `LISTEN_ADDR` |
| recruitment | `DATABASE_URL`, `IDENTITY_JWKS_URL`, `SERVICE_TOKEN`, `CANDIDATE_URL` | `MIGRATION_DATABASE_URL`, `LISTEN_ADDR` |
| admin | `DATABASE_URL`, `IDENTITY_URL`, `SERVICE_TOKEN` | `MIGRATION_DATABASE_URL`, `JWT_PRIVATE_KEY`, `SEED_ADMIN_EMAIL`, `SEED_ADMIN_PASSWORD`, `LISTEN_ADDR` |
| bff-crm | `IDENTITY_URL`, `CANDIDATE_URL`, `RECRUITMENT_URL` | `REDIS_URL`, `COOKIE_SECURE`, `BASE_DOMAIN`, `SESSION_TTL_HOURS`, `LISTEN_ADDR` |
| bff-admin | `ADMIN_URL` | `REDIS_URL`, `COOKIE_SECURE`, `SESSION_TTL_HOURS`, `LISTEN_ADDR` |
| web-crm / web-admin | `BFF_URL`, переменные `LEPTOS_*` | — |

- Миграции применяются при старте от `MIGRATION_DATABASE_URL` (владелец схемы), а запросы обслуживаются
  от `DATABASE_URL` — роли без `BYPASSRLS` (в compose это `recruiting_app`).
- Без `JWT_PRIVATE_KEY` сервис генерирует временный ключ: токены перестают действовать после перезапуска.
- `candidate` проверяет токены по ключу identity (`IDENTITY_JWKS_URL`, например `http://127.0.0.1:8080/.well-known/jwks.json`): ключ загружается лениво и перечитывается при смене, поэтому сервис стартует и без identity (запросы получают 401).
- `COOKIE_SECURE=false` нужен только при работе по http (локально).
- Веб-приложения собираются `cargo leptos build --project web-crm` (и `web-admin`).

## Воронка и данные людей

`recruitment` хранит вакансии и отклики, а людей (имя, источник, город, ожидания, навыки, стаж) держит у себя
**проекцией** — копией, нужной для фильтров, поиска и сортировки одним SQL-запросом. Проекция обновляется при
каждом чтении по ленте изменений `candidate` (`GET /internal/tenants/{id}/person-changes?after=`, сервисный
токен): кандидат создан или изменён — воронка видит это сразу, а при недоступности `candidate` отвечает по
последним известным данным (заголовок `x-people-stale: true`). Удаление кандидата удаляет его отклики.
Письма, приглашения на интервью и передача вышедшего в кадровую систему **имитируются записью в историю
отклика** — реальных интеграций нет. Подробности — в `openspec/changes/add-hiring-funnel/design.md`.

## Структура сервиса

Каждый сервис — один crate с модулями `domain` (агрегаты и инварианты), `application` (сценарии и порты),
`infrastructure` (PostgreSQL, HTTP-клиенты), `api` (axum-маршруты, OpenAPI на `/openapi.json`).
Новый сервис копирует эту форму.
