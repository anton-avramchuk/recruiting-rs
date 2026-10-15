use axum::{Json, Router, routing::get};
use candidate_contracts::{
    CreatePersonRequest, DuplicateInfo, EducationDto, EmploymentDto, PersonChange,
    PersonChangesPage, PersonDto, PersonPage, PersonSnapshot, PersonSummaryDto,
    UpdatePersonRequest,
};
use utoipa::{
    Modify, OpenApi,
    openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
};

use super::{AppState, internal, persons};

#[derive(OpenApi)]
#[openapi(
    info(title = "Candidate API", version = "0.1.0"),
    paths(
        persons::create,
        persons::list,
        persons::get_one,
        persons::update,
        persons::replace_employments,
        persons::replace_educations,
        persons::remove,
        internal::person_changes,
        platform_core::health::health,
    ),
    components(schemas(
        PersonDto,
        PersonSummaryDto,
        PersonPage,
        CreatePersonRequest,
        UpdatePersonRequest,
        EmploymentDto,
        EducationDto,
        DuplicateInfo,
        PersonChange,
        PersonChangesPage,
        PersonSnapshot,
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
            "service",
            SecurityScheme::Http(HttpBuilder::new().scheme(HttpAuthScheme::Bearer).build()),
        );
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
