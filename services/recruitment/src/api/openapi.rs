use axum::{Json, Router, routing::get};
use recruitment_contracts::{
    AnalyticsDto, ApplicationDetailDto, ApplicationDto, ApplicationPage, BatchApplicationsRequest,
    BatchApplicationsResponse, BulkRequest, BulkResult, ClientPeopleDto, ClientRefDto,
    ClientStatDto, ContactRefDto, CreateApplicationRequest, CreateVacancyRequest, EventDto,
    FunnelStepDto, HandoffRequest, InterviewItemDto, InterviewRequest, MoveRequest, OverviewDto,
    RejectRequest, SourceStatDto, Stage, StageCount, UpdateVacancyRequest, VacancyDto,
    VacancyLoadDto, WeekCountDto,
};
use utoipa::{
    Modify, OpenApi,
    openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
};

use super::{AppState, analytics, applications, vacancies};

#[derive(OpenApi)]
#[openapi(
    info(title = "Recruitment API", version = "0.1.0"),
    paths(
        vacancies::create,
        vacancies::list,
        vacancies::get_one,
        vacancies::update,
        applications::create,
        applications::list,
        applications::people_of_client,
        applications::get_one,
        applications::move_to,
        applications::advance,
        applications::reject,
        applications::interview,
        applications::handoff,
        applications::bulk,
        applications::batch,
        applications::overview,
        applications::interviews,
        analytics::analytics,
        platform_core::health::health,
    ),
    components(schemas(
        VacancyDto,
        ClientPeopleDto,
        ClientRefDto,
        ClientStatDto,
        ContactRefDto,
        CreateVacancyRequest,
        UpdateVacancyRequest,
        ApplicationDto,
        ApplicationPage,
        ApplicationDetailDto,
        EventDto,
        CreateApplicationRequest,
        MoveRequest,
        RejectRequest,
        InterviewRequest,
        HandoffRequest,
        BulkRequest,
        BatchApplicationsRequest,
        BatchApplicationsResponse,
        BulkResult,
        OverviewDto,
        StageCount,
        InterviewItemDto,
        AnalyticsDto,
        FunnelStepDto,
        WeekCountDto,
        SourceStatDto,
        VacancyLoadDto,
        Stage,
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
