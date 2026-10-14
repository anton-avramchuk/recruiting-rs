# recruiting-rs

Мультитенантная рекрутинговая CRM целиком на Rust, включая фронтенд (Leptos). Это скелет платформы:
организации (тенанты), пользователи, вход, админка платформы и оболочка приложения.
Доменные сущности (кандидаты, вакансии, интервью) добавляются отдельными изменениями.

## Архитектура

```
браузер ──► web-crm ──► bff-crm ──► identity ──┐
браузер ──► web-admin ► bff-admin ► admin ◄────┘ (справочник тенантов, создание владельца)
```

| Компонент | Назначение |
|---|---|
| `services/identity` | пользователи тенантов, вход, JWT (EdDSA), refresh с ротацией, роли; БД `identity` |
| `services/admin` | реестр тенантов, платформенные администраторы, жизненный цикл тенанта; БД `admin` |
| `services/bff-crm`, `services/bff-admin` | по BFF на каждый фронтенд: серверная сессия в Redis, cookie `HttpOnly`, CSRF, прозрачный refresh, прокси по allowlist |
| `apps/web-crm`, `apps/web-admin` | Leptos (SSR + hydrate); каждое приложение общается только со своим BFF |
| `crates/platform-core` | конфиг из env, ошибки `problem+json`, health, shutdown, `TenantId` |
| `crates/platform-db` | пул, миграции, `TenantTx` (контекст тенанта для RLS), помощники тестов |
| `crates/platform-auth` | JWT, JWKS, роли и права, extractor, межсервисный токен, пароли (argon2) |
| `crates/bff-core` | общий код обоих BFF |
| `crates/ui-kit` | общие Leptos-компоненты и server functions входа/сессии |
| `crates/admin-contracts` | DTO сервиса admin, общие для сервера и фронтенда |
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
docker compose up -d --wait        # PostgreSQL (БД identity, admin, роль recruiting_app) и Redis
cargo run -p e2e --example dev     # вся платформа на свободных портах, одноразовые данные
```

Команда соберёт сервисы и веб-приложения, поднимет их, создаст демо-организацию и напечатает адреса
и учётные данные:

- клиентское приложение: организация `demo`, `owner@demo.io` / `demo-password-1`
- админка платформы: `root@platform.io` / `root-password-1`

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
| admin | `DATABASE_URL`, `IDENTITY_URL`, `SERVICE_TOKEN` | `MIGRATION_DATABASE_URL`, `JWT_PRIVATE_KEY`, `SEED_ADMIN_EMAIL`, `SEED_ADMIN_PASSWORD`, `LISTEN_ADDR` |
| bff-crm | `IDENTITY_URL` | `REDIS_URL`, `COOKIE_SECURE`, `BASE_DOMAIN`, `SESSION_TTL_HOURS`, `LISTEN_ADDR` |
| bff-admin | `ADMIN_URL` | `REDIS_URL`, `COOKIE_SECURE`, `SESSION_TTL_HOURS`, `LISTEN_ADDR` |
| web-crm / web-admin | `BFF_URL`, переменные `LEPTOS_*` | — |

- Миграции применяются при старте от `MIGRATION_DATABASE_URL` (владелец схемы), а запросы обслуживаются
  от `DATABASE_URL` — роли без `BYPASSRLS` (в compose это `recruiting_app`).
- Без `JWT_PRIVATE_KEY` сервис генерирует временный ключ: токены перестают действовать после перезапуска.
- `COOKIE_SECURE=false` нужен только при работе по http (локально).
- Веб-приложения собираются `cargo leptos build --project web-crm` (и `web-admin`).

## Структура сервиса

Каждый сервис — один crate с модулями `domain` (агрегаты и инварианты), `application` (сценарии и порты),
`infrastructure` (PostgreSQL, HTTP-клиенты), `api` (axum-маршруты, OpenAPI на `/openapi.json`).
Новый сервис копирует эту форму.
