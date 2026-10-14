use sqlx::{PgPool, migrate::Migrator, postgres::PgPoolOptions};

pub async fn connect(url: &str, max_connections: u32) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(max_connections)
        .connect(url)
        .await
}

/// Применяет миграции сервиса (вызывается при старте от роли-владельца схемы).
pub async fn migrate(
    pool: &PgPool,
    migrator: &Migrator,
) -> Result<(), sqlx::migrate::MigrateError> {
    migrator.run(pool).await
}
