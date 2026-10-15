//! Контракты сервиса client: заказчики, их контакты и справочник должностей.
//! Общие для сервиса, BFF, фронтенда (в том числе wasm) и потребителей ленты изменений.
use platform_values::double_option;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Должность из справочника организации.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct PositionDto {
    pub id: Uuid,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct PositionRequest {
    pub name: String,
}

/// Заказчик в списке.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ClientDto {
    pub id: Uuid,
    pub name: String,
    pub note: Option<String>,
    pub archived: bool,
    pub contacts_count: i64,
    /// RFC 3339.
    pub created_at: String,
}

/// Контактное лицо заказчика.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ContactDto {
    pub id: Uuid,
    pub client_id: Uuid,
    pub full_name: String,
    pub position: Option<PositionDto>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub note: Option<String>,
}

/// Карточка заказчика с контактами.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ClientDetailDto {
    pub client: ClientDto,
    pub contacts: Vec<ContactDto>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CreateClientRequest {
    pub name: String,
    #[serde(default)]
    pub note: Option<String>,
}

/// Частичное обновление: нет в теле - не трогать, `null` - очистить.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct UpdateClientRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    #[schema(value_type = Option<String>)]
    pub note: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CreateContactRequest {
    pub full_name: String,
    #[serde(default)]
    pub position_id: Option<Uuid>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct UpdateContactRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub full_name: Option<String>,
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    #[schema(value_type = Option<Uuid>)]
    pub position_id: Option<Option<Uuid>>,
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    #[schema(value_type = Option<String>)]
    pub email: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    #[schema(value_type = Option<String>)]
    pub phone: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    #[schema(value_type = Option<String>)]
    pub note: Option<Option<String>>,
}

/// Состояние заказчика в ленте изменений.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ClientSnapshot {
    pub name: String,
    pub archived: bool,
}

/// Состояние контакта в ленте: название должности уже подставлено.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ContactSnapshot {
    pub client_id: Uuid,
    pub full_name: String,
    pub position: Option<String>,
}

/// Вид сущности в ленте.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Entity {
    Client,
    Contact,
}

/// Запись ленты: последнее состояние сущности либо признак удаления.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ClientChange {
    pub seq: i64,
    pub entity: Entity,
    pub entity_id: Uuid,
    pub deleted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client: Option<ClientSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contact: Option<ContactSnapshot>,
}

/// Страница ленты; `next` - позиция, с которой читать дальше.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ClientChangesPage {
    pub items: Vec<ClientChange>,
    pub next: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn update_client_tells_missing_from_null() {
        let absent: UpdateClientRequest = serde_json::from_value(json!({"name": "A"})).unwrap();
        assert_eq!(absent.note, None);
        let cleared: UpdateClientRequest = serde_json::from_value(json!({"note": null})).unwrap();
        assert_eq!(cleared.note, Some(None));
        let set: UpdateClientRequest = serde_json::from_value(json!({"note": "x"})).unwrap();
        assert_eq!(set.note, Some(Some("x".into())));
    }

    #[test]
    fn update_contact_distinguishes_position_states() {
        let id = Uuid::new_v4();
        let set: UpdateContactRequest =
            serde_json::from_value(json!({"position_id": id.to_string()})).unwrap();
        assert_eq!(set.position_id, Some(Some(id)));
        let cleared: UpdateContactRequest =
            serde_json::from_value(json!({"position_id": null})).unwrap();
        assert_eq!(cleared.position_id, Some(None));
        assert_eq!(
            serde_json::from_value::<UpdateContactRequest>(json!({}))
                .unwrap()
                .position_id,
            None
        );
    }

    #[test]
    fn a_change_omits_the_missing_snapshots() {
        let change = ClientChange {
            seq: 3,
            entity: Entity::Contact,
            entity_id: Uuid::nil(),
            deleted: true,
            client: None,
            contact: None,
        };
        let value = serde_json::to_value(&change).unwrap();
        assert_eq!(value["entity"], "contact");
        assert!(value.get("contact").is_none() && value.get("client").is_none());
        assert_eq!(
            serde_json::from_value::<ClientChange>(value).unwrap(),
            change
        );
    }
}
