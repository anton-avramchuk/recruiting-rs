//! Что пользователю можно в интерфейсе: по встроенным ролям и по правам собственной роли.
//! Окончательно права проверяет API; здесь только решается, что показывать.
use ui_kit::UserInfo;

fn has_role(user: &UserInfo, name: &str) -> bool {
    user.roles.iter().any(|r| r == name)
}

fn has_perm(user: &UserInfo, name: &str) -> bool {
    user.perms
        .as_ref()
        .is_some_and(|perms| perms.iter().any(|p| p == name))
}

/// Создавать и менять данные: владелец, рекрутёр или роль с правом записи.
pub fn can_write(user: &UserInfo) -> bool {
    has_role(user, "owner") || has_role(user, "recruiter") || has_perm(user, "write_data")
}

/// Необратимо удалять данные: владелец или роль с правом удаления.
pub fn can_delete(user: &UserInfo) -> bool {
    has_role(user, "owner") || has_perm(user, "delete_data")
}

/// Управлять пользователями и ролями (раздел «Безопасность»): только владелец.
pub fn is_owner(user: &UserInfo) -> bool {
    has_role(user, "owner")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(roles: &[&str], perms: Option<&[&str]>) -> UserInfo {
        UserInfo {
            roles: roles.iter().map(|r| (*r).to_string()).collect(),
            perms: perms.map(|p| p.iter().map(|x| (*x).to_string()).collect()),
            ..Default::default()
        }
    }

    #[test]
    fn built_in_roles_keep_their_rights() {
        let owner = user(&["owner"], None);
        assert!(can_write(&owner) && can_delete(&owner) && is_owner(&owner));
        let recruiter = user(&["recruiter"], None);
        assert!(can_write(&recruiter) && !can_delete(&recruiter) && !is_owner(&recruiter));
        let viewer = user(&["viewer"], None);
        assert!(!can_write(&viewer) && !can_delete(&viewer) && !is_owner(&viewer));
    }

    #[test]
    fn a_custom_role_gets_exactly_its_permissions() {
        let reader = user(&[], Some(&["read_data"]));
        assert!(!can_write(&reader) && !can_delete(&reader) && !is_owner(&reader));
        let editor = user(&[], Some(&["read_data", "write_data"]));
        assert!(can_write(&editor) && !can_delete(&editor));
        let cleaner = user(&[], Some(&["read_data", "write_data", "delete_data"]));
        assert!(can_write(&cleaner) && can_delete(&cleaner));
        // раздел «Безопасность» собственным ролям недоступен
        assert!(!is_owner(&cleaner));
    }

    #[test]
    fn an_empty_session_can_do_nothing() {
        let nobody = UserInfo::default();
        assert!(!can_write(&nobody) && !can_delete(&nobody) && !is_owner(&nobody));
    }
}
