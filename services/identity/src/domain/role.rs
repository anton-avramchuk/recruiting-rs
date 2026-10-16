use platform_auth::{Permission, Role};
use platform_core::tenant::TenantId;
use uuid::Uuid;

use super::DomainError;

/// Собственная роль организации: название и набор прав из `Permission::GRANTABLE`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomRole {
    pub id: Uuid,
    pub tenant_id: TenantId,
    pub name: String,
    pub permissions: Vec<Permission>,
}

impl CustomRole {
    pub const MAX_NAME_CHARS: usize = 60;

    pub fn create(
        tenant_id: TenantId,
        name: &str,
        permissions: &[String],
    ) -> Result<Self, DomainError> {
        Ok(Self {
            id: Uuid::new_v4(),
            tenant_id,
            name: Self::validate_name(name)?,
            permissions: Self::validate_permissions(permissions)?,
        })
    }

    pub fn rename(&mut self, name: &str) -> Result<(), DomainError> {
        self.name = Self::validate_name(name)?;
        Ok(())
    }

    /// Меняет набор прав; возвращает, изменился ли он.
    pub fn set_permissions(&mut self, permissions: &[String]) -> Result<bool, DomainError> {
        let next = Self::validate_permissions(permissions)?;
        let changed = next != self.permissions;
        self.permissions = next;
        Ok(changed)
    }

    /// Ключ уникальности названия: без пробелов по краям и без учёта регистра.
    pub fn name_key(name: &str) -> String {
        name.trim().to_lowercase()
    }

    pub fn key(&self) -> String {
        Self::name_key(&self.name)
    }

    /// Совпадает ли название с названием или кодом встроенной роли.
    pub fn is_reserved(name: &str) -> bool {
        let key = Self::name_key(name);
        BUILT_IN
            .iter()
            .any(|b| b.code == key || b.title.to_lowercase() == key)
    }

    fn validate_name(name: &str) -> Result<String, DomainError> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > Self::MAX_NAME_CHARS {
            return Err(DomainError::InvalidRoleName(Self::MAX_NAME_CHARS));
        }
        Ok(name.to_string())
    }

    /// Права в каноническом порядке без повторов; запись и удаление требуют чтения.
    fn validate_permissions(raw: &[String]) -> Result<Vec<Permission>, DomainError> {
        let mut parsed = Vec::new();
        for item in raw {
            let permission = Permission::parse(item)
                .filter(|p| Permission::GRANTABLE.contains(p))
                .ok_or(DomainError::InvalidPermissions(
                    "unknown or not grantable permission",
                ))?;
            parsed.push(permission);
        }
        if parsed.is_empty() {
            return Err(DomainError::InvalidPermissions(
                "at least one permission is required",
            ));
        }
        let ordered: Vec<Permission> = Permission::GRANTABLE
            .into_iter()
            .filter(|p| parsed.contains(p))
            .collect();
        if !ordered.contains(&Permission::ReadData) {
            return Err(DomainError::InvalidPermissions("read_data is required"));
        }
        Ok(ordered)
    }
}

/// Встроенная роль в списке ролей.
pub struct BuiltIn {
    pub role: Role,
    pub code: &'static str,
    pub title: &'static str,
}

/// Встроенные роли организации в порядке показа.
pub const BUILT_IN: [BuiltIn; 3] = [
    BuiltIn {
        role: Role::Owner,
        code: "owner",
        title: "Владелец",
    },
    BuiltIn {
        role: Role::Recruiter,
        code: "recruiter",
        title: "Рекрутёр",
    },
    BuiltIn {
        role: Role::Viewer,
        code: "viewer",
        title: "Наблюдатель",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    fn perms(p: &[&str]) -> Vec<String> {
        p.iter().map(|s| (*s).to_string()).collect()
    }

    fn make(name: &str, p: &[&str]) -> Result<CustomRole, DomainError> {
        CustomRole::create(TenantId::new(), name, &perms(p))
    }

    #[test]
    fn a_role_gets_a_trimmed_name_and_ordered_unique_permissions() {
        let role = make("  Стажёр ", &["write_data", "read_data", "write_data"]).unwrap();
        assert_eq!(role.name, "Стажёр");
        assert_eq!(
            role.permissions,
            vec![Permission::ReadData, Permission::WriteData]
        );
    }

    #[test]
    fn bad_names_are_rejected() {
        assert!(make("  ", &["read_data"]).is_err());
        assert!(make(&"я".repeat(61), &["read_data"]).is_err());
        assert!(make(&"я".repeat(60), &["read_data"]).is_ok());
    }

    #[test]
    fn permissions_must_be_grantable_and_include_reading() {
        for bad in [
            vec![],
            vec!["write_data"],
            vec!["delete_data"],
            vec!["write_data", "delete_data"],
            vec!["read_data", "manage_users"],
            vec!["read_data", "manage_tenants"],
            vec!["read_data", "sudo"],
        ] {
            assert!(make("R", &bad).is_err(), "{bad:?}");
        }
        assert!(make("R", &["read_data", "write_data", "delete_data"]).is_ok());
    }

    #[test]
    fn set_permissions_reports_whether_anything_changed() {
        let mut role = make("R", &["read_data", "write_data"]).unwrap();
        assert!(
            !role
                .set_permissions(&perms(&["write_data", "read_data"]))
                .unwrap()
        );
        assert!(role.set_permissions(&perms(&["read_data"])).unwrap());
        assert!(role.set_permissions(&perms(&["write_data"])).is_err());
        assert_eq!(role.permissions, vec![Permission::ReadData]);
    }

    #[test]
    fn built_in_names_and_codes_are_reserved_in_any_case() {
        for name in ["Владелец", "рекрутёр", " НАБЛЮДАТЕЛЬ ", "owner", "Viewer"]
        {
            assert!(CustomRole::is_reserved(name), "{name}");
        }
        assert!(!CustomRole::is_reserved("Стажёр"));
    }
}
