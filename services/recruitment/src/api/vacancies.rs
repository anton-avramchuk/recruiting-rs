use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
};
use platform_auth::{AuthUser, Permission};
use platform_core::{
    error::{ApiError, ProblemDetails},
    tenant::TenantId,
};
use recruitment_contracts::{CreateVacancyRequest, UpdateVacancyRequest, VacancyDto};
use serde::Deserialize;
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
                client_id: req.client_id,
                contact_id: req.contact_id,
                city: req.city,
                openings: req.openings,
                priority: req.priority,
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

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub(super) struct ListParams {
    /// Только вакансии этого заказчика.
    client_id: Option<Uuid>,
}

#[utoipa::path(get, path = "/vacancies", security(("bearer" = [])), params(ListParams), responses(
    (status = 200, body = Vec<VacancyDto>),
    (status = 401, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 403, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn list(
    State(state): State<AppState>,
    user: AuthUser,
    Query(params): Query<ListParams>,
) -> Result<Json<Vec<VacancyDto>>, ApiError> {
    user.require(Permission::ReadData)?;
    let today = state.vacancies.clock().today();
    let items = state
        .vacancies
        .list(tenant_of(&user)?, params.client_id)
        .await?;
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
                client_id: req.client_id,
                contact_id: req.contact_id,
                city: req.city,
                openings: req.openings,
                priority: req.priority,
                recruiter: req.recruiter,
                required_skills: req.required_skills,
                open_days: req.open_days,
            },
        )
        .await?;
    Ok(Json(vacancy_to_dto(&view, state.vacancies.clock().today())))
}
