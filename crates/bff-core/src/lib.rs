//! Общий код BFF: серверные сессии, cookie, CSRF, refresh, прокси.
mod bff;
mod config;
mod proxy;
mod session;
mod upstream;

pub use bff::{Bff, CSRF_HEADER, router};
pub use config::{AllowRule, BffConfig, LoginTransform};
pub use session::{
    MemorySessionStore, RedisSessionStore, Session, SessionStore, StoreError, random_token,
};
pub use upstream::{Grant, HttpUpstreamAuth, UpstreamAuth, UpstreamError};
