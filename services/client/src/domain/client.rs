//! Заказчик: корень агрегата, которому принадлежат контактные лица.
use platform_core::tenant::TenantId;
use uuid::Uuid;

use super::{DomainError, required_name, text};

const MAX_NAME: usize = 200;
const MAX_NOTE: usize = 2000;

/// Ключ уникальности названия: без регистра и лишних пробелов.
pub fn name_key(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NewClient {
    pub name: String,
    pub note: Option<String>,
}

/// Частичное изменение: `None` - не трогать, `Some(None)` - очистить.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClientPatch {
    pub name: Option<String>,
    pub note: Option<Option<String>>,
    pub archived: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Client {
    pub id: Uuid,
    pub tenant_id: TenantId,
    pub name: String,
    pub note: Option<String>,
    pub archived: bool,
}

fn note(raw: Option<String>) -> Result<Option<String>, DomainError> {
    let note = text(raw);
    match note {
        Some(n) if n.chars().count() > MAX_NOTE => Err(DomainError::invalid(
            "note",
            format!("не длиннее {MAX_NOTE} символов"),
        )),
        other => Ok(other),
    }
}

impl Client {
    pub fn create(tenant_id: TenantId, new: NewClient) -> Result<Self, DomainError> {
        Ok(Self {
            id: Uuid::new_v4(),
            tenant_id,
            name: required_name("name", &new.name, MAX_NAME)?,
            note: note(new.note)?,
            archived: false,
        })
    }

    pub fn apply(&mut self, patch: ClientPatch) -> Result<(), DomainError> {
        let mut next = self.clone();
        if let Some(name) = patch.name {
            next.name = required_name("name", &name, MAX_NAME)?;
        }
        if let Some(value) = patch.note {
            next.note = note(value)?;
        }
        if let Some(archived) = patch.archived {
            next.archived = archived;
        }
        *self = next;
        Ok(())
    }

    pub fn name_key(&self) -> String {
        name_key(&self.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tenant() -> TenantId {
        TenantId::new()
    }

    #[test]
    fn a_client_needs_a_name_and_trims_it() {
        let client = Client::create(
            tenant(),
            NewClient {
                name: "  Acme   Corp ".into(),
                note: Some("  ".into()),
            },
        )
        .unwrap();
        assert_eq!(client.name, "Acme Corp");
        assert_eq!(client.note, None);
        assert!(!client.archived);
        assert_eq!(
            Client::create(tenant(), NewClient::default()),
            Err(DomainError::invalid("name", "не может быть пустым"))
        );
    }

    #[test]
    fn too_long_values_are_rejected_with_the_field() {
        let long = "я".repeat(201);
        assert!(matches!(
            Client::create(
                tenant(),
                NewClient {
                    name: long,
                    note: None
                }
            ),
            Err(DomainError::Invalid { field: "name", .. })
        ));
        let long_note = "я".repeat(2001);
        assert!(matches!(
            Client::create(
                tenant(),
                NewClient {
                    name: "A".into(),
                    note: Some(long_note)
                }
            ),
            Err(DomainError::Invalid { field: "note", .. })
        ));
    }

    #[test]
    fn the_name_key_ignores_case_and_spaces() {
        assert_eq!(name_key("  Acme   CORP "), name_key("acme corp"));
        assert_ne!(name_key("Acme"), name_key("Acme Inc"));
    }

    #[test]
    fn a_patch_changes_only_what_it_names() {
        let mut client = Client::create(
            tenant(),
            NewClient {
                name: "Acme".into(),
                note: Some("важный".into()),
            },
        )
        .unwrap();
        client
            .apply(ClientPatch {
                archived: Some(true),
                ..Default::default()
            })
            .unwrap();
        assert!(client.archived);
        assert_eq!(client.note.as_deref(), Some("важный"));
        client
            .apply(ClientPatch {
                note: Some(None),
                name: Some("Acme 2".into()),
                archived: Some(false),
            })
            .unwrap();
        assert_eq!(client.note, None);
        assert_eq!(client.name, "Acme 2");
        assert!(!client.archived);
    }

    #[test]
    fn a_failed_patch_leaves_the_client_untouched() {
        let mut client = Client::create(
            tenant(),
            NewClient {
                name: "Acme".into(),
                note: None,
            },
        )
        .unwrap();
        let before = client.clone();
        let result = client.apply(ClientPatch {
            name: Some("   ".into()),
            archived: Some(true),
            ..Default::default()
        });
        assert!(result.is_err());
        assert_eq!(client, before);
    }
}
