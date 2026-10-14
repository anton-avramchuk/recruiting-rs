//! Хелперы интеграционных тестов. Нужен PostgreSQL (`TEST_DATABASE_URL`, роль-суперпользователь).
use std::str::FromStr;

use sqlx::{
    PgPool,
    migrate::Migrator,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use uuid::Uuid;

pub const APP_ROLE: &str = "recruiting_app_test";
const APP_PASSWORD: &str = "recruiting_app_test";
const DEFAULT_URL: &str = "postgres://postgres:postgres@localhost:5432/postgres";

/// Временная БД с применёнными миграциями.
/// `owner` — суперпользователь (обходит RLS, для засева данных),
/// `app` — непривилегированная роль без `BYPASSRLS`, как в рабочем окружении.
pub struct TestDb {
    pub owner: PgPool,
    pub app: PgPool,
    name: String,
    admin: PgPool,
}

impl TestDb {
    pub async fn create(migrator: &Migrator) -> Self {
        let url = std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| DEFAULT_URL.into());
        let base = PgConnectOptions::from_str(&url).expect("valid TEST_DATABASE_URL");
        let admin = PgPoolOptions::new()
            .max_connections(2)
            .connect_with(base.clone())
            .await
            .expect("connect to test postgres (is docker compose up?)");

        ensure_app_role(&admin).await;

        let name = format!("t_{}", Uuid::new_v4().simple());
        sqlx::raw_sql(&format!("CREATE DATABASE \"{name}\""))
            .execute(&admin)
            .await
            .expect("create test database");

        let owner = PgPoolOptions::new()
            .max_connections(4)
            .connect_with(base.clone().database(&name))
            .await
            .expect("connect owner");
        migrator.run(&owner).await.expect("run migrations");
        sqlx::raw_sql(&format!(
            "GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO {APP_ROLE}"
        ))
        .execute(&owner)
        .await
        .expect("grant to app role");

        let app = PgPoolOptions::new()
            .max_connections(1) // одно соединение: проверяет, что контекст не утекает между запросами
            .connect_with(
                base.database(&name)
                    .username(APP_ROLE)
                    .password(APP_PASSWORD),
            )
            .await
            .expect("connect app role");

        Self {
            owner,
            app,
            name,
            admin,
        }
    }

    /// Удаляет временную БД.
    pub async fn close(self) {
        self.owner.close().await;
        self.app.close().await;
        let _ = sqlx::raw_sql(&format!("DROP DATABASE \"{}\" WITH (FORCE)", self.name))
            .execute(&self.admin)
            .await;
    }
}

async fn ensure_app_role(admin: &PgPool) {
    // Роль общая для кластера; параллельные тесты могут создавать её одновременно.
    let mut conn = admin.acquire().await.expect("acquire");
    sqlx::query("SELECT pg_advisory_lock(727001)")
        .execute(&mut *conn)
        .await
        .expect("lock");
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = $1)")
            .bind(APP_ROLE)
            .fetch_one(&mut *conn)
            .await
            .expect("check role");
    if !exists {
        sqlx::raw_sql(&format!(
            "CREATE ROLE {APP_ROLE} LOGIN PASSWORD '{APP_PASSWORD}' NOSUPERUSER NOBYPASSRLS"
        ))
        .execute(&mut *conn)
        .await
        .expect("create role");
    }
    sqlx::query("SELECT pg_advisory_unlock(727001)")
        .execute(&mut *conn)
        .await
        .expect("unlock");
}

/// Тест-страж: имена таблиц схемы `public` со столбцом `tenant_id`, у которых
/// не включён RLS, не включён FORCE или нет ни одной политики. Сервисы вызывают
/// это в своих интеграционных тестах и ожидают пустой результат.
pub async fn tables_without_rls(pool: &PgPool) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT c.relname::text
         FROM pg_class c
         JOIN pg_namespace n ON n.oid = c.relnamespace AND n.nspname = 'public'
         WHERE c.relkind = 'r'
           AND EXISTS (
               SELECT 1 FROM pg_attribute a
               WHERE a.attrelid = c.oid AND a.attname = 'tenant_id' AND NOT a.attisdropped)
           AND NOT (
               c.relrowsecurity AND c.relforcerowsecurity
               AND EXISTS (SELECT 1 FROM pg_policy p WHERE p.polrelid = c.oid))
         ORDER BY c.relname",
    )
    .fetch_all(pool)
    .await
    .expect("query rls status")
}
