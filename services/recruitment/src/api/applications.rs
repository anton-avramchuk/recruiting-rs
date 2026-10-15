use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    routing::{get, post},
};
use platform_auth::{AuthUser, Permission};
use platform_core::error::{ApiError, ProblemDetails};
use recruitment_contracts::{
    ApplicationDetailDto, ApplicationDto, ApplicationPage, BulkRequest, BulkResult,
    CreateApplicationRequest, HandoffRequest, InterviewRequest, MoveRequest, RejectRequest, Stage,
};
use serde::Deserialize;
use uuid::Uuid;

use super::{
    AppState,
    mapping::{application_to_dto, event_to_dto},
    vacancies::tenant_of,
};
use crate::{
    application::{BulkAction, FunnelError, ListQuery, applications::DEFAULT_PAGE_SIZE},
    infrastructure::applications::{ApplicationFilter, Sort},
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/applications", get(list).post(create))
        .route("/applications/bulk", post(bulk))
        .route("/applications/{id}", get(get_one))
        .route("/applications/{id}/stage", post(move_to))
        .route("/applications/{id}/next", post(advance))
        .route("/applications/{id}/reject", post(reject))
        .route("/applications/{id}/interviews", post(interview))
        .route("/applications/{id}/handoff", post(handoff))
}

#[utoipa::path(post, path = "/applications", security(("bearer" = [])), request_body = CreateApplicationRequest, responses(
    (status = 201, body = ApplicationDto),
    (status = 400, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 401, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 403, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 404, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 409, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<CreateApplicationRequest>,
) -> Result<(StatusCode, Json<ApplicationDto>), ApiError> {
    user.require(Permission::WriteData)?;
    let row = state
        .applications
        .create(
            tenant_of(&user)?,
            req.person_id,
            req.vacancy_id,
            req.stage.as_deref(),
        )
        .await?;
    Ok((StatusCode::CREATED, Json(application_to_dto(&row))))
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub(super) struct ListParams {
    vacancy_id: Option<Uuid>,
    person_id: Option<Uuid>,
    /// Ключ этапа: `new`, `screening`, `hr`, `tech`, `final`, `offer`, `hired`, `rejected`.
    stage: Option<String>,
    source: Option<String>,
    /// Поиск по имени, вакансии, городу, навыкам и email.
    q: Option<String>,
    /// `age` (по умолчанию), `name`, `stage`, `match`, `salary`.
    sort: Option<String>,
    /// `asc` или `desc`.
    dir: Option<String>,
    page: Option<i64>,
    page_size: Option<i64>,
}

#[utoipa::path(get, path = "/applications", security(("bearer" = [])), params(ListParams), responses(
    (status = 200, body = ApplicationPage, headers(("x-people-stale" = String, description = "true, если данные людей могут отставать"))),
    (status = 400, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 401, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 403, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn list(
    State(state): State<AppState>,
    user: AuthUser,
    Query(params): Query<ListParams>,
) -> Result<(HeaderMap, Json<ApplicationPage>), ApiError> {
    user.require(Permission::ReadData)?;
    let stage = params
        .stage
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(|s| {
            Stage::from_key(s.trim()).ok_or_else(|| {
                ApiError::bad_request("unknown stage").with_extension("field", "stage".into())
            })
        })
        .transpose()?;
    let sort = match params.sort.as_deref().filter(|s| !s.is_empty()) {
        None => Sort::Age,
        Some(s) => Sort::parse(s).ok_or_else(|| FunnelError::BadQuery("unknown sort".into()))?,
    };
    let descending = match params.dir.as_deref().filter(|s| !s.is_empty()) {
        None => None,
        Some("asc") => Some(false),
        Some("desc") => Some(true),
        Some(_) => return Err(FunnelError::BadQuery("dir must be asc or desc".into()).into()),
    };
    let mut query = ListQuery::new(ApplicationFilter {
        id: None,
        vacancy_id: params.vacancy_id,
        person_id: params.person_id,
        stage,
        source: params.source.filter(|s| !s.trim().is_empty()),
        q: params.q,
    });
    query.sort = sort;
    query.descending = descending;
    query.page = params.page.unwrap_or(1);
    query.page_size = params.page_size.unwrap_or(DEFAULT_PAGE_SIZE);

    let page = state.applications.list(tenant_of(&user)?, query).await?;
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-people-stale",
        HeaderValue::from_static(if page.stale { "true" } else { "false" }),
    );
    Ok((
        headers,
        Json(ApplicationPage {
            items: page.items.iter().map(application_to_dto).collect(),
            total: page.total,
            page: page.page,
            page_size: page.page_size,
        }),
    ))
}

#[utoipa::path(get, path = "/applications/{id}", security(("bearer" = [])), params(("id" = Uuid, Path, description = "Application id")), responses(
    (status = 200, body = ApplicationDetailDto),
    (status = 401, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 403, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 404, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn get_one(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ApplicationDetailDto>, ApiError> {
    user.require(Permission::ReadData)?;
    let detail = state.applications.get(tenant_of(&user)?, id).await?;
    Ok(Json(ApplicationDetailDto {
        application: application_to_dto(&detail.application),
        history: detail.history.iter().map(event_to_dto).collect(),
        person_applications: detail
            .person_applications
            .iter()
            .map(application_to_dto)
            .collect(),
    }))
}

#[utoipa::path(post, path = "/applications/{id}/stage", security(("bearer" = [])), params(("id" = Uuid, Path, description = "Application id")), request_body = MoveRequest, responses(
    (status = 200, body = ApplicationDto),
    (status = 400, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 403, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 404, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn move_to(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<MoveRequest>,
) -> Result<Json<ApplicationDto>, ApiError> {
    user.require(Permission::WriteData)?;
    let row = state
        .applications
        .move_to(tenant_of(&user)?, id, &req.stage)
        .await?;
    Ok(Json(application_to_dto(&row)))
}

#[utoipa::path(post, path = "/applications/{id}/next", security(("bearer" = [])), params(("id" = Uuid, Path, description = "Application id")), responses(
    (status = 200, body = ApplicationDto),
    (status = 403, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 404, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 409, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn advance(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ApplicationDto>, ApiError> {
    user.require(Permission::WriteData)?;
    let row = state.applications.advance(tenant_of(&user)?, id).await?;
    Ok(Json(application_to_dto(&row)))
}

#[utoipa::path(post, path = "/applications/{id}/reject", security(("bearer" = [])), params(("id" = Uuid, Path, description = "Application id")), request_body = RejectRequest, responses(
    (status = 200, body = ApplicationDto),
    (status = 403, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 404, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn reject(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<RejectRequest>,
) -> Result<Json<ApplicationDto>, ApiError> {
    user.require(Permission::WriteData)?;
    let row = state
        .applications
        .reject(tenant_of(&user)?, id, req.reason)
        .await?;
    Ok(Json(application_to_dto(&row)))
}

#[utoipa::path(post, path = "/applications/{id}/interviews", security(("bearer" = [])), params(("id" = Uuid, Path, description = "Application id")), request_body = InterviewRequest, responses(
    (status = 200, body = ApplicationDto),
    (status = 400, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 403, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 404, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn interview(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<InterviewRequest>,
) -> Result<Json<ApplicationDto>, ApiError> {
    user.require(Permission::WriteData)?;
    let row = state
        .applications
        .schedule_interview(tenant_of(&user)?, id, &req.at)
        .await?;
    Ok(Json(application_to_dto(&row)))
}

#[utoipa::path(post, path = "/applications/{id}/handoff", security(("bearer" = [])), params(("id" = Uuid, Path, description = "Application id")), request_body = HandoffRequest, responses(
    (status = 200, body = ApplicationDto),
    (status = 400, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 403, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 404, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 409, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn handoff(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<HandoffRequest>,
) -> Result<Json<ApplicationDto>, ApiError> {
    user.require(Permission::WriteData)?;
    let row = state
        .applications
        .hand_off(tenant_of(&user)?, id, &req.system)
        .await?;
    Ok(Json(application_to_dto(&row)))
}

#[utoipa::path(post, path = "/applications/bulk", security(("bearer" = [])), request_body = BulkRequest, responses(
    (status = 200, body = BulkResult),
    (status = 400, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 403, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn bulk(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<BulkRequest>,
) -> Result<Json<BulkResult>, ApiError> {
    user.require(Permission::WriteData)?;
    let action = BulkAction::parse(&req.action, req.reason).map_err(FunnelError::from)?;
    let outcome = state
        .applications
        .bulk(tenant_of(&user)?, &req.ids, action)
        .await?;
    Ok(Json(BulkResult {
        changed: outcome.changed,
        skipped: outcome.skipped,
    }))
}
