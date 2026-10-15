//! Должность из справочника организации.
use platform_core::tenant::TenantId;
use uuid::Uuid;

use super::{DomainError, name_key, required_name};

const MAX_NAME: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Position {
    pub id: Uuid,
    pub tenant_id: TenantId,
    pub name: String,
}

impl Position {
    pub fn create(tenant_id: TenantId, name: &str) -> Result<Self, DomainError> {
        Ok(Self {
            id: Uuid::new_v4(),
            tenant_id,
            name: required_name("name", name, MAX_NAME)?,
        })
    }

    pub fn rename(&mut self, name: &str) -> Result<(), DomainError> {
        self.name = required_name("name", name, MAX_NAME)?;
        Ok(())
    }

    pub fn name_key(&self) -> String {
        name_key(&self.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_position_is_named_and_can_be_renamed() {
        let mut position = Position::create(TenantId::new(), "  HR   директор ").unwrap();
        assert_eq!(position.name, "HR директор");
        position.rename("Директор по персоналу").unwrap();
        assert_eq!(position.name, "Директор по персоналу");
        assert!(position.rename(" ").is_err());
        assert_eq!(position.name, "Директор по персоналу");
    }

    #[test]
    fn the_key_ignores_case() {
        let a = Position::create(TenantId::new(), "Директор").unwrap();
        let b = Position::create(TenantId::new(), "ДИРЕКТОР ").unwrap();
        assert_eq!(a.name_key(), b.name_key());
    }

    #[test]
    fn an_empty_or_long_name_names_the_field() {
        assert_eq!(
            Position::create(TenantId::new(), ""),
            Err(DomainError::invalid("name", "не может быть пустым"))
        );
        assert!(matches!(
            Position::create(TenantId::new(), &"я".repeat(101)),
            Err(DomainError::Invalid { field: "name", .. })
        ));
    }
}
