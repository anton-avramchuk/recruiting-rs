//! Общая платформенная инфраструктура сервисов: конфиг, ошибки, health, shutdown.
pub mod config;
pub mod error;
pub mod health;
pub mod shutdown;
pub mod telemetry;
pub mod tenant;
