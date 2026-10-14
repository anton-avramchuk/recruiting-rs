use platform_core::tenant::TenantId;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::role::{Permission, Role};

/// Для кого выпущен токен: токены пользователей тенантов и платформенных
/// администраторов не взаимозаменяемы.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Audience {
    Tenant,
    Platform,
}

impl Audience {
    pub fn as_str(self) -> &'static str {
        match self {
            Audience::Tenant => "tenant",
            Audience::Platform => "platform",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claims {
    pub sub: Uuid,
    /// Тенант пользователя; у платформенных администраторов отсутствует.
    pub tid: Option<TenantId>,
    pub roles: Vec<Role>,
    pub aud: Audience,
    pub iat: i64,
    pub exp: i64,
}

impl Claims {
    pub fn new(
        sub: Uuid,
        tid: Option<TenantId>,
        roles: Vec<Role>,
        aud: Audience,
        ttl_secs: i64,
    ) -> Self {
        let now = now_secs();
        Self {
            sub,
            tid,
            roles,
            aud,
            iat: now,
            exp: now + ttl_secs,
        }
    }

    pub fn has(&self, permission: Permission) -> bool {
        self.roles
            .iter()
            .any(|r| r.permissions().contains(&permission))
    }
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock after epoch")
        .as_secs() as i64
}
