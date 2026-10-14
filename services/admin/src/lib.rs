//! Сервис admin: реестр тенантов и платформенные администраторы.
use sqlx::migrate::Migrator;

pub mod api;
pub mod application;
pub mod domain;
pub mod infrastructure;

pub static MIGRATOR: Migrator = sqlx::migrate!("./migrations");
