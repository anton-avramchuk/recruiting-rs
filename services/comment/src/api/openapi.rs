use axum::{Json, Router, routing::get};
use comment_contracts::{
    CommentDto, CommentPage, CreateCommentRequest, TargetType, UpdateCommentRequest,
};
use utoipa::{
    Modify, OpenApi,
    openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
};

use super::{AppState, comments};

#[derive(OpenApi)]
#[openapi(
    info(title = "Comment API", version = "0.1.0"),
    paths(
        comments::create,
        comments::list,
        comments::update,
        comments::remove,
        platform_core::health::health,
    ),
    components(schemas(
        CommentDto,
        CommentPage,
        CreateCommentRequest,
        UpdateCommentRequest,
        TargetType,
        platform_core::error::ProblemDetails,
    )),
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
    }
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/openapi.json", get(|| async { Json(ApiDoc::openapi()) }))
}
