//! Раздел «Безопасность»: пользователи организации, роли, блокировка, сброс пароля (для владельца).
pub mod server;
pub mod user;
pub mod view;

/// Подпись роли в интерфейсе.
pub fn role_label(role: &str) -> &'static str {
    match role {
        "owner" => "Владелец",
        "recruiter" => "Рекрутёр",
        "viewer" => "Наблюдатель",
        _ => "Другая роль",
    }
}

#[cfg(test)]
mod tests {
    use super::role_label;

    #[test]
    fn roles_have_russian_labels() {
        assert_eq!(role_label("owner"), "Владелец");
        assert_eq!(role_label("recruiter"), "Рекрутёр");
        assert_eq!(role_label("viewer"), "Наблюдатель");
        assert_eq!(role_label("wizard"), "Другая роль");
    }
}
