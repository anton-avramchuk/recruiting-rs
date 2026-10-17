//! Раздел «Безопасность»: пользователи организации, роли, блокировка, сброс пароля (для владельца).
pub mod roles;
pub mod server;
pub mod user;
pub mod view;

/// Подпись права в интерфейсе.
pub fn permission_label(permission: &str) -> &'static str {
    match permission {
        "read_data" => "Чтение",
        "write_data" => "Запись",
        "delete_data" => "Удаление",
        "manage_users" => "Управление пользователями",
        _ => "Другое право",
    }
}

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
    use super::{permission_label, role_label};

    #[test]
    fn permissions_have_russian_labels() {
        assert_eq!(permission_label("read_data"), "Чтение");
        assert_eq!(permission_label("write_data"), "Запись");
        assert_eq!(permission_label("delete_data"), "Удаление");
    }

    #[test]
    fn roles_have_russian_labels() {
        assert_eq!(role_label("owner"), "Владелец");
        assert_eq!(role_label("recruiter"), "Рекрутёр");
        assert_eq!(role_label("viewer"), "Наблюдатель");
        assert_eq!(role_label("wizard"), "Другая роль");
    }
}
