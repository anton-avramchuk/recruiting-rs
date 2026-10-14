//! Сервис identity: пользователи тенантов, вход, токены, роли.
use sqlx::migrate::Migrator;

pub static MIGRATOR: Migrator = sqlx::migrate!("./migrations");
