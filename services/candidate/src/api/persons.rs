use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
};
use candidate_contracts::{CreatePersonRequest, DuplicateInfo, PersonDto, PersonPage};
use platform_auth::{AuthUser, Permission};
use platform_core::{error::ApiError, tenant::TenantId};
use serde::Deserialize;
use uuid::Uuid;

use super::{
    AppState,
    mapping::{person_to_dto, summary_to_dto},
};
use crate::{
    application::{ListQuery, PersonError, persons::DEFAULT_PAGE_SIZE},
    domain::{ContactsInput, DomainError, NewPerson},
    infrastructure::persons::ListFilter,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/persons", post(create).get(list))
        .route("/persons/{id}", get(get_one))
}

/// Тенант берётся только из токена, никогда из запроса.
fn tenant_of(user: &AuthUser) -> Result<TenantId, ApiError> {
    user.claims.tid.ok_or_else(ApiError::forbidden)
}

async fn create(
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
            },
            req.confirm_duplicate,
        )
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(person_to_dto(&person, state.persons.today())),
    ))
}

#[derive(Debug, Deserialize)]
struct ListParams {
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

async fn list(
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

async fn get_one(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<PersonDto>, ApiError> {
    user.require(Permission::ReadData)?;
    let person = state.persons.get(tenant_of(&user)?, id).await?;
    Ok(Json(person_to_dto(&person, state.persons.today())))
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
