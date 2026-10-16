use platform_core::tenant::TenantId;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::role::{Permission, Role};

/// Для кого выпущен токен: токены пользователей тенантов и платформенных
/// администраторов не взаимозаменяемы.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Audience {
    Tenant,
    Platform,
}

impl Audience {
    pub fn as_str(self) -> &'static str {
        match self {
            Audience::Tenant => "tenant",
            Audience::Platform => "platform",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claims {
    pub sub: Uuid,
    /// Тенант пользователя; у платформенных администраторов отсутствует.
    pub tid: Option<TenantId>,
    pub roles: Vec<Role>,
    /// Права собственной роли организации сверх прав встроенных ролей.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub perms: Vec<Permission>,
    pub aud: Audience,
    pub iat: i64,
    pub exp: i64,
    /// Отображаемое имя пользователя (или email); есть не во всех токенах.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl Claims {
    pub fn new(
        sub: Uuid,
        tid: Option<TenantId>,
        roles: Vec<Role>,
        aud: Audience,
        ttl_secs: i64,
    ) -> Self {
        let now = now_secs();
        Self {
            sub,
            tid,
            roles,
            perms: Vec::new(),
            aud,
            iat: now,
            exp: now + ttl_secs,
            name: None,
        }
    }

    /// Добавляет имя пользователя; пустое имя считается отсутствующим.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        let name = name.into();
        let name = name.trim();
        self.name = (!name.is_empty()).then(|| name.to_string());
        self
    }

    /// Добавляет права собственной роли.
    pub fn with_perms(mut self, perms: Vec<Permission>) -> Self {
        self.perms = perms;
        self
    }

    pub fn has(&self, permission: Permission) -> bool {
        self.perms.contains(&permission)
            || self
                .roles
                .iter()
                .any(|r| r.permissions().contains(&permission))
    }
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock after epoch")
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claims(roles: Vec<Role>, perms: Vec<Permission>) -> Claims {
        Claims::new(Uuid::new_v4(), None, roles, Audience::Tenant, 60).with_perms(perms)
    }

    #[test]
    fn built_in_roles_keep_their_rights() {
        assert!(claims(vec![Role::Owner], vec![]).has(Permission::DeleteData));
        assert!(!claims(vec![Role::Viewer], vec![]).has(Permission::WriteData));
    }

    #[test]
    fn custom_permissions_add_to_the_roles() {
        let c = claims(vec![], vec![Permission::ReadData, Permission::WriteData]);
        assert!(c.has(Permission::ReadData) && c.has(Permission::WriteData));
        assert!(!c.has(Permission::DeleteData));
        assert!(!c.has(Permission::ManageUsers));
    }

    #[test]
    fn old_tokens_without_perms_still_parse_and_empty_perms_are_not_written() {
        let old = serde_json::json!({
            "sub": Uuid::new_v4(), "tid": null, "roles": ["viewer"],
            "aud": "tenant", "iat": 1, "exp": 2
        });
        let parsed: Claims = serde_json::from_value(old).unwrap();
        assert!(parsed.perms.is_empty());
        let written = serde_json::to_value(&parsed).unwrap();
        assert!(written.get("perms").is_none());
        let with = serde_json::to_value(claims(vec![], vec![Permission::ReadData])).unwrap();
        assert_eq!(with["perms"], serde_json::json!(["read_data"]));
    }

    #[test]
    fn permission_names_round_trip() {
        for p in Permission::GRANTABLE {
            assert_eq!(Permission::parse(p.as_str()), Some(p));
        }
        assert_eq!(Permission::parse("sudo"), None);
    }
}
