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
}

/// Email в нормализованном виде (trim + нижний регистр).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Email(String);

impl Email {
    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        let s = raw.trim().to_lowercase();
        match s.split_once('@') {
            Some((local, domain))
                if !local.is_empty()
                    && domain.contains('.')
                    && !domain.starts_with('.')
                    && !domain.ends_with('.')
                    && !s.contains(char::is_whitespace) =>
            {
                Ok(Self(s))
            }
            _ => Err(DomainError::InvalidEmail),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
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
    pub roles: Vec<Role>,
    pub status: UserStatus,
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
            status: UserStatus::Active,
        })
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

    pub fn unblock(&mut self) {
        self.status = UserStatus::Active;
    }

    pub fn change_roles(&mut self, roles: Vec<Role>) -> Result<(), DomainError> {
        Self::validate_roles(&roles)?;
        self.roles = roles;
        Ok(())
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
    fn email_is_normalized() {
        assert_eq!(
            Email::parse("  Anna@Acme.IO ").unwrap().as_str(),
            "anna@acme.io"
        );
    }

    #[test]
    fn invalid_emails_are_rejected() {
        for bad in ["", "no-at", "@x.io", "a@b", "a b@x.io", "a@.io", "a@x."] {
            assert_eq!(Email::parse(bad), Err(DomainError::InvalidEmail), "{bad}");
        }
    }

    #[test]
    fn platform_admin_role_cannot_be_given_to_tenant_user() {
        assert_eq!(
            user(vec![Role::PlatformAdmin]).unwrap_err(),
            DomainError::InvalidRole("platform_admin")
        );
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
