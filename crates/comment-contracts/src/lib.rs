//! Контракты сервиса comment: комментарии к заказчикам, вакансиям и кандидатам.
//! Общие для сервиса, BFF и фронтенда (в том числе wasm).
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Предел длины текста комментария в символах.
pub const MAX_BODY_CHARS: usize = 5000;

/// К чему относится комментарий.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TargetType {
    Client,
    Vacancy,
    Person,
}

impl TargetType {
    pub fn key(self) -> &'static str {
        match self {
            TargetType::Client => "client",
            TargetType::Vacancy => "vacancy",
            TargetType::Person => "person",
        }
    }

    pub fn from_key(key: &str) -> Option<TargetType> {
        [TargetType::Client, TargetType::Vacancy, TargetType::Person]
            .into_iter()
            .find(|t| t.key() == key)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CommentDto {
    pub id: Uuid,
    pub target_type: TargetType,
    pub target_id: Uuid,
    pub author_id: Uuid,
    /// Имя автора на момент записи.
    pub author_name: String,
    pub body: String,
    /// RFC 3339.
    pub created_at: String,
    /// RFC 3339; есть, если комментарий правили.
    pub edited_at: Option<String>,
}

/// Страница ленты; `next` - курсор следующей страницы (нет, если это конец).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CommentPage {
    pub items: Vec<CommentDto>,
    pub next: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CreateCommentRequest {
    /// `client`, `vacancy` или `person`.
    pub target_type: String,
    pub target_id: Uuid,
    pub body: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct UpdateCommentRequest {
    pub body: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_keys_roundtrip() {
        for t in [TargetType::Client, TargetType::Vacancy, TargetType::Person] {
            assert_eq!(TargetType::from_key(t.key()), Some(t));
            assert_eq!(serde_json::to_value(t).unwrap(), serde_json::json!(t.key()));
        }
        assert_eq!(TargetType::from_key("candidate"), None);
        assert_eq!(TargetType::from_key(""), None);
    }

    #[test]
    fn a_comment_serializes_with_optional_edit_time() {
        let dto = CommentDto {
            id: Uuid::nil(),
            target_type: TargetType::Person,
            target_id: Uuid::nil(),
            author_id: Uuid::nil(),
            author_name: "Мария".into(),
            body: "Перезвонить".into(),
            created_at: "2026-01-01T00:00:00+00:00".into(),
            edited_at: None,
        };
        let value = serde_json::to_value(&dto).unwrap();
        assert_eq!(value["target_type"], "person");
        assert_eq!(value["edited_at"], serde_json::Value::Null);
        assert_eq!(serde_json::from_value::<CommentDto>(value).unwrap(), dto);
    }
}
