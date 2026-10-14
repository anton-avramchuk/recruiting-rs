use axum::{Json, Router, extract::State, http::StatusCode, routing::post};
use candidate_contracts::{CreatePersonRequest, DuplicateInfo, PersonDto};
use platform_auth::{AuthUser, Permission};
use platform_core::{error::ApiError, tenant::TenantId};

use super::{AppState, mapping::person_to_dto};
use crate::{
    application::PersonError,
    domain::{ContactsInput, DomainError, NewPerson},
};

pub fn routes() -> Router<AppState> {
    Router::new().route("/persons", post(create))
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
            PersonError::Db(e) => ApiError::internal(&e),
            PersonError::Store(e) => ApiError::internal(&e),
        }
    }
}
