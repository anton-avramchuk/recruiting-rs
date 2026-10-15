//! DTO и общие правила сервиса recruitment: для сервера, BFF и Rust-фронтенда.
//!
//! Моменты времени - RFC 3339, даты - `YYYY-MM-DD`: контракты собираются и в wasm,
//! где не нужны ни chrono, ни источник случайности.
mod dto;
mod stage;

pub use stage::{Stage, match_percent};

mod vacancy;

pub use vacancy::{HR_SYSTEMS, Priority};

pub use dto::*;
