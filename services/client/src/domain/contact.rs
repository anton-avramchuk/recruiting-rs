//! Контактное лицо заказчика.
use platform_core::tenant::TenantId;
use platform_values::{Email, Phone};
use uuid::Uuid;

use super::{DomainError, required_name, text};

const MAX_NAME: usize = 200;
const MAX_NOTE: usize = 2000;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NewContact {
    pub full_name: String,
    pub position_id: Option<Uuid>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContactPatch {
    pub full_name: Option<String>,
    pub position_id: Option<Option<Uuid>>,
    pub email: Option<Option<String>>,
    pub phone: Option<Option<String>>,
    pub note: Option<Option<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contact {
    pub id: Uuid,
    pub tenant_id: TenantId,
    pub client_id: Uuid,
    pub full_name: String,
    pub position_id: Option<Uuid>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub note: Option<String>,
}

fn email(raw: Option<String>) -> Result<Option<String>, DomainError> {
    text(raw)
        .map(|v| {
            Email::parse(&v)
                .map(|e| e.as_str().to_string())
                .map_err(|_| DomainError::invalid("email", "некорректный email"))
        })
        .transpose()
}

fn phone(raw: Option<String>) -> Result<Option<String>, DomainError> {
    text(raw)
        .map(|v| {
            Phone::parse(&v)
                .map(|p| p.as_str().to_string())
                .map_err(|_| DomainError::invalid("phone", "некорректный телефон"))
        })
        .transpose()
}

fn note(raw: Option<String>) -> Result<Option<String>, DomainError> {
    match text(raw) {
        Some(n) if n.chars().count() > MAX_NOTE => Err(DomainError::invalid(
            "note",
            format!("не длиннее {MAX_NOTE} символов"),
        )),
        other => Ok(other),
    }
}

impl Contact {
    /// Контакт создаётся только внутри заказчика: `client_id` берётся у агрегата.
    pub fn create(
        tenant_id: TenantId,
        client_id: Uuid,
        new: NewContact,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            id: Uuid::new_v4(),
            tenant_id,
            client_id,
            full_name: required_name("full_name", &new.full_name, MAX_NAME)?,
            position_id: new.position_id,
            email: email(new.email)?,
            phone: phone(new.phone)?,
            note: note(new.note)?,
        })
    }

    pub fn apply(&mut self, patch: ContactPatch) -> Result<(), DomainError> {
        let mut next = self.clone();
        if let Some(name) = patch.full_name {
            next.full_name = required_name("full_name", &name, MAX_NAME)?;
        }
        if let Some(position) = patch.position_id {
            next.position_id = position;
        }
        if let Some(value) = patch.email {
            next.email = email(value)?;
        }
        if let Some(value) = patch.phone {
            next.phone = phone(value)?;
        }
        if let Some(value) = patch.note {
            next.note = note(value)?;
        }
        *self = next;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contact() -> Contact {
        Contact::create(
            TenantId::new(),
            Uuid::new_v4(),
            NewContact {
                full_name: "  Мария   Петрова ".into(),
                email: Some(" Maria@Acme.IO ".into()),
                phone: Some("8 (900) 123-45-67".into()),
                ..Default::default()
            },
        )
        .unwrap()
    }

    #[test]
    fn contacts_are_normalized_on_creation() {
        let c = contact();
        assert_eq!(c.full_name, "Мария Петрова");
        assert_eq!(c.email.as_deref(), Some("maria@acme.io"));
        assert_eq!(c.phone.as_deref(), Some("+79001234567"));
    }

    #[test]
    fn a_contact_needs_a_name_and_a_valid_email() {
        let client_id = Uuid::new_v4();
        assert_eq!(
            Contact::create(TenantId::new(), client_id, NewContact::default()),
            Err(DomainError::invalid("full_name", "не может быть пустым"))
        );
        assert_eq!(
            Contact::create(
                TenantId::new(),
                client_id,
                NewContact {
                    full_name: "А".into(),
                    email: Some("не почта".into()),
                    ..Default::default()
                }
            ),
            Err(DomainError::invalid("email", "некорректный email"))
        );
        assert!(matches!(
            Contact::create(
                TenantId::new(),
                client_id,
                NewContact {
                    full_name: "А".into(),
                    phone: Some("12".into()),
                    ..Default::default()
                }
            ),
            Err(DomainError::Invalid { field: "phone", .. })
        ));
    }

    #[test]
    fn a_blank_email_means_not_set() {
        let c = Contact::create(
            TenantId::new(),
            Uuid::new_v4(),
            NewContact {
                full_name: "А".into(),
                email: Some("   ".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(c.email, None);
    }

    #[test]
    fn a_patch_can_set_and_clear_the_position() {
        let mut c = contact();
        let position = Uuid::new_v4();
        c.apply(ContactPatch {
            position_id: Some(Some(position)),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(c.position_id, Some(position));
        assert_eq!(c.full_name, "Мария Петрова");
        c.apply(ContactPatch {
            position_id: Some(None),
            email: Some(None),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(c.position_id, None);
        assert_eq!(c.email, None);
        assert!(c.phone.is_some());
    }

    #[test]
    fn a_failed_patch_changes_nothing() {
        let mut c = contact();
        let before = c.clone();
        assert!(
            c.apply(ContactPatch {
                full_name: Some("Новое".into()),
                email: Some(Some("bad".into())),
                ..Default::default()
            })
            .is_err()
        );
        assert_eq!(c, before);
    }

    #[test]
    fn the_contact_belongs_to_the_given_client() {
        let client_id = Uuid::new_v4();
        let c = Contact::create(
            TenantId::new(),
            client_id,
            NewContact {
                full_name: "А".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(c.client_id, client_id);
    }
}
