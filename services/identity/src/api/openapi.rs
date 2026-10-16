use axum::{Json, Router, routing::get};
use utoipa::{
    Modify, OpenApi,
    openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
};

use super::{AppState, auth, internal, roles, users};
use crate::application::auth::TokenPair;

#[derive(OpenApi)]
#[openapi(
    info(title = "Identity API", version = "0.1.0"),
    paths(
        auth::login,
        auth::refresh,
        auth::logout,
        users::list,
        users::get_one,
        users::create,
        users::update,
        roles::list,
        roles::create,
        roles::update,
        roles::remove,
        internal::ensure_owner,
        platform_core::health::health,
    ),
    components(schemas(TokenPair, platform_core::error::ProblemDetails)),
    modifiers(&Security)
)]
struct ApiDoc;

struct Security;

impl Modify for Security {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "bearer",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("JWT")
                    .build(),
            ),
        );
        components.add_security_scheme(
            "service",
            SecurityScheme::Http(HttpBuilder::new().scheme(HttpAuthScheme::Bearer).build()),
        );
    }
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/openapi.json", get(|| async { Json(ApiDoc::openapi()) }))
}
