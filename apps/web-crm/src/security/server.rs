//! Вызовы API пользователей организации через BFF (`/api/users`).
use grid_contracts::{GridPage, GridQuery};
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
    /// Собственная роль организации, если назначена.
    #[serde(default)]
    pub role_id: Option<String>,
    #[serde(default)]
    pub role_name: Option<String>,
    /// RFC 3339.
    #[serde(default)]
    pub created_at: Option<String>,
}

impl UserRow {
    /// Значение выбора роли: идентификатор собственной роли, код единственной встроенной или пусто.
    pub fn role_value(&self) -> String {
        match (&self.role_id, self.roles.as_slice()) {
            (Some(id), _) => id.clone(),
            (None, [one]) => one.clone(),
            _ => String::new(),
        }
    }
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

    /// Тело `PATCH /users/{id}`: встроенная роль уходит списком из одной роли, собственная - `role_id`.
    pub fn body(&self) -> Value {
        let mut body = serde_json::Map::new();
        if let Some(role) = &self.role {
            insert_role(&mut body, role);
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

/// Страница пользователей по запросу грида.
pub async fn list_users(query: GridQuery) -> Result<Reply<GridPage<UserRow>>, ServerFnError> {
    let params = query
        .to_pairs()
        .into_iter()
        .map(|(key, value)| format!("{key}={}", ui_kit::encode_query_value(&value)))
        .collect::<Vec<_>>()
        .join("&");
    let path = if params.is_empty() {
        "/api/users".to_string()
    } else {
        format!("/api/users?{params}")
    };
    let reply = ui_kit::api::call(Method::Get, &path, None).await?;
    Ok(classify(reply.status, &reply.body, typed))
}

/// Загрузчик для `Grid`: страница либо текст ошибки.
pub async fn load_users(query: GridQuery) -> Result<GridPage<UserRow>, String> {
    match list_users(query).await {
        Ok(Reply::Ok(page)) => Ok(page),
        Ok(other) => Err(other
            .failure()
            .unwrap_or_else(|| "Не удалось загрузить пользователей.".into())),
        Err(_) => Err("Не удалось загрузить пользователей. Попробуйте позже.".into()),
    }
}

pub async fn get_user(id: String) -> Result<Reply<UserRow>, ServerFnError> {
    if id.parse::<uuid_like::Id>().is_err() {
        return Ok(Reply::NotFound);
    }
    let reply = ui_kit::api::call(Method::Get, &format!("/api/users/{id}"), None).await?;
    Ok(classify(reply.status, &reply.body, typed))
}

/// Выбранная роль - идентификатор собственной роли либо код встроенной.
fn insert_role(body: &mut serde_json::Map<String, Value>, role: &str) {
    if role.parse::<uuid_like::Id>().is_ok() {
        body.insert("role_id".into(), json!(role));
    } else {
        body.insert("roles".into(), json!([role]));
    }
}

pub async fn create_user(form: NewUserForm) -> Result<Reply<UserRow>, ServerFnError> {
    let mut body = serde_json::Map::new();
    body.insert("email".into(), json!(form.email.trim()));
    body.insert("display_name".into(), json!(form.display_name.trim()));
    body.insert("password".into(), json!(form.password));
    insert_role(&mut body, &form.role);
    let reply = ui_kit::api::call(Method::Post, "/api/users", Some(Value::Object(body))).await?;
    Ok(classify(reply.status, &reply.body, typed))
}

/// Роль организации: встроенная (`kind = builtin`, `id` - код) или собственная.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleRow {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub permissions: Vec<String>,
    #[serde(default)]
    pub users: i64,
}

pub async fn list_roles() -> Result<Reply<Vec<RoleRow>>, ServerFnError> {
    let reply = ui_kit::api::call(Method::Get, "/api/roles", None).await?;
    Ok(classify(reply.status, &reply.body, typed))
}

pub async fn create_role(
    name: String,
    permissions: Vec<String>,
) -> Result<Reply<RoleRow>, ServerFnError> {
    let body = json!({"name": name.trim(), "permissions": permissions});
    let reply = ui_kit::api::call(Method::Post, "/api/roles", Some(body)).await?;
    Ok(classify(reply.status, &reply.body, typed))
}

pub async fn update_role(
    id: String,
    name: String,
    permissions: Vec<String>,
) -> Result<Reply<RoleRow>, ServerFnError> {
    if id.parse::<uuid_like::Id>().is_err() {
        return Ok(Reply::NotFound);
    }
    let body = json!({"name": name.trim(), "permissions": permissions});
    let reply = ui_kit::api::call(Method::Patch, &format!("/api/roles/{id}"), Some(body)).await?;
    Ok(classify(reply.status, &reply.body, typed))
}

pub async fn delete_role(id: String) -> Result<Reply<()>, ServerFnError> {
    if id.parse::<uuid_like::Id>().is_err() {
        return Ok(Reply::NotFound);
    }
    let reply = ui_kit::api::call(Method::Delete, &format!("/api/roles/{id}"), None).await?;
    Ok(match reply.status {
        200 | 204 => Reply::Ok(()),
        other => classify(other, &reply.body, |_| Some(())),
    })
}

/// Текст ошибки у формы роли: конфликты переводятся.
pub fn role_failure_text<T>(reply: &Reply<T>) -> Option<String> {
    match reply {
        Reply::Conflict(m) => Some(role_conflict_text(m)),
        Reply::Invalid { message, .. } => Some(role_invalid_text(message)),
        other => other.failure(),
    }
}

/// Сообщения проверки роли по-русски.
pub fn role_invalid_text(message: &str) -> String {
    if message.contains("at least one permission") || message.contains("read_data is required") {
        "Выберите хотя бы одно право; запись и удаление требуют чтения.".into()
    } else if message.contains("role name must be") {
        "Название роли: от 1 до 60 символов.".into()
    } else if message.contains("permission") {
        "Недопустимый набор прав.".into()
    } else {
        message.to_string()
    }
}

pub fn role_conflict_text(message: &str) -> String {
    if message.contains("already in use") {
        "Роль с таким названием уже есть.".into()
    } else if message.contains("reserved") {
        "Это название занято встроенной ролью.".into()
    } else if let Some(rest) = message.strip_prefix("role is assigned to ") {
        let count = rest.split_whitespace().next().unwrap_or("?");
        format!("Роль назначена пользователям ({count}). Сначала назначьте им другую роль.")
    } else {
        message.to_string()
    }
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
        m if m.contains("role not found") => "Такой роли нет.".into(),
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
    fn the_chosen_role_is_sent_as_a_built_in_code_or_as_a_custom_role_id() {
        assert_eq!(
            UserPatch::role("viewer").body(),
            json!({"roles": ["viewer"]})
        );
        let id = "11111111-2222-3333-4444-555555555555";
        assert_eq!(UserPatch::role(id).body(), json!({"role_id": id}));
    }

    #[test]
    fn the_role_value_prefers_the_custom_role() {
        let mut user = UserRow {
            id: "u".into(),
            email: "a@b.io".into(),
            display_name: String::new(),
            roles: vec!["viewer".into()],
            status: "active".into(),
            role_id: None,
            role_name: None,
            created_at: None,
        };
        assert_eq!(user.role_value(), "viewer");
        user.roles.clear();
        user.role_id = Some("rid".into());
        assert_eq!(user.role_value(), "rid");
        user.role_id = None;
        assert_eq!(user.role_value(), "");
    }

    #[test]
    fn role_validation_messages_are_translated() {
        assert!(
            role_invalid_text("invalid role permissions: at least one permission is required")
                .contains("хотя бы одно")
        );
        assert!(role_invalid_text("role name must be 1 to 60 characters").contains("1 до 60"));
        assert_eq!(role_invalid_text("что-то ещё"), "что-то ещё");
    }

    #[test]
    fn role_conflicts_are_translated() {
        assert_eq!(
            role_conflict_text("role name already in use"),
            "Роль с таким названием уже есть."
        );
        assert_eq!(
            role_conflict_text("role name is reserved"),
            "Это название занято встроенной ролью."
        );
        assert!(role_conflict_text("role is assigned to 3 users").contains("(3)"));
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
