use platform_core::tenant::TenantId;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DomainError {
    #[error("invalid slug: {0}")]
    InvalidSlug(&'static str),
    #[error("tenant name must not be empty")]
    EmptyName,
    #[error("cannot {action} a tenant in status {from}")]
    InvalidTransition {
        action: &'static str,
        from: &'static str,
    },
}

/// Короткое имя тенанта для входа и поддомена: `[a-z0-9-]`, 2..=40 символов.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slug(String);

const RESERVED: &[&str] = &["admin", "api", "www", "app", "internal", "static"];

impl Slug {
    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        let s = raw.trim();
        if !(2..=40).contains(&s.len()) {
            return Err(DomainError::InvalidSlug("length must be 2..40"));
        }
        if !s
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        {
            return Err(DomainError::InvalidSlug(
                "only a-z, 0-9 and dash are allowed",
            ));
        }
        if s.starts_with('-') || s.ends_with('-') || s.contains("--") {
            return Err(DomainError::InvalidSlug("dashes must separate words"));
        }
        if RESERVED.contains(&s) {
            return Err(DomainError::InvalidSlug("reserved"));
        }
        Ok(Self(s.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TenantStatus {
    Provisioning,
    Active,
    Suspended,
}

impl TenantStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Provisioning => "provisioning",
            Self::Active => "active",
            Self::Suspended => "suspended",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "provisioning" => Some(Self::Provisioning),
            "active" => Some(Self::Active),
            "suspended" => Some(Self::Suspended),
            _ => None,
        }
    }
}

/// Агрегат «тенант». Переходы: provisioning → active ⇄ suspended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tenant {
    pub id: TenantId,
    pub slug: Slug,
    pub name: String,
    pub status: TenantStatus,
}

impl Tenant {
    pub fn begin_provisioning(slug: Slug, name: &str) -> Result<Self, DomainError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(DomainError::EmptyName);
        }
        Ok(Self {
            id: TenantId::new(),
            slug,
            name: name.to_string(),
            status: TenantStatus::Provisioning,
        })
    }

    pub fn complete_provisioning(&mut self) -> Result<(), DomainError> {
        self.transition(
            "complete provisioning of",
            TenantStatus::Provisioning,
            TenantStatus::Active,
        )
    }

    pub fn suspend(&mut self) -> Result<(), DomainError> {
        self.transition("suspend", TenantStatus::Active, TenantStatus::Suspended)
    }

    pub fn activate(&mut self) -> Result<(), DomainError> {
        self.transition("activate", TenantStatus::Suspended, TenantStatus::Active)
    }

    fn transition(
        &mut self,
        action: &'static str,
        from: TenantStatus,
        to: TenantStatus,
    ) -> Result<(), DomainError> {
        if self.status != from {
            return Err(DomainError::InvalidTransition {
                action,
                from: self.status.as_str(),
            });
        }
        self.status = to;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tenant() -> Tenant {
        Tenant::begin_provisioning(Slug::parse("acme").unwrap(), "Acme").unwrap()
    }

    #[test]
    fn valid_slugs() {
        for ok in ["ab", "acme", "acme-corp", "a1-b2", &"x".repeat(40)] {
            assert!(Slug::parse(ok).is_ok(), "{ok}");
        }
    }

    #[test]
    fn invalid_slugs() {
        for bad in [
            "",
            "a",
            "Acme",
            "a b",
            "-a",
            "a-",
            "a--b",
            "admin",
            "api",
            "ёж",
            &"x".repeat(41),
        ] {
            assert!(Slug::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn new_tenant_starts_in_provisioning() {
        assert_eq!(tenant().status, TenantStatus::Provisioning);
        assert_eq!(
            Tenant::begin_provisioning(Slug::parse("acme").unwrap(), "  "),
            Err(DomainError::EmptyName)
        );
    }

    #[test]
    fn lifecycle_follows_the_allowed_transitions() {
        let mut t = tenant();
        assert!(
            t.suspend().is_err(),
            "cannot suspend before provisioning completes"
        );
        t.complete_provisioning().unwrap();
        assert!(t.complete_provisioning().is_err());
        assert!(t.activate().is_err(), "already active");
        t.suspend().unwrap();
        assert!(t.suspend().is_err(), "already suspended");
        t.activate().unwrap();
        assert_eq!(t.status, TenantStatus::Active);
    }
}
