//! Сервис recruitment: вакансии, отклики кандидатов и воронка найма.
use sqlx::migrate::Migrator;

pub mod api;
pub mod application;
pub mod domain;
pub mod infrastructure;

pub static MIGRATOR: Migrator = sqlx::migrate!("./migrations");
