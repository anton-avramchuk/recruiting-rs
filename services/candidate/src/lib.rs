//! Сервис candidate: кандидаты организаций, их контакты и история.
use sqlx::migrate::Migrator;

pub static MIGRATOR: Migrator = sqlx::migrate!("./migrations");
