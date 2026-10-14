use axum::{Json, Router, extract::State, http::StatusCode, routing::post};
use platform_auth::{AuthUser, Permission};
use platform_core::error::ApiError;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::AppState;
use crate::{
    application::{
        provisioning::OwnerSpec,
        tenants::{NewTenant, TenantError},
    },
    domain::Tenant,
};

pub fn routes() -> Router<AppState> {
    Router::new().route("/tenants", post(create))
}

#[derive(Serialize)]
pub struct TenantDto {
    id: Uuid,
    slug: String,
    name: String,
    status: &'static str,
}

impl From<Tenant> for TenantDto {
    fn from(t: Tenant) -> Self {
        Self {
            id: t.id.0,
            slug: t.slug.as_str().to_string(),
            name: t.name,
            status: t.status.as_str(),
        }
    }
}

#[derive(Deserialize)]
struct OwnerRequest {
    email: String,
    password: String,
    #[serde(default)]
    display_name: String,
}

#[derive(Deserialize)]
struct CreateTenantRequest {
    slug: String,
    name: String,
    owner: OwnerRequest,
}

async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<CreateTenantRequest>,
) -> Result<(StatusCode, Json<TenantDto>), ApiError> {
    user.require(Permission::ManageTenants)?;
    let tenant = state
        .tenants
        .create(NewTenant {
            slug: req.slug,
            name: req.name,
            owner: OwnerSpec {
                email: req.owner.email,
                password: req.owner.password,
                display_name: req.owner.display_name,
            },
        })
        .await?;
    Ok((StatusCode::CREATED, Json(tenant.into())))
}

impl From<TenantError> for ApiError {
    fn from(e: TenantError) -> Self {
        match e {
            TenantError::Domain(d) => ApiError::bad_request(d.to_string()),
            TenantError::Invalid(m) => ApiError::bad_request(m),
            TenantError::SlugTaken => ApiError::conflict("slug already taken"),
            TenantError::NotFound => ApiError::not_found(),
            TenantError::Provisioning(m) => {
                tracing::error!(error = %m, "tenant provisioning failed");
                ApiError::new(StatusCode::BAD_GATEWAY, "Bad Gateway")
                    .with_detail("identity service could not create the tenant owner")
            }
            TenantError::Db(e) => ApiError::internal(&e),
        }
    }
}
