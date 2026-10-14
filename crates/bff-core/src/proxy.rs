//! Проксирование `/api/*` в бэкенд с подстановкой токена сессии.
use axum::Router;

use crate::bff::Bff;

pub(crate) fn routes() -> Router<Bff> {
    Router::new()
}
