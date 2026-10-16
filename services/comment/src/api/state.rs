use std::sync::Arc;

use axum::extract::FromRef;
use platform_auth::TokenVerifier;
use sqlx::PgPool;

use crate::application::CommentService;

#[derive(Clone)]
pub struct AppState {
    pub comments: CommentService,
    pub verifier: Arc<TokenVerifier>,
}

impl AppState {
    /// Сервисный токен не нужен (внутренних эндпоинтов нет); аргумент оставлен ради единообразия
    /// запуска с остальными сервисами.
    pub fn new(
        pool: PgPool,
        verifier: Arc<TokenVerifier>,
        _service_token: impl Into<String>,
    ) -> Self {
        Self {
            comments: CommentService::new(pool),
            verifier,
        }
    }
}

impl FromRef<AppState> for Arc<TokenVerifier> {
    fn from_ref(state: &AppState) -> Self {
        state.verifier.clone()
    }
}
