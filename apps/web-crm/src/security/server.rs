//! Вызовы API пользователей организации через BFF (`/api/users`).
use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use ui_kit::api::Method;

use crate::{
    candidates::server::uuid_like,
    funnel::server::{Reply, classify, typed},
};

/// Пользователь организации в списке.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserRow {
    pub id: String,
    pub email: String,
    #[serde(default)]
    pub display_name: String,
    pub roles: Vec<String>,
    pub status: String,
}

/// Новый пользователь.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewUserForm {
    pub email: String,
    pub display_name: String,
    pub role: String,
    pub password: String,
}

/// Изменения пользователя; пустые поля не меняются.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserPatch {
    pub role: Option<String>,
    pub status: Option<String>,
    pub display_name: Option<String>,
    pub password: Option<String>,
}

impl UserPatch {
    pub fn role(role: &str) -> Self {
        Self {
            role: Some(role.into()),
            ..Self::default()
        }
    }

    pub fn status(status: &str) -> Self {
        Self {
            status: Some(status.into()),
            ..Self::default()
        }
    }

    pub fn name(name: &str) -> Self {
        Self {
            display_name: Some(name.trim().into()),
            ..Self::default()
        }
    }

    pub fn password(password: &str) -> Self {
        Self {
            password: Some(password.into()),
            ..Self::default()
        }
    }

    /// Тело `PATCH /users/{id}`: роль уходит списком из одной роли.
    pub fn body(&self) -> Value {
        let mut body = serde_json::Map::new();
        if let Some(role) = &self.role {
            body.insert("roles".into(), json!([role]));
        }
        if let Some(status) = &self.status {
            body.insert("status".into(), json!(status));
        }
        if let Some(name) = &self.display_name {
            body.insert("display_name".into(), json!(name));
        }
        if let Some(password) = &self.password {
            body.insert("password".into(), json!(password));
        }
        Value::Object(body)
    }
}

pub async fn list_users() -> Result<Reply<Vec<UserRow>>, ServerFnError> {
    let reply = ui_kit::api::call(Method::Get, "/api/users", None).await?;
    Ok(classify(reply.status, &reply.body, typed))
}

pub async fn create_user(form: NewUserForm) -> Result<Reply<UserRow>, ServerFnError> {
    let body = json!({
        "email": form.email.trim(),
        "display_name": form.display_name.trim(),
        "password": form.password,
        "roles": [form.role],
    });
    let reply = ui_kit::api::call(Method::Post, "/api/users", Some(body)).await?;
    Ok(classify(reply.status, &reply.body, typed))
}

pub async fn update_user(id: String, patch: UserPatch) -> Result<Reply<UserRow>, ServerFnError> {
    if id.parse::<uuid_like::Id>().is_err() {
        return Ok(Reply::NotFound);
    }
    let reply = ui_kit::api::call(
        Method::Patch,
        &format!("/api/users/{id}"),
        Some(patch.body()),
    )
    .await?;
    Ok(classify(reply.status, &reply.body, typed))
}

/// Текст ошибки у формы: сообщения конфликтов переводятся, остальное - общими фразами.
pub fn failure_text<T>(reply: &Reply<T>) -> Option<String> {
    match reply {
        Reply::Conflict(m) => Some(conflict_text(m)),
        other => other.failure(),
    }
}

/// Сообщения конфликтов сервиса пользователей по-русски.
pub fn conflict_text(message: &str) -> String {
    match message {
        m if m.contains("email already registered") => {
            "Пользователь с таким email уже есть.".into()
        }
        m if m.contains("cannot block yourself") => "Нельзя заблокировать самого себя.".into(),
        m if m.contains("active owner") => {
            "У организации должен остаться активный владелец.".into()
        }
        m => m.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_patch_sends_only_the_changed_fields() {
        assert_eq!(
            UserPatch::role("viewer").body(),
            json!({"roles": ["viewer"]})
        );
        assert_eq!(
            UserPatch::status("blocked").body(),
            json!({"status": "blocked"})
        );
        assert_eq!(
            UserPatch::name("  Анна ").body(),
            json!({"display_name": "Анна"})
        );
        assert_eq!(
            UserPatch::password("long-enough-1").body(),
            json!({"password": "long-enough-1"})
        );
    }

    #[test]
    fn service_conflicts_get_russian_phrases() {
        assert_eq!(
            conflict_text("email already registered"),
            "Пользователь с таким email уже есть."
        );
        assert_eq!(
            conflict_text("cannot block yourself"),
            "Нельзя заблокировать самого себя."
        );
        assert_eq!(
            conflict_text("organization must keep an active owner"),
            "У организации должен остаться активный владелец."
        );
    }

    #[test]
    fn a_conflict_reply_is_translated() {
        let reply: Reply<()> = Reply::Conflict("cannot block yourself".into());
        assert_eq!(
            failure_text(&reply).as_deref(),
            Some("Нельзя заблокировать самого себя.")
        );
    }
}
