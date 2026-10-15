use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post, put},
};
use candidate_contracts::{
    CreatePersonRequest, DuplicateInfo, EducationDto, EmploymentDto, PersonDto, PersonPage,
    UpdatePersonRequest,
};
use platform_auth::{AuthUser, Permission};
use platform_core::{
    error::{ApiError, ProblemDetails},
    tenant::TenantId,
};
use serde::Deserialize;
use uuid::Uuid;

use super::{
    AppState,
    mapping::{educations_from_dto, employments_from_dto, person_to_dto, summary_to_dto},
};
use crate::{
    application::{
        ContactsPatch, ListQuery, PersonError, UpdatePerson, persons::DEFAULT_PAGE_SIZE,
    },
    domain::{ContactsInput, DomainError, NewPerson, ProfilePatch},
    infrastructure::persons::ListFilter,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/persons", post(create).get(list))
        .route("/persons/{id}", get(get_one).patch(update).delete(remove))
        .route("/persons/{id}/employments", put(replace_employments))
        .route("/persons/{id}/educations", put(replace_educations))
}

/// Тенант берётся только из токена, никогда из запроса.
fn tenant_of(user: &AuthUser) -> Result<TenantId, ApiError> {
    user.claims.tid.ok_or_else(ApiError::forbidden)
}

#[utoipa::path(post, path = "/persons", security(("bearer" = [])), request_body = CreatePersonRequest, responses(
    (status = 201, body = PersonDto),
    (status = 400, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 401, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 403, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 409, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<CreatePersonRequest>,
) -> Result<(StatusCode, Json<PersonDto>), ApiError> {
    user.require(Permission::WriteData)?;
    let person = state
        .persons
        .create(
            tenant_of(&user)?,
            NewPerson {
                full_name: req.full_name,
                contacts: ContactsInput {
                    phone: req.phone,
                    email: req.email,
                    telegram: req.telegram,
                    whatsapp: req.whatsapp,
                },
                source: req.source,
                expected_salary: req.expected_salary,
                summary: req.summary,
                skills: req.skills,
                declared_experience_years: req.declared_experience_years,
                city: req.city,
                note: req.note,
            },
            req.confirm_duplicate,
        )
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(person_to_dto(&person, state.persons.today())),
    ))
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub(super) struct ListParams {
    q: Option<String>,
    source: Option<String>,
    skill: Option<String>,
    min_experience: Option<i32>,
    max_experience: Option<i32>,
    min_salary: Option<i64>,
    max_salary: Option<i64>,
    page: Option<i64>,
    page_size: Option<i64>,
}

#[utoipa::path(get, path = "/persons", security(("bearer" = [])), params(ListParams), responses(
    (status = 200, body = PersonPage),
    (status = 400, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 401, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 403, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn list(
    State(state): State<AppState>,
    user: AuthUser,
    Query(params): Query<ListParams>,
) -> Result<Json<PersonPage>, ApiError> {
    user.require(Permission::ReadData)?;
    let page = state
        .persons
        .list(
            tenant_of(&user)?,
            ListQuery {
                filter: ListFilter {
                    q: params.q,
                    source: params.source.filter(|s| !s.trim().is_empty()),
                    skill: params.skill.filter(|s| !s.trim().is_empty()),
                    min_experience: params.min_experience,
                    max_experience: params.max_experience,
                    min_salary: params.min_salary,
                    max_salary: params.max_salary,
                },
                page: params.page.unwrap_or(1),
                page_size: params.page_size.unwrap_or(DEFAULT_PAGE_SIZE),
            },
        )
        .await?;
    Ok(Json(PersonPage {
        items: page.items.into_iter().map(summary_to_dto).collect(),
        total: page.total,
        page: page.page,
        page_size: page.page_size,
    }))
}

#[utoipa::path(get, path = "/persons/{id}", security(("bearer" = [])), params(("id" = Uuid, Path, description = "Person id")), responses(
    (status = 200, body = PersonDto),
    (status = 401, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 403, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 404, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn get_one(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<PersonDto>, ApiError> {
    user.require(Permission::ReadData)?;
    let person = state.persons.get(tenant_of(&user)?, id).await?;
    Ok(Json(person_to_dto(&person, state.persons.today())))
}

#[utoipa::path(patch, path = "/persons/{id}", security(("bearer" = [])), params(("id" = Uuid, Path, description = "Person id")), request_body = UpdatePersonRequest, responses(
    (status = 200, body = PersonDto),
    (status = 400, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 401, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 403, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 404, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn update(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdatePersonRequest>,
) -> Result<Json<PersonDto>, ApiError> {
    user.require(Permission::WriteData)?;
    let person = state
        .persons
        .update(
            tenant_of(&user)?,
            id,
            UpdatePerson {
                profile: ProfilePatch {
                    full_name: req.full_name,
                    source: req.source,
                    expected_salary: req.expected_salary,
                    summary: req.summary,
                    skills: req.skills,
                    city: req.city,
                    note: req.note,
                },
                contacts: ContactsPatch {
                    phone: req.phone,
                    email: req.email,
                    telegram: req.telegram,
                    whatsapp: req.whatsapp,
                },
            },
        )
        .await?;
    Ok(Json(person_to_dto(&person, state.persons.today())))
}

#[utoipa::path(put, path = "/persons/{id}/employments", security(("bearer" = [])), params(("id" = Uuid, Path, description = "Person id")), request_body = Vec<EmploymentDto>, responses(
    (status = 200, body = PersonDto),
    (status = 400, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 401, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 403, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 404, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn replace_employments(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(items): Json<Vec<EmploymentDto>>,
) -> Result<Json<PersonDto>, ApiError> {
    user.require(Permission::WriteData)?;
    let employments = employments_from_dto(items).map_err(PersonError::from)?;
    let person = state
        .persons
        .replace_employments(tenant_of(&user)?, id, employments)
        .await?;
    Ok(Json(person_to_dto(&person, state.persons.today())))
}

#[utoipa::path(put, path = "/persons/{id}/educations", security(("bearer" = [])), params(("id" = Uuid, Path, description = "Person id")), request_body = Vec<EducationDto>, responses(
    (status = 200, body = PersonDto),
    (status = 400, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 401, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 403, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 404, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn replace_educations(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(items): Json<Vec<EducationDto>>,
) -> Result<Json<PersonDto>, ApiError> {
    user.require(Permission::WriteData)?;
    let educations = educations_from_dto(items).map_err(PersonError::from)?;
    let person = state
        .persons
        .replace_educations(tenant_of(&user)?, id, educations)
        .await?;
    Ok(Json(person_to_dto(&person, state.persons.today())))
}

#[utoipa::path(delete, path = "/persons/{id}", security(("bearer" = [])), params(("id" = Uuid, Path, description = "Person id")), responses(
    (status = 204, description = "Person and history deleted"),
    (status = 401, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 403, body = ProblemDetails, content_type = "application/problem+json"),
    (status = 404, body = ProblemDetails, content_type = "application/problem+json"),
))]
pub(super) async fn remove(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    user.require(Permission::DeleteData)?;
    state.persons.delete(tenant_of(&user)?, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

impl From<PersonError> for ApiError {
    fn from(e: PersonError) -> Self {
        match e {
            PersonError::Domain(DomainError::Invalid { field, reason }) => {
                ApiError::bad_request(reason).with_extension("field", field.into())
            }
            PersonError::Duplicate(d) => {
                let info = DuplicateInfo {
                    id: d.id,
                    full_name: d.full_name,
                    matched_by: d.matched_by.as_str().to_string(),
                };
                ApiError::conflict("possible duplicate")
                    .with_extension("duplicate", serde_json::to_value(info).unwrap_or_default())
            }
            PersonError::NotFound => ApiError::not_found(),
            PersonError::BadQuery(m) => ApiError::bad_request(m),
            PersonError::Db(e) => ApiError::internal(&e),
            PersonError::Store(e) => ApiError::internal(&e),
        }
    }
}
