use platform_auth::Role;
use platform_core::tenant::TenantId;
use uuid::Uuid;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DomainError {
    #[error("invalid email")]
    InvalidEmail,
    #[error("role {0} cannot be assigned to a tenant user")]
    InvalidRole(&'static str),
    #[error("user must have at least one role")]
    NoRoles,
    #[error("password must be at least {0} characters")]
    WeakPassword(usize),
    #[error("role name must be 1 to {0} characters")]
    InvalidRoleName(usize),
    #[error("invalid role permissions: {0}")]
    InvalidPermissions(&'static str),
    #[error("role not found")]
    UnknownRole,
    #[error("give either built-in roles or role_id, not both")]
    AmbiguousRole,
}

pub use platform_values::Email;

impl From<platform_values::ValueError> for DomainError {
    fn from(_: platform_values::ValueError) -> Self {
        DomainError::InvalidEmail
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserStatus {
    Active,
    Blocked,
}

impl UserStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            UserStatus::Active => "active",
            UserStatus::Blocked => "blocked",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "active" => Some(Self::Active),
            "blocked" => Some(Self::Blocked),
            _ => None,
        }
    }
}

/// Агрегат «пользователь тенанта».
#[derive(Debug, Clone)]
pub struct User {
    pub id: Uuid,
    pub tenant_id: TenantId,
    pub email: Email,
    pub password_hash: String,
    pub display_name: String,
    /// Встроенные роли; пусто, если назначена собственная роль.
    pub roles: Vec<Role>,
    /// Собственная роль организации (взаимоисключающе с `roles`).
    pub role_id: Option<Uuid>,
    /// Название собственной роли; заполняется при чтении.
    pub role_name: Option<String>,
    pub status: UserStatus,
    /// Когда создан; у ещё не сохранённого пользователя не задано (проставляет БД).
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl User {
    pub fn register(
        tenant_id: TenantId,
        email: Email,
        password_hash: String,
        display_name: String,
        roles: Vec<Role>,
    ) -> Result<Self, DomainError> {
        Self::validate_roles(&roles)?;
        Ok(Self {
            id: Uuid::new_v4(),
            tenant_id,
            email,
            password_hash,
            display_name,
            roles,
            role_id: None,
            role_name: None,
            status: UserStatus::Active,
            created_at: None,
        })
    }

    /// Пользователь с собственной ролью организации (без встроенных).
    pub fn register_with_role(
        tenant_id: TenantId,
        email: Email,
        password_hash: String,
        display_name: String,
        role_id: Uuid,
        role_name: String,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            email,
            password_hash,
            display_name,
            roles: Vec::new(),
            role_id: Some(role_id),
            role_name: Some(role_name),
            status: UserStatus::Active,
            created_at: None,
        }
    }

    pub const MIN_PASSWORD_LEN: usize = 8;

    pub fn validate_password(plain: &str) -> Result<(), DomainError> {
        if plain.chars().count() < Self::MIN_PASSWORD_LEN {
            Err(DomainError::WeakPassword(Self::MIN_PASSWORD_LEN))
        } else {
            Ok(())
        }
    }

    pub fn validate_roles(roles: &[Role]) -> Result<(), DomainError> {
        if roles.is_empty() {
            return Err(DomainError::NoRoles);
        }
        match roles.iter().find(|r| !r.is_tenant_role()) {
            Some(r) => Err(DomainError::InvalidRole(r.as_str())),
            None => Ok(()),
        }
    }

    pub fn can_log_in(&self) -> bool {
        self.status == UserStatus::Active
    }

    pub fn block(&mut self) {
        self.status = UserStatus::Blocked;
    }

    /// Активный владелец: тот, на ком держится управление организацией.
    pub fn is_active_owner(&self) -> bool {
        self.status == UserStatus::Active && self.roles.contains(&Role::Owner)
    }

    pub fn unblock(&mut self) {
        self.status = UserStatus::Active;
    }

    /// Назначает встроенные роли и снимает собственную.
    pub fn change_roles(&mut self, roles: Vec<Role>) -> Result<(), DomainError> {
        Self::validate_roles(&roles)?;
        self.roles = roles;
        self.role_id = None;
        self.role_name = None;
        Ok(())
    }

    /// Назначает собственную роль и снимает встроенные.
    pub fn assign_custom_role(&mut self, role_id: Uuid, role_name: String) {
        self.roles = Vec::new();
        self.role_id = Some(role_id);
        self.role_name = Some(role_name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(roles: Vec<Role>) -> Result<User, DomainError> {
        User::register(
            TenantId::new(),
            Email::parse("A@b.io").unwrap(),
            "h".into(),
            String::new(),
            roles,
        )
    }

    #[test]
    fn platform_admin_role_cannot_be_given_to_tenant_user() {
        assert_eq!(
            user(vec![Role::PlatformAdmin]).unwrap_err(),
            DomainError::InvalidRole("platform_admin")
        );
    }

    #[test]
    fn short_password_is_rejected() {
        assert_eq!(
            User::validate_password("short"),
            Err(DomainError::WeakPassword(8))
        );
        assert!(User::validate_password("long-enough").is_ok());
    }

    #[test]
    fn user_needs_a_role() {
        assert_eq!(user(vec![]).unwrap_err(), DomainError::NoRoles);
    }

    #[test]
    fn blocked_user_cannot_log_in_until_unblocked() {
        let mut u = user(vec![Role::Viewer]).unwrap();
        assert!(u.can_log_in());
        u.block();
        assert!(!u.can_log_in());
        u.unblock();
        assert!(u.can_log_in());
    }
}
