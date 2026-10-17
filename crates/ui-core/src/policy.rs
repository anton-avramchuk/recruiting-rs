//! Что пользователю можно в интерфейсе: по встроенным ролям и по правам собственной роли.
//! Окончательно права проверяет API; здесь решается только, что показывать.

/// Роли и права сессии пользователя.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Policy {
    roles: Vec<String>,
    perms: Option<Vec<String>>,
}

impl Policy {
    /// `perms` - права собственной роли организации; у встроенных ролей их нет (`None`).
    pub fn new(roles: &[String], perms: Option<&[String]>) -> Self {
        Self {
            roles: roles.to_vec(),
            perms: perms.map(<[String]>::to_vec),
        }
    }

    fn has_role(&self, name: &str) -> bool {
        self.roles.iter().any(|r| r == name)
    }

    fn has_perm(&self, name: &str) -> bool {
        self.perms
            .as_ref()
            .is_some_and(|perms| perms.iter().any(|p| p == name))
    }

    /// Создавать и менять данные: владелец, рекрутёр или роль с правом записи.
    pub fn can_write(&self) -> bool {
        self.has_role("owner") || self.has_role("recruiter") || self.has_perm("write_data")
    }

    /// Необратимо удалять данные: владелец или роль с правом удаления.
    pub fn can_delete(&self) -> bool {
        self.has_role("owner") || self.has_perm("delete_data")
    }

    /// Управлять пользователями и ролями (раздел «Безопасность»): только владелец.
    pub fn is_owner(&self) -> bool {
        self.has_role("owner")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(roles: &[&str], perms: Option<&[&str]>) -> Policy {
        let roles: Vec<String> = roles.iter().map(|r| (*r).to_string()).collect();
        let perms: Option<Vec<String>> =
            perms.map(|p| p.iter().map(|x| (*x).to_string()).collect());
        Policy::new(&roles, perms.as_deref())
    }

    #[test]
    fn built_in_roles_keep_their_rights() {
        let owner = policy(&["owner"], None);
        assert!(owner.can_write() && owner.can_delete() && owner.is_owner());
        let recruiter = policy(&["recruiter"], None);
        assert!(recruiter.can_write() && !recruiter.can_delete() && !recruiter.is_owner());
        let viewer = policy(&["viewer"], None);
        assert!(!viewer.can_write() && !viewer.can_delete() && !viewer.is_owner());
    }

    #[test]
    fn a_custom_role_gets_exactly_its_permissions() {
        let reader = policy(&[], Some(&["read_data"]));
        assert!(!reader.can_write() && !reader.can_delete() && !reader.is_owner());
        let editor = policy(&[], Some(&["read_data", "write_data"]));
        assert!(editor.can_write() && !editor.can_delete());
        let cleaner = policy(&[], Some(&["read_data", "write_data", "delete_data"]));
        assert!(cleaner.can_write() && cleaner.can_delete());
        // раздел «Безопасность» собственным ролям недоступен
        assert!(!cleaner.is_owner());
    }

    #[test]
    fn an_empty_session_can_do_nothing() {
        let nobody = Policy::default();
        assert!(!nobody.can_write() && !nobody.can_delete() && !nobody.is_owner());
    }
}
