//! DTO сервиса admin: общие для сервера, BFF и Rust-фронтенда.
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct TenantDto {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    /// `provisioning`, `active` или `suspended`.
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct TenantPage {
    pub items: Vec<TenantDto>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct OwnerRequest {
    pub email: String,
    pub password: String,
    #[serde(default)]
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CreateTenantRequest {
    pub slug: String,
    pub name: String,
    pub owner: OwnerRequest,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_display_name_is_optional() {
        let req: CreateTenantRequest = serde_json::from_str(
            r#"{"slug":"acme","name":"Acme","owner":{"email":"a@b.io","password":"x"}}"#,
        )
        .unwrap();
        assert_eq!(req.owner.display_name, "");
    }
}
