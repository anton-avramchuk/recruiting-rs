//! Комментарий к объекту организации.
use comment_contracts::{MAX_BODY_CHARS, TargetType};
use platform_core::tenant::TenantId;
use uuid::Uuid;

use super::DomainError;

/// Имя автора, если в токене его не было.
pub const UNKNOWN_AUTHOR: &str = "Пользователь";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewComment {
    pub target_type: String,
    pub target_id: Uuid,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    pub id: Uuid,
    pub tenant_id: TenantId,
    pub target_type: TargetType,
    pub target_id: Uuid,
    pub author_id: Uuid,
    pub author_name: String,
    pub body: String,
}

/// Текст без пробелов по краям: от 1 до `MAX_BODY_CHARS` символов.
fn body(raw: &str) -> Result<String, DomainError> {
    let text = raw.trim();
    if text.is_empty() {
        return Err(DomainError::invalid(
            "body",
            "комментарий не может быть пустым",
        ));
    }
    if text.chars().count() > MAX_BODY_CHARS {
        return Err(DomainError::invalid(
            "body",
            format!("не длиннее {MAX_BODY_CHARS} символов"),
        ));
    }
    Ok(text.to_string())
}

impl Comment {
    pub fn create(
        tenant_id: TenantId,
        author_id: Uuid,
        author_name: Option<&str>,
        new: NewComment,
    ) -> Result<Self, DomainError> {
        let target_type = TargetType::from_key(new.target_type.trim())
            .ok_or_else(|| DomainError::invalid("target_type", "client, vacancy или person"))?;
        let author_name = author_name
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .unwrap_or(UNKNOWN_AUTHOR)
            .to_string();
        Ok(Self {
            id: Uuid::new_v4(),
            tenant_id,
            target_type,
            target_id: new.target_id,
            author_id,
            author_name,
            body: body(&new.body)?,
        })
    }

    /// Правит текст; при ошибке комментарий не меняется.
    pub fn edit(&mut self, text: &str) -> Result<(), DomainError> {
        self.body = body(text)?;
        Ok(())
    }

    /// Править может только автор.
    pub fn can_edit(&self, user: Uuid) -> bool {
        self.author_id == user
    }

    /// Удалить может автор и владелец организации.
    pub fn can_delete(&self, user: Uuid, is_owner: bool) -> bool {
        is_owner || self.author_id == user
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new(text: &str) -> NewComment {
        NewComment {
            target_type: "person".into(),
            target_id: Uuid::new_v4(),
            body: text.into(),
        }
    }

    fn comment(author: Uuid) -> Comment {
        Comment::create(TenantId::new(), author, Some("Мария"), new("Перезвонить")).unwrap()
    }

    #[test]
    fn text_is_trimmed_and_keeps_inner_newlines() {
        let c = Comment::create(
            TenantId::new(),
            Uuid::new_v4(),
            Some(" Мария Петрова "),
            new("  первая строка\nвторая  "),
        )
        .unwrap();
        assert_eq!(c.body, "первая строка\nвторая");
        assert_eq!(c.author_name, "Мария Петрова");
        assert_eq!(c.target_type, TargetType::Person);
    }

    #[test]
    fn empty_and_too_long_text_name_the_body_field() {
        for text in ["", "   \n "] {
            assert_eq!(
                Comment::create(TenantId::new(), Uuid::new_v4(), None, new(text)),
                Err(DomainError::invalid(
                    "body",
                    "комментарий не может быть пустым"
                ))
            );
        }
        let long = "я".repeat(MAX_BODY_CHARS + 1);
        assert!(matches!(
            Comment::create(TenantId::new(), Uuid::new_v4(), None, new(&long)),
            Err(DomainError::Invalid { field: "body", .. })
        ));
        // ровно предел - можно
        assert!(
            Comment::create(
                TenantId::new(),
                Uuid::new_v4(),
                None,
                new(&"я".repeat(MAX_BODY_CHARS))
            )
            .is_ok()
        );
    }

    #[test]
    fn an_unknown_target_type_is_rejected() {
        let err = Comment::create(
            TenantId::new(),
            Uuid::new_v4(),
            None,
            NewComment {
                target_type: "candidate".into(),
                ..new("x")
            },
        )
        .unwrap_err();
        assert!(matches!(
            err,
            DomainError::Invalid {
                field: "target_type",
                ..
            }
        ));
    }

    #[test]
    fn a_missing_name_gets_a_neutral_one() {
        let c = Comment::create(TenantId::new(), Uuid::new_v4(), None, new("x")).unwrap();
        assert_eq!(c.author_name, UNKNOWN_AUTHOR);
        let blank = Comment::create(TenantId::new(), Uuid::new_v4(), Some("  "), new("x")).unwrap();
        assert_eq!(blank.author_name, UNKNOWN_AUTHOR);
    }

    #[test]
    fn only_the_author_edits_and_the_owner_may_also_delete() {
        let author = Uuid::new_v4();
        let other = Uuid::new_v4();
        let c = comment(author);
        assert!(c.can_edit(author));
        assert!(!c.can_edit(other));
        assert!(c.can_delete(author, false));
        assert!(!c.can_delete(other, false));
        assert!(c.can_delete(other, true), "owner deletes any");
        assert!(!c.can_edit(other), "owner still cannot edit someone else's");
    }

    #[test]
    fn a_failed_edit_keeps_the_text() {
        let mut c = comment(Uuid::new_v4());
        assert!(c.edit("  ").is_err());
        assert_eq!(c.body, "Перезвонить");
        c.edit(" Новый текст ").unwrap();
        assert_eq!(c.body, "Новый текст");
    }
}
