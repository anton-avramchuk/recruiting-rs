use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::get,
};
use platform_auth::{AuthUser, Permission};
use platform_core::{
    error::{ApiError, ProblemDetails},
    tenant::TenantId,
};
use recruitment_contracts::{CreateVacancyRequest, UpdateVacancyRequest, VacancyDto};
use uuid::Uuid;

use super::{AppState, mapping::vacancy_to_dto};
use crate::domain::{NewVacancy, VacancyPatch};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/vacancies", get(list).post(create))
        .route("/vacancies/{id}", get(get_one).patch(update))
}

/// Тенант берётся только из токена, никогда из запроса.
pub(super) fn tenant_of(user: &AuthUser) -> Result<TenantId, ApiError> {
    user.claims.tid.ok_or_else(ApiError::forbidden)
}

#[utoipa::path(post, path = "/vacancies", security(("bearer" = [])), request_body = CreateVacancyRequest, responses(
    (status = 201, body = VacancyDto),
    (status = 400, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 401, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 403, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<CreateVacancyRequest>,
) -> Result<(StatusCode, Json<VacancyDto>), ApiError> {
    user.require(Permission::WriteData)?;
    let view = state
        .vacancies
        .create(
            tenant_of(&user)?,
            NewVacancy {
                title: req.title,
                department: req.department,
                city: req.city,
                openings: req.openings,
                priority: req.priority,
                hiring_manager: req.hiring_manager,
                recruiter: req.recruiter,
                required_skills: req.required_skills,
                open_days: req.open_days,
            },
        )
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(vacancy_to_dto(&view, state.vacancies.clock().today())),
    ))
}

#[utoipa::path(get, path = "/vacancies", security(("bearer" = [])), responses(
    (status = 200, body = Vec<VacancyDto>),
    (status = 401, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 403, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn list(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<Vec<VacancyDto>>, ApiError> {
    user.require(Permission::ReadData)?;
    let today = state.vacancies.clock().today();
    let items = state.vacancies.list(tenant_of(&user)?).await?;
    Ok(Json(
        items.iter().map(|v| vacancy_to_dto(v, today)).collect(),
    ))
}

#[utoipa::path(get, path = "/vacancies/{id}", security(("bearer" = [])), params(("id" = Uuid, Path, description = "Vacancy id")), responses(
    (status = 200, body = VacancyDto),
    (status = 401, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 403, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 404, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn get_one(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<VacancyDto>, ApiError> {
    user.require(Permission::ReadData)?;
    let view = state.vacancies.get(tenant_of(&user)?, id).await?;
    Ok(Json(vacancy_to_dto(&view, state.vacancies.clock().today())))
}

#[utoipa::path(patch, path = "/vacancies/{id}", security(("bearer" = [])), params(("id" = Uuid, Path, description = "Vacancy id")), request_body = UpdateVacancyRequest, responses(
    (status = 200, body = VacancyDto),
    (status = 400, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 401, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 403, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 404, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn update(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateVacancyRequest>,
) -> Result<Json<VacancyDto>, ApiError> {
    user.require(Permission::WriteData)?;
    let view = state
        .vacancies
        .update(
            tenant_of(&user)?,
            id,
            VacancyPatch {
                title: req.title,
                department: req.department,
                city: req.city,
                openings: req.openings,
                priority: req.priority,
                hiring_manager: req.hiring_manager,
                recruiter: req.recruiter,
                required_skills: req.required_skills,
                open_days: req.open_days,
            },
        )
        .await?;
    Ok(Json(vacancy_to_dto(&view, state.vacancies.clock().today())))
}
